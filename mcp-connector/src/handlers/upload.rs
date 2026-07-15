use std::{path::PathBuf, sync::Arc};
use axum::{
    extract::{Multipart, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use tokio::{fs::File, io::AsyncWriteExt};
use uuid::Uuid;
use tracing::{info, warn};

use crate::auth::validate_access_token;
use crate::state::AppState;

/// Allowed file extensions (lowercase). Reject anything not on this list.
const ALLOWED_EXTENSIONS: &[&str] = &[
    // Documents
    "pdf", "txt", "md", "csv", "json", "xml",
    // Images
    "jpg", "jpeg", "png", "gif", "webp", "svg",
    // Audio / Video
    "mp3", "wav", "mp4", "webm",
    // Archives (benign)
    "zip",
];

pub async fn upload_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if let Err(e) = validate_access_token(&headers, &state.config.jwt_secret) {
        return e.into_response();
    }

    let upload_dir = PathBuf::from(&state.config.upload_dir);
    if !upload_dir.exists() {
        if let Err(e) = tokio::fs::create_dir_all(&upload_dir).await {
            warn!("Failed to create upload dir: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Server configuration error"})),
            ).into_response();
        }
    }

    let mut uploaded_files = Vec::new();

    while let Some(field) = match multipart.next_field().await {
        Ok(Some(f)) => Some(f),
        Ok(None) => None,
        Err(e) => {
            warn!("Multipart parse error: {}", e);
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid multipart data"}))).into_response();
        }
    } {
        let name = field.name().unwrap_or("unknown").to_string();
        let filename = field.file_name().unwrap_or("file").to_string();
        let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
        
        let data = match field.bytes().await {
            Ok(d) => d,
            Err(_) => continue,
        };

        if data.len() as u64 > state.config.max_upload_bytes {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"error": "File exceeds maximum size limit"})),
            ).into_response();
        }

        let file_id = Uuid::new_v4().to_string();
        let ext = std::path::Path::new(&filename)
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        
        // Extension allowlist — reject executables and unknown types
        if !ext.is_empty() && !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
            warn!("Upload rejected: disallowed extension '.{}' from client", ext);
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error": format!("File type '.{}' is not allowed", ext)}))
            ).into_response();
        }
        
        let safe_filename = if ext.is_empty() {
            file_id.clone()
        } else {
            format!("{}.{}", file_id, ext)
        };

        let filepath = upload_dir.join(&safe_filename);
        let mut file = match File::create(&filepath).await {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to create file {:?}: {}", filepath, e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "File IO error"}))).into_response();
            }
        };

        if let Err(e) = file.write_all(&data).await {
            warn!("Failed to write file {:?}: {}", filepath, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "File IO error"}))).into_response();
        }

        state.upload_bytes.with_label_values(&[&content_type]).inc_by(data.len() as f64);
        
        info!("File uploaded: {} ({} bytes)", safe_filename, data.len());

        // Never reflect the raw client filename back — could enable downstream XSS.
        // Instead, return only the safe UUID-based filename and metadata we control.
        uploaded_files.push(json!({
            "field": name,
            "file_id": file_id,
            "url": format!("/uploads/{}", safe_filename),
            "size": data.len(),
            "content_type": content_type
        }));
    }

    (
        StatusCode::OK,
        Json(json!({
            "status": "success",
            "files": uploaded_files
        })),
    ).into_response()
}
