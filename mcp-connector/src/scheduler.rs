use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{info, error};
use crate::state::AppState;

pub async fn start_scheduler(state: Arc<AppState>) {
    info!("Starting background cron scheduler...");
    
    // Poll every 60 seconds
    let mut interval = time::interval(Duration::from_secs(60));
    
    loop {
        interval.tick().await;
        
        // Fetch all tasks
        let tasks = match sqlx::query(
            "SELECT id, task_name, cron_expression FROM scheduled_tasks"
        )
        .fetch_all(&state.db)
        .await
        {
            Ok(t) => t,
            Err(e) => {
                error!("Scheduler failed to fetch tasks: {}", e);
                continue;
            }
        };
        
        for row in tasks {
            use sqlx::Row;
            let task_name: String = row.get("task_name");
            let cron_expression: String = row.get("cron_expression");

            info!("Scheduler executing task (mock): {} (Cron: {})", task_name, cron_expression);
            
            // Log it in audit log
            sqlx::query("INSERT INTO audit_logs (event_type, detail, severity) VALUES ($1, $2, 'info')")
                .bind("TASK_EXECUTED")
                .bind(format!("Automatically executed scheduled task: {}", task_name))
                .execute(&state.db)
                .await
                .ok();
        }
    }
}
