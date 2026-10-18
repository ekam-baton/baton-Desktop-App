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

use crate::auth::validate_client_authorization;
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
    if let Err(e) = validate_client_authorization(&headers, &state, None).await {
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

    while let Some(mut field) = match multipart.next_field().await {
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

        let file_id = Uuid::new_v4().to_string();
        let ext = std::path::Path::new(&filename)
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        
        // Extension allowlist — reject executables, unknown types, and extensionless files
        if ext.is_empty() || !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
            warn!("Upload rejected: disallowed or missing extension '.{}' from client", ext);
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error": format!("File type '.{}' is not allowed", ext)}))
            ).into_response();
        }
        
        let safe_filename = format!("{}.{}", file_id, ext);

        let filepath = upload_dir.join(&safe_filename);
        let mut file = match File::create(&filepath).await {
            Ok(f) => f,
            Err(e) => {
                warn!("Failed to create file {:?}: {}", filepath, e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "File IO error"}))).into_response();
            }
        };

        // Stream chunks directly from socket to disk without heap buffering
        let mut total_bytes: u64 = 0;
        let mut upload_exceeded = false;
        while let Some(chunk) = match field.chunk().await {
            Ok(c) => c,
            Err(e) => {
                warn!("Error reading multipart chunk: {}", e);
                let _ = tokio::fs::remove_file(&filepath).await;
                return (StatusCode::BAD_REQUEST, Json(json!({"error": "Failed reading upload chunk"}))).into_response();
            }
        } {
            total_bytes += chunk.len() as u64;
            if total_bytes > state.config.max_upload_bytes {
                upload_exceeded = true;
                break;
            }
            if let Err(e) = file.write_all(&chunk).await {
                warn!("Failed to write chunk to {:?}: {}", filepath, e);
                let _ = tokio::fs::remove_file(&filepath).await;
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "File IO error"}))).into_response();
            }
        }

        if upload_exceeded {
            let _ = tokio::fs::remove_file(&filepath).await;
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({"error": "File exceeds maximum size limit"})),
            ).into_response();
        }

        if let Err(e) = file.flush().await {
            warn!("Failed to flush file {:?}: {}", filepath, e);
            let _ = tokio::fs::remove_file(&filepath).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "File IO error"}))).into_response();
        }

        state.upload_bytes.with_label_values(&[&content_type]).inc_by(total_bytes as f64);
        
        info!("File uploaded: {} ({} bytes)", safe_filename, total_bytes);

        // Never reflect the raw client filename back — could enable downstream XSS.
        // Instead, return only the safe UUID-based filename and metadata we control.
        uploaded_files.push(json!({
            "field": name,
            "file_id": file_id,
            "url": format!("/uploads/{}", safe_filename),
            "size": total_bytes,
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
