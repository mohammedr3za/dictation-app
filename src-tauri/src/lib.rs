pub mod asr;
pub mod config;
pub mod dictation;
pub mod hotkey;
pub mod logfile;
pub mod models;
pub mod prompt;

use config::{InsertMode, Settings};
use dictation::Dictation;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Serialize, Clone)]
struct DownloadProgress {
    downloaded: u64,
    total: u64,
}

/// Why the saved shortcut is inactive, if it is.
#[derive(Default)]
struct HotkeyStatus(Mutex<Option<String>>);

#[tauri::command]
fn load_settings() -> Settings {
    config::load()
}

fn is_valid_hotkey(accelerator: &str) -> bool {
    hotkey::parse_chord(accelerator).is_some() || accelerator.parse::<Shortcut>().is_ok()
}

/// Replaces whatever shortcut is active with `accelerator`. A modifier-only
/// chord goes to the keyboard monitor; anything with a key goes to the OS
/// hotkey API through the global shortcut plugin.
///
/// Fails when another app owns the combination, or when macOS has not granted
/// Input Monitoring. The setting is saved regardless, so the UI can say why the
/// shortcut is inactive.
fn apply_hotkey(app: &tauri::AppHandle, accelerator: &str) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    let monitor = app.state::<hotkey::ModifierMonitor>();
    let result = match hotkey::parse_chord(accelerator) {
        Some(chord) => monitor.watch(Some(chord)),
        None => {
            let _ = monitor.watch(None);
            shortcuts
                .register(accelerator)
                .map_err(|e| format!("{accelerator} could not be registered: {e}"))
        }
    };
    match &result {
        Ok(()) => logfile::line(format!("shortcut {accelerator}: active")),
        Err(error) => logfile::line(format!("shortcut {accelerator}: {error}")),
    }
    if let Ok(mut status) = app.state::<HotkeyStatus>().0.lock() {
        *status = result.clone().err();
    }
    result
}

/// Loads the selected model in the background, so the first dictation after
/// launch or after switching models does not wait for it.
fn preload_model(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = app.state::<Dictation>().preload();
    });
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, settings: Settings) -> Result<(), String> {
    config::save(&settings).map_err(|e| format!("{e:#}"))?;
    preload_model(&app);
    apply_hotkey(&app, &settings.hotkey)
}

#[tauri::command]
fn hotkey_status(status: tauri::State<HotkeyStatus>) -> Option<String> {
    status.0.lock().ok().and_then(|status| status.clone())
}

/// Suspends the shortcut while the settings window records a new one.
/// Otherwise the current combination reaches the shortcut handler instead of
/// the web view, and pressing it starts a dictation.
#[tauri::command]
fn pause_hotkey(app: tauri::AppHandle, paused: bool) -> Result<(), String> {
    if paused {
        let _ = app.state::<hotkey::ModifierMonitor>().watch(None);
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
fn delete_model(dictation: tauri::State<Dictation>, id: String) -> Result<(), String> {
    dictation.unload_model(&id);
    models::delete(&id).map_err(|e| format!("{e:#}"))
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

/// Starts or stops a dictation from the shortcut, on its own thread:
/// transcription takes up to a second and must not stall the window or the
/// keyboard monitor.
fn toggle(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let dictation = app.state::<Dictation>();
        if dictation.is_recording() {
            let result = dictation.stop();
            let _ = app.emit("recording", false);
            match result {
                Ok(text) if !text.is_empty() => deliver(&app, text),
                Ok(_) => logfile::line("no speech recognized"),
                Err(error) => {
                    logfile::line(format!("stop failed: {error:#}"));
                    let _ = app.emit("dictation-error", format!("{error:#}"));
                }
            }
        } else {
            match dictation.start() {
                Ok(()) => {
                    logfile::line("recording started");
                    let _ = app.emit("recording", true);
                }
                Err(error) => {
                    logfile::line(format!("recording failed: {error:#}"));
                    let _ = app.emit("dictation-error", format!("{error:#}"));
                }
            }
        }
    });
}

/// Inserts the transcript on the main thread: typing the paste shortcut looks
/// up the keyboard layout, which macOS only allows there.
fn deliver(app: &tauri::AppHandle, text: String) {
    let paste = config::load().insert_mode == InsertMode::Paste;
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || match dictation::insert(&text, paste) {
        Ok(()) => {
            logfile::line(format!("inserted {} chars (paste={paste})", text.chars().count()));
            let _ = handle.emit("transcript", text);
        }
        Err(error) => {
            logfile::line(format!("insert failed: {error:#}"));
            let _ = handle.emit("dictation-error", format!("{error:#}"));
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        logfile::line("shortcut pressed");
                        toggle(app);
                    }
                })
                .build(),
        )
        .manage(Dictation::default())
        .manage(HotkeyStatus::default())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("Dictation");
            }
            let handle = app.handle().clone();
            app.manage(hotkey::ModifierMonitor::new(move || {
                logfile::line("shortcut tapped");
                toggle(&handle);
            }));

            let mut settings = config::load();
            logfile::line(format!(
                "start v{} on {} model={} insert={:?}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                settings.model_id,
                settings.insert_mode
            ));
            // Older versions let the shortcut be typed by hand, so a saved value may
            // not parse at all. Replace it rather than start with no shortcut.
            if !is_valid_hotkey(&settings.hotkey) {
                eprintln!("saved shortcut {:?} is invalid; using the default", settings.hotkey);
                settings.hotkey = Settings::default().hotkey;
                let _ = config::save(&settings);
            }
            let _ = apply_hotkey(app.handle(), &settings.hotkey);
            // An update gets a new signature, so macOS drops it from the list.
            if settings.insert_mode == InsertMode::Paste {
                let trusted = dictation::request_accessibility();
                logfile::line(format!("accessibility: {}", if trusted { "allowed" } else { "not allowed" }));
            }

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                // Older versions kept unused full-precision weights on disk.
                for id in models::installed() {
                    models::prune(&models::model_dir(id));
                }
                let started = std::time::Instant::now();
                match handle.state::<Dictation>().preload() {
                    Ok(()) => logfile::line(format!("model loaded in {} ms", started.elapsed().as_millis())),
                    Err(error) => logfile::line(format!("model not loaded: {error:#}")),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            hotkey_status,
            pause_hotkey,
            list_models,
            download_model,
            delete_model,
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

    #[test]
    fn default_shortcut_is_valid() {
        assert!(super::is_valid_hotkey(&crate::config::Settings::default().hotkey));
        assert!(super::is_valid_hotkey("Alt+Command+KeyD"));
        assert!(!super::is_valid_hotkey("Command"));
    }
}
