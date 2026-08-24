// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

use std::collections::HashMap;

fn get_env_path() -> std::path::PathBuf {
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "ekam", "baton") {
        proj_dirs.data_dir().join(".env")
    } else {
        std::path::PathBuf::from(".env")
    }
}

#[tauri::command]
fn get_env() -> Result<HashMap<String, String>, String> {
    let path = get_env_path();
    let mut map = HashMap::new();
    if let Ok(content) = std::fs::read_to_string(&path) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    Ok(map)
}

#[tauri::command]
fn set_env(updates: HashMap<String, String>) -> Result<(), String> {
    let path = get_env_path();
    let mut lines = Vec::new();
    
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    if let Ok(content) = std::fs::read_to_string(&path) {
        lines = content.lines().map(|s| s.to_string()).collect();
    }

    for (k, v) in updates {
        let mut found = false;
        for line in lines.iter_mut() {
            let trimmed = line.trim();
            if trimmed.starts_with(&format!("{}=", k)) {
                *line = format!("{}={}", k, v);
                found = true;
                break;
            }
        }
        if !found {
            lines.push(format!("{}={}", k, v));
        }
    }
    let new_content = lines.join("\n") + "\n";
    std::fs::write(&path, new_content).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_admin_password() -> Result<String, String> {
    if let Some(proj_dirs) = directories::ProjectDirs::from("com", "ekam", "baton") {
        let path = proj_dirs.data_dir().join("baton_admin_password.txt");
        if let Ok(pw) = std::fs::read_to_string(&path) {
            return Ok(pw.trim().to_string());
        }
    }
    // If it can't be read, it will fail
    Err("Could not read admin password file".into())
}

#[tauri::command]
fn open_inbox_file(file_name: String) -> Result<(), String> {
    let inbox_dir = if let Some(proj_dirs) = directories::ProjectDirs::from("com", "ekam", "baton") {
        proj_dirs.data_local_dir().join("inbox")
    } else {
        std::env::temp_dir().join("baton_inbox")
    };
    
    // Prevent directory traversal
    if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
        return Err("Invalid file name".to_string());
    }

    let file_path = inbox_dir.join(&file_name);
    
    if !file_path.exists() {
        return Err(format!("File does not exist: {}", file_name));
    }

    // Use opener to open the file with system default application
    opener::open(&file_path).map_err(|e| format!("Failed to open file: {}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .setup(|app| {
            // Spawn the Axum backend in a separate Tokio runtime
            std::thread::spawn(|| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if let Err(e) = mcp_connector::run_server().await {
                        eprintln!("Backend server error: {:?}", e);
                    }
                });
            });
            // Set up System Tray
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;
            use tauri::Manager;

            let show_i = MenuItem::with_id(app, "show", "Show Dashboard", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit Baton", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            TrayIconBuilder::new()
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            window.show().unwrap();
                            window.set_focus().unwrap();
                        }
                    }
                    _ => {}
                })
                .build(app)?;
                
            let splash_window = app.get_webview_window("splashscreen").unwrap();
            let main_window = app.get_webview_window("main").unwrap();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(2500));
                splash_window.close().unwrap();
                main_window.show().unwrap();
                main_window.set_focus().unwrap();
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Hide the window instead of closing the app
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![greet, get_env, set_env, get_admin_password, open_inbox_file])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
