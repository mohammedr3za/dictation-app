//! Appends diagnostic lines to `dictation.log` in the config folder, so a
//! problem can be traced however the app was launched. Transcript text is
//! never logged, only lengths and timings.

use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn path() -> PathBuf {
    crate::config::config_dir().join("dictation.log")
}

pub fn line(message: impl std::fmt::Display) {
    eprintln!("{message}");
    let path = path();
    // Start over once it passes 1 MB, so it never grows without bound.
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > 1_000_000) {
        let _ = std::fs::remove_file(&path);
    }
    let _ = std::fs::create_dir_all(crate::config::config_dir());
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        let _ = writeln!(file, "{seconds} {message}");
    }
}
