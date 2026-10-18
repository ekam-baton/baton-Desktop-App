use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{info, error};
use chrono::{Datelike, Timelike};
use crate::state::AppState;

pub fn matches_cron_field(field: &str, current: u32, min_val: u32, max_val: u32) -> bool {
    let field = field.trim();
    if field == "*" {
        return true;
    }
    if let Some(step_str) = field.strip_prefix("*/") {
        if let Ok(step) = step_str.parse::<u32>() {
            if step > 0 && (current % step == 0) {
                return true;
            }
        }
        return false;
    }
    for part in field.split(',') {
        let part = part.trim();
        if let Some((start_s, end_s)) = part.split_once('-') {
            if let (Ok(start), Ok(end)) = (start_s.parse::<u32>(), end_s.parse::<u32>()) {
                if current >= start && current <= end && start <= end {
                    return true;
                }
            }
        } else if let Ok(val) = part.parse::<u32>() {
            if val == current && val >= min_val && val <= max_val {
                return true;
            }
        }
    }
    false
}

pub fn is_cron_due(expr: &str, now: &chrono::DateTime<chrono::Local>) -> bool {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return false;
    }
    let min = now.minute();
    let hour = now.hour();
    let dom = now.day();
    let month = now.month();
    let dow = now.weekday().num_days_from_sunday(); // 0 = Sun, 6 = Sat

    matches_cron_field(parts[0], min, 0, 59)
        && matches_cron_field(parts[1], hour, 0, 23)
        && matches_cron_field(parts[2], dom, 1, 31)
        && matches_cron_field(parts[3], month, 1, 12)
        && matches_cron_field(parts[4], dow, 0, 6)
}

pub async fn start_scheduler(state: Arc<AppState>) {
    info!("Starting background cron scheduler...");
    
    // Poll every 60 seconds, starting after the first 60s delay
    let mut interval = time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(60),
        Duration::from_secs(60),
    );
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
    
    loop {
        interval.tick().await;
        
        let now = chrono::Local::now();

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
            let task_name: String = match row.try_get("task_name") {
                Ok(name) => name,
                Err(e) => {
                    error!("Scheduler invalid task_name: {}", e);
                    continue;
                }
            };
            let cron_expression: String = match row.try_get("cron_expression") {
                Ok(cron) => cron,
                Err(e) => {
                    error!("Scheduler invalid cron_expression: {}", e);
                    continue;
                }
            };

            if !is_cron_due(&cron_expression, &now) {
                continue;
            }

            info!("Scheduler executing task: {} (Cron: {})", task_name, cron_expression);
            
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_wildcard_matching() {
        assert!(matches_cron_field("*", 30, 0, 59));
        assert!(matches_cron_field("*/15", 30, 0, 59));
        assert!(!matches_cron_field("*/15", 31, 0, 59));
        assert!(matches_cron_field("10,20,30", 30, 0, 59));
        assert!(!matches_cron_field("10,20,40", 30, 0, 59));
        assert!(matches_cron_field("25-35", 30, 0, 59));
        assert!(!matches_cron_field("35-45", 30, 0, 59));
    }

    #[test]
    fn test_is_cron_due_format() {
        let now = chrono::Local::now();
        // Wildcard always matches
        assert!(is_cron_due("* * * * *", &now));
        // Wrong number of fields fails
        assert!(!is_cron_due("* * * *", &now));
        assert!(!is_cron_due("* * * * * *", &now));
    }
}

