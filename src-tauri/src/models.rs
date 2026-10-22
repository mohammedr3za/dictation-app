use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::io::Read;
use std::path::PathBuf;

#[derive(Serialize, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub note: String,
    pub size_mb: u64,
    pub installed: bool,
}

struct Spec {
    id: &'static str,
    name: &'static str,
    note: &'static str,
    size_mb: u64,
    url: &'static str,
    folder: &'static str,
}

const SPECS: &[Spec] = &[
    Spec {
        id: "zipformer-en",
        name: "Zipformer English",
        note: "Fast, ~100 MB int8, no punctuation",
        size_mb: 293,
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-zipformer-en-2023-06-26.tar.bz2",
        folder: "sherpa-onnx-zipformer-en-2023-06-26",
    },
    Spec {
        id: "parakeet-tdt-0.6b-v2",
        name: "Parakeet TDT 0.6B v2",
        note: "Most accurate English, ~600 MB",
        size_mb: 600,
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2",
        folder: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8",
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

pub fn list() -> Vec<ModelInfo> {
    SPECS
        .iter()
        .map(|spec| ModelInfo {
            id: spec.id.into(),
            name: spec.name.into(),
            note: spec.note.into(),
            size_mb: spec.size_mb,
            installed: model_dir(spec.id).join("tokens.txt").exists(),
        })
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

    Ok(target)
}
