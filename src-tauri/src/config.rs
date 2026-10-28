use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum InsertMode {
    Paste,
    Clipboard,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub model_id: String,
    pub hotkey: String,
    pub language: String,
    pub insert_mode: InsertMode,
    pub cleanup_enabled: bool,
    pub cleanup_base_url: String,
    pub cleanup_model: String,
    pub prompt_enabled: bool,
    pub prompt_base_url: String,
    pub prompt_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model_id: "parakeet-tdt-0.6b-v2".into(),
            hotkey: "Alt+Command".into(),
            language: "en".into(),
            insert_mode: InsertMode::Paste,
            cleanup_enabled: false,
            cleanup_base_url: "https://api.goedelmachines.com/v1".into(),
            cleanup_model: "deepseek/deepseek-v4.1-flash".into(),
            prompt_enabled: false,
            prompt_base_url: "https://api.goedelmachines.com/v1".into(),
            prompt_model: "deepseek/deepseek-v4.1-flash".into(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dictation-app")
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn load() -> Settings {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) -> Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let json = serde_json::to_string_pretty(settings)?;
    std::fs::write(settings_path(), json).context("writing settings")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_settings_fall_back_to_defaults() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(!settings.prompt_enabled);
        assert_eq!(settings.prompt_base_url, "https://api.goedelmachines.com/v1");
        assert_eq!(settings.prompt_model, "deepseek/deepseek-v4.1-flash");
    }
}
