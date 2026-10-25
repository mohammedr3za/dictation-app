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

/// Replaces whatever global shortcut is registered with `accelerator`.
///
/// Fails when the combination cannot be registered, which happens when another
/// app already owns it. The setting is saved regardless, so the caller can report
/// that the shortcut is inactive and let the user pick another.
fn apply_hotkey(app: &tauri::AppHandle, accelerator: &str) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    shortcuts
        .register(accelerator)
        .map_err(|e| format!("{accelerator} could not be registered: {e}"))
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, settings: Settings) -> Result<(), String> {
    config::save(&settings).map_err(|e| format!("{e:#}"))?;
    apply_hotkey(&app, &settings.hotkey)
}

/// Suspends the global shortcut while the settings window records a new one.
/// Otherwise the OS delivers the current combination to the shortcut handler
/// instead of the web view, and pressing it starts a dictation.
#[tauri::command]
fn pause_hotkey(app: tauri::AppHandle, paused: bool) -> Result<(), String> {
    if paused {
        app.global_shortcut()
            .unregister_all()
            .map_err(|e| format!("{e}"))
    } else {
        apply_hotkey(&app, &config::load().hotkey)
    }
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
            let settings = config::load();
            if let Err(error) = apply_hotkey(app.handle(), &settings.hotkey) {
                eprintln!("global shortcut unavailable: {error}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            pause_hotkey,
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use tauri_plugin_global_shortcut::Shortcut;

    /// One code from every group the settings UI can capture (`CAPTURABLE_CODE`
    /// in src/main.ts). If the parser rejects one, the UI would save a shortcut
    /// that can never be registered.
    #[test]
    fn captured_shortcuts_parse() {
        let codes = [
            "KeyA", "KeyZ", "Digit0", "Digit9", "F1", "F12", "F24", "Space", "Enter", "Tab",
            "Backquote", "Minus", "Equal", "BracketLeft", "BracketRight", "Backslash",
            "Semicolon", "Quote", "Comma", "Period", "Slash", "ArrowUp", "ArrowDown",
            "ArrowLeft", "ArrowRight", "Numpad0", "Numpad9",
        ];
        let modifiers = ["Control", "Alt", "Shift", "Command", "Control+Alt+Shift+Command"];

        for modifier in modifiers {
            for code in codes {
                let accelerator = format!("{modifier}+{code}");
                assert!(
                    Shortcut::from_str(&accelerator).is_ok(),
                    "{accelerator} should parse"
                );
            }
        }

        // Guards against a parser that accepts anything.
        assert!(Shortcut::from_str("Control+NotAKey").is_err());
    }
}
