use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub note: String,
    pub size_mb: u64,
    pub recommended: bool,
    pub installed: bool,
}

struct Spec {
    id: &'static str,
    name: &'static str,
    note: &'static str,
    /// Download size; installed size is in `note`.
    size_mb: u64,
    recommended: bool,
    url: &'static str,
    folder: &'static str,
}

const SPECS: &[Spec] = &[
    Spec {
        id: "parakeet-tdt-0.6b-v2",
        name: "Parakeet TDT 0.6B v2",
        note: "Most accurate, with punctuation and capitals. 630 MB on disk",
        size_mb: 460,
        recommended: true,
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2",
        folder: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8",
    },
    Spec {
        id: "zipformer-en",
        name: "Zipformer English",
        note: "Fastest, lower accuracy. All capitals, no punctuation. 70 MB on disk",
        size_mb: 293,
        recommended: false,
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-zipformer-en-2023-06-26.tar.bz2",
        folder: "sherpa-onnx-zipformer-en-2023-06-26",
    },
];

pub fn model_root() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dictation-app")
        .join("models")
}

pub fn model_dir(id: &str) -> PathBuf {
    model_root().join(id)
}

pub fn is_installed(id: &str) -> bool {
    model_dir(id).join("tokens.txt").exists()
}

pub fn list() -> Vec<ModelInfo> {
    SPECS
        .iter()
        .map(|spec| ModelInfo {
            id: spec.id.into(),
            name: spec.name.into(),
            note: spec.note.into(),
            size_mb: spec.size_mb,
            recommended: spec.recommended,
            installed: is_installed(spec.id),
        })
        .collect()
}

/// Ids of the models on disk.
pub fn installed() -> Vec<&'static str> {
    SPECS
        .iter()
        .map(|spec| spec.id)
        .filter(|id| is_installed(id))
        .collect()
}

fn spec(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.id == id)
}

/// Downloads and extracts a model. `progress` receives (downloaded, total) bytes.
pub fn download(id: &str, progress: impl Fn(u64, u64)) -> Result<PathBuf> {
    let spec = spec(id).ok_or_else(|| anyhow!("unknown model {id}"))?;
    let root = model_root();
    std::fs::create_dir_all(&root)?;

    let archive = root.join(format!("{id}.tar.bz2"));
    let mut response = ureq::get(spec.url)
        .call()
        .context("requesting model download")?;
    let total: u64 = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut reader = response.body_mut().as_reader();
    let mut file = std::fs::File::create(&archive)?;
    let mut buffer = vec![0u8; 256 * 1024];
    let mut downloaded = 0u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buffer[..read])?;
        downloaded += read as u64;
        progress(downloaded, total);
    }
    drop(file);

    let decoder = bzip2::read::BzDecoder::new(std::fs::File::open(&archive)?);
    let mut tar = tar::Archive::new(decoder);
    tar.unpack(&root)?;

    let extracted = root.join(spec.folder);
    let target = model_dir(id);
    if extracted.exists() && extracted != target {
        let _ = std::fs::remove_dir_all(&target);
        std::fs::rename(&extracted, &target)?;
    }
    let _ = std::fs::remove_file(&archive);
    prune(&target);

    Ok(target)
}

/// Removes an installed model and any half-finished download of it.
pub fn delete(id: &str) -> Result<()> {
    // Only known ids, so the id can never point outside the models folder.
    spec(id).ok_or_else(|| anyhow!("unknown model {id}"))?;
    let dir = model_dir(id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("deleting {}", dir.display()))?;
    }
    let _ = std::fs::remove_file(model_root().join(format!("{id}.tar.bz2")));
    Ok(())
}

/// Deletes full-precision `.onnx` files that sit next to their int8 versions.
/// Only the int8 files are loaded, and the others can be several times larger.
pub fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let files: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    let name = |path: &PathBuf| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    };
    if !files.iter().any(|path| name(path).ends_with(".int8.onnx")) {
        return;
    }
    for path in &files {
        let file = name(path);
        if file.ends_with(".onnx") && !file.ends_with(".int8.onnx") {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dictation-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn prune_keeps_only_int8_weights() {
        let dir = scratch("prune");
        for file in ["encoder.onnx", "encoder.int8.onnx", "joiner.onnx", "joiner.int8.onnx", "tokens.txt"] {
            std::fs::write(dir.join(file), b"x").unwrap();
        }
        prune(&dir);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["encoder.int8.onnx", "joiner.int8.onnx", "tokens.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn prune_leaves_a_model_without_int8_files_alone() {
        let dir = scratch("prune-fp32");
        std::fs::write(dir.join("model.onnx"), b"x").unwrap();
        prune(&dir);
        assert!(dir.join("model.onnx").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_rejects_unknown_ids() {
        assert!(delete("../../etc").is_err());
        assert!(delete("not-a-model").is_err());
    }

    #[test]
    fn exactly_one_model_is_recommended() {
        assert_eq!(SPECS.iter().filter(|s| s.recommended).count(), 1);
    }
}
