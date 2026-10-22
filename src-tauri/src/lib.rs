pub mod asr;
pub mod config;
pub mod models;

use config::Settings;
use serde::Serialize;
use tauri::{Emitter, Manager};

#[derive(Serialize, Clone)]
struct DownloadProgress {
    downloaded: u64,
    total: u64,
}

#[tauri::command]
fn load_settings() -> Settings {
    config::load()
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> {
    config::save(&settings).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn list_models() -> Vec<models::ModelInfo> {
    models::list()
}

#[tauri::command]
async fn download_model(app: tauri::AppHandle, id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let emitter = app.clone();
        models::download(&id, move |downloaded, total| {
            let _ = emitter.emit(
                "model-progress",
                DownloadProgress { downloaded, total },
            );
        })
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| format!("{e}"))?
}

#[tauri::command]
fn has_api_key() -> bool {
    keyring::Entry::new("dictation-app", "cleanup-api-key")
        .ok()
        .and_then(|entry| entry.get_password().ok())
        .map(|key| !key.is_empty())
        .unwrap_or(false)
}

#[tauri::command]
fn save_api_key(key: String) -> Result<(), String> {
    let entry = keyring::Entry::new("dictation-app", "cleanup-api-key")
        .map_err(|e| format!("{e}"))?;
    if key.is_empty() {
        let _ = entry.delete_credential();
        Ok(())
    } else {
        entry.set_password(&key).map_err(|e| format!("{e}"))
    }
}

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("Dictation");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            list_models,
            download_model,
            has_api_key,
            save_api_key,
            app_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
