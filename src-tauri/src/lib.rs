pub mod asr;
pub mod config;
pub mod dictation;
pub mod models;

use config::{InsertMode, Settings};
use dictation::Dictation;
use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

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
            let _ = emitter.emit("model-progress", DownloadProgress { downloaded, total });
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
    let entry =
        keyring::Entry::new("dictation-app", "cleanup-api-key").map_err(|e| format!("{e}"))?;
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

#[tauri::command]
fn start_dictation(app: tauri::AppHandle, dictation: tauri::State<Dictation>) -> Result<(), String> {
    dictation.start().map_err(|e| format!("{e:#}"))?;
    let _ = app.emit("recording", true);
    Ok(())
}

#[tauri::command]
fn stop_dictation(
    app: tauri::AppHandle,
    dictation: tauri::State<Dictation>,
) -> Result<String, String> {
    let text = dictation.stop().map_err(|e| format!("{e:#}"))?;
    let _ = app.emit("recording", false);

    if text.is_empty() {
        return Ok(text);
    }

    let settings = config::load();
    let paste = settings.insert_mode == InsertMode::Paste;
    dictation::insert(&text, paste).map_err(|e| format!("{e:#}"))?;
    let _ = app.emit("transcript", text.clone());
    Ok(text)
}

#[tauri::command]
fn cancel_dictation(app: tauri::AppHandle, dictation: tauri::State<Dictation>) {
    dictation.cancel();
    let _ = app.emit("recording", false);
}

#[tauri::command]
fn is_recording(dictation: tauri::State<Dictation>) -> bool {
    dictation.is_recording()
}

fn toggle(app: &tauri::AppHandle) {
    let dictation = app.state::<Dictation>();
    if dictation.is_recording() {
        match dictation.stop() {
            Ok(text) if !text.is_empty() => {
                let settings = config::load();
                let paste = settings.insert_mode == InsertMode::Paste;
                let _ = dictation::insert(&text, paste);
                let _ = app.emit("transcript", text);
            }
            Ok(_) => {}
            Err(error) => {
                let _ = app.emit("dictation-error", format!("{error:#}"));
            }
        }
        let _ = app.emit("recording", false);
    } else {
        match dictation.start() {
            Ok(()) => {
                let _ = app.emit("recording", true);
            }
            Err(error) => {
                let _ = app.emit("dictation-error", format!("{error:#}"));
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle(app);
                    }
                })
                .build(),
        )
        .manage(Dictation::default())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("Dictation");
            }
            let _ = app.global_shortcut().register("Alt+Command+KeyD");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            list_models,
            download_model,
            has_api_key,
            save_api_key,
            app_info,
            start_dictation,
            stop_dictation,
            cancel_dictation,
            is_recording
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
