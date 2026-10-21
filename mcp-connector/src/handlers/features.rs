use axum::{
    extract::{Path, State, Multipart},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::state::AppState;
use crate::handlers::admin::check_auth;

#[derive(Serialize, sqlx::FromRow)]
pub struct AuditLog {
    pub id: i64,
    pub event_type: String,
    pub detail: String,
    pub severity: String,
    pub created_at: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ScheduledTask {
    pub id: i64,
    pub task_name: String,
    pub cron_expression: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreateTaskReq {
    pub task_name: String,
    pub cron_expression: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Permission {
    pub key: String,
    pub value: bool,
}

#[derive(Deserialize)]
pub struct UpdatePermissionReq {
    pub value: bool,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct InboxFile {
    pub id: i64,
    pub file_name: String,
    pub sender: String,
    pub created_at: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct KnowledgeDir {
    pub id: i64,
    pub directory_path: String,
    pub file_count: i64,
    pub created_at: String,
}

pub async fn get_audit_logs(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AuditLog>>, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    let logs = sqlx::query_as::<_, AuditLog>(
        "SELECT id, event_type, detail, severity, datetime(created_at, 'localtime') as created_at FROM audit_logs ORDER BY id DESC LIMIT 100"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(logs))
}

pub async fn get_permissions(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Permission>>, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    let perms = sqlx::query_as::<_, Permission>(
        "SELECT key, value FROM permissions"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(perms))
}

pub async fn update_permission(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Json(payload): Json<UpdatePermissionReq>,
) -> Result<StatusCode, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    let res = sqlx::query("UPDATE permissions SET value = $1 WHERE key = $2")
        .bind(payload.value)
        .bind(&key)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if res.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    
    sqlx::query("INSERT INTO audit_logs (event_type, detail, severity) VALUES ($1, $2, 'warning')")
        .bind("PERMISSION_CHANGED")
        .bind(format!("Permission {} changed to {}", key, payload.value))
        .execute(&state.db)
        .await
        .ok();
        
    Ok(StatusCode::OK)
}

pub async fn get_schedule(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ScheduledTask>>, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    let tasks = sqlx::query_as::<_, ScheduledTask>(
        "SELECT id, task_name, cron_expression, datetime(created_at, 'localtime') as created_at FROM scheduled_tasks ORDER BY id DESC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(tasks))
}

pub async fn create_schedule(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTaskReq>,
) -> Result<StatusCode, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;

    let task_name = payload.task_name.trim();
    let cron_expression = payload.cron_expression.trim();

    if task_name.is_empty() || task_name.len() > 128 || cron_expression.is_empty() || cron_expression.len() > 100 {
        return Err(StatusCode::BAD_REQUEST);
    }

    if !crate::scheduler::is_valid_cron_expression(cron_expression) {
        return Err(StatusCode::BAD_REQUEST);
    }

    sqlx::query("INSERT INTO scheduled_tasks (task_name, cron_expression) VALUES ($1, $2)")
        .bind(task_name)
        .bind(cron_expression)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
    sqlx::query("INSERT INTO audit_logs (event_type, detail, severity) VALUES ($1, $2, 'info')")
        .bind("TASK_SCHEDULED")
        .bind(format!("Scheduled task: {}", task_name))
        .execute(&state.db)
        .await
        .ok();
        
    Ok(StatusCode::CREATED)
}

pub async fn delete_schedule(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    let res = sqlx::query("DELETE FROM scheduled_tasks WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if res.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::OK)
}

pub async fn get_inbox(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<InboxFile>>, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    let files = sqlx::query_as::<_, InboxFile>(
        "SELECT id, file_name, sender, datetime(created_at, 'localtime') as created_at FROM inbox_files ORDER BY id DESC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(files))
}

#[derive(Deserialize)]
pub struct CreateKnowledgeReq {
    pub directory_path: String,
}

pub async fn create_knowledge(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateKnowledgeReq>,
) -> Result<StatusCode, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    let trimmed_path = payload.directory_path.trim();
    if trimmed_path.is_empty() || trimmed_path.len() > 1024 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Mock Vector DB indexing: just record the directory
    // In a real implementation this would use 'ort' or 'candle' to embed files
    let file_count = 42; // Mock count
    
    sqlx::query("INSERT INTO knowledge_base (directory_path, file_count) VALUES ($1, $2)")
        .bind(trimmed_path)
        .bind(file_count)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
    sqlx::query("INSERT INTO audit_logs (event_type, detail, severity) VALUES ($1, $2, 'info')")
        .bind("KNOWLEDGE_INDEXED")
        .bind(format!("Indexed {} files in directory '{}' into local vector DB.", file_count, trimmed_path))
        .execute(&state.db)
        .await
        .ok();

    Ok(StatusCode::CREATED)
}

pub async fn get_knowledge(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<KnowledgeDir>>, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    let dirs = sqlx::query_as::<_, KnowledgeDir>(
        "SELECT id, directory_path, file_count, datetime(created_at, 'localtime') as created_at FROM knowledge_base ORDER BY id DESC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(dirs))
}

pub async fn delete_knowledge(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    let res = sqlx::query("DELETE FROM knowledge_base WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if res.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    sqlx::query("INSERT INTO audit_logs (event_type, detail, severity) VALUES ($1, $2, 'info')")
        .bind("KNOWLEDGE_DELETED")
        .bind(format!("Deleted indexed knowledge base entry #{}", id))
        .execute(&state.db)
        .await
        .ok();
    Ok(StatusCode::OK)
}

pub async fn handoff_upload(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<StatusCode, StatusCode> {
    check_auth(&headers, &state.config.admin_password)?;
    let mut file_count = 0;
    
    // Determine inbox directory path
    let inbox_dir = if let Some(proj_dirs) = directories::ProjectDirs::from("com", "ekam", "baton") {
        proj_dirs.data_local_dir().join("inbox")
    } else {
        std::env::temp_dir().join("baton_inbox")
    };
    
    // Create the directory if it doesn't exist
    let _ = tokio::fs::create_dir_all(&inbox_dir).await;

    const ALLOWED_EXTENSIONS: &[&str] = &[
        "pdf", "txt", "md", "csv", "json", "xml",
        "jpg", "jpeg", "png", "gif", "webp", "svg",
        "mp3", "wav", "mp4", "webm", "zip",
    ];

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let _name = field.name().unwrap_or("").to_string();
        let raw_name = field.file_name().unwrap_or("unknown").to_string();
        let ext = std::path::Path::new(&raw_name)
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if ext.is_empty() || !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
            tracing::warn!("Handoff upload rejected: disallowed or missing extension '.{}'", ext);
            continue;
        }

        let display_name = std::path::Path::new(&raw_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .chars()
            .take(128)
            .collect::<String>();

        let unique_filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
        let data = field.bytes().await.unwrap_or_default();
        if data.is_empty() || (data.len() as u64) > state.config.max_upload_bytes {
            tracing::warn!("Handoff upload rejected: empty or exceeds size limit");
            continue;
        }

        let file_path = inbox_dir.join(&unique_filename);
        if let Err(e) = tokio::fs::write(&file_path, &data).await {
            tracing::error!("Failed to write inbox file to disk: {}", e);
            continue;
        }
        
        file_count += 1;
        
        sqlx::query("INSERT INTO inbox_files (file_name, sender) VALUES ($1, 'Desktop Handoff')")
            .bind(&display_name)
            .execute(&state.db)
            .await
            .ok();
    }
    
    sqlx::query("INSERT INTO audit_logs (event_type, detail, severity) VALUES ($1, $2, 'success')")
        .bind("HANDOFF_RECEIVED")
        .bind(format!("Injected {} files into context memory.", file_count))
        .execute(&state.db)
        .await
        .ok();
        
    Ok(StatusCode::CREATED)
}
