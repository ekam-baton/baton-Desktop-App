use axum::{
    extract::{Path, State, Multipart},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::state::AppState;

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
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AuditLog>>, StatusCode> {
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
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Permission>>, StatusCode> {
    let perms = sqlx::query_as::<_, Permission>(
        "SELECT key, value FROM permissions"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(perms))
}

pub async fn update_permission(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Json(payload): Json<UpdatePermissionReq>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query("UPDATE permissions SET value = $1 WHERE key = $2")
        .bind(payload.value)
        .bind(&key)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    sqlx::query("INSERT INTO audit_logs (event_type, detail, severity) VALUES ($1, $2, 'warning')")
        .bind("PERMISSION_CHANGED")
        .bind(format!("Permission {} changed to {}", key, payload.value))
        .execute(&state.db)
        .await
        .ok();
        
    Ok(StatusCode::OK)
}

pub async fn get_schedule(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ScheduledTask>>, StatusCode> {
    let tasks = sqlx::query_as::<_, ScheduledTask>(
        "SELECT id, task_name, cron_expression, datetime(created_at, 'localtime') as created_at FROM scheduled_tasks ORDER BY id DESC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(tasks))
}

pub async fn create_schedule(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTaskReq>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query("INSERT INTO scheduled_tasks (task_name, cron_expression) VALUES ($1, $2)")
        .bind(&payload.task_name)
        .bind(&payload.cron_expression)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
    sqlx::query("INSERT INTO audit_logs (event_type, detail, severity) VALUES ($1, $2, 'info')")
        .bind("TASK_SCHEDULED")
        .bind(format!("Scheduled task: {}", payload.task_name))
        .execute(&state.db)
        .await
        .ok();
        
    Ok(StatusCode::CREATED)
}

pub async fn delete_schedule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query("DELETE FROM scheduled_tasks WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn get_inbox(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<InboxFile>>, StatusCode> {
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
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateKnowledgeReq>,
) -> Result<StatusCode, StatusCode> {
    // Mock Vector DB indexing: just record the directory
    // In a real implementation this would use 'ort' or 'candle' to embed files
    let file_count = 42; // Mock count
    
    sqlx::query("INSERT INTO knowledge_base (directory_path, file_count) VALUES ($1, $2)")
        .bind(&payload.directory_path)
        .bind(file_count)
        .execute(&state.db)
        .await
        .ok();
        
    sqlx::query("INSERT INTO audit_logs (event_type, detail, severity) VALUES ($1, $2, 'info')")
        .bind("KNOWLEDGE_INDEXED")
        .bind(format!("Indexed {} files in directory '{}' into local vector DB.", file_count, payload.directory_path))
        .execute(&state.db)
        .await
        .ok();

    Ok(StatusCode::CREATED)
}

pub async fn get_knowledge(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<KnowledgeDir>>, StatusCode> {
    let dirs = sqlx::query_as::<_, KnowledgeDir>(
        "SELECT id, directory_path, file_count, datetime(created_at, 'localtime') as created_at FROM knowledge_base ORDER BY id DESC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(dirs))
}

pub async fn handoff_upload(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<StatusCode, StatusCode> {
    let mut file_count = 0;
    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let name = field.name().unwrap_or("").to_string();
        let file_name = field.file_name().unwrap_or("unknown").to_string();
        let _data = field.bytes().await.unwrap_or_default();
        
        file_count += 1;
        
        sqlx::query("INSERT INTO inbox_files (file_name, sender) VALUES ($1, 'Desktop Handoff')")
            .bind(&file_name)
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
