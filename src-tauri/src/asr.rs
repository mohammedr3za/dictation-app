use anyhow::{anyhow, Context, Result};
use sherpa_onnx::{
    OfflineModelConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig,
};
use std::path::{Path, PathBuf};

/// Files that make up one offline transducer model.
#[derive(Debug, Clone)]
pub struct ModelFiles {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
}

impl ModelFiles {
    /// Locates the int8 transducer files inside a model directory.
    pub fn discover(dir: &Path) -> Result<Self> {
        let find = |prefix: &str, suffix: &str| -> Result<PathBuf> {
            let mut matches: Vec<PathBuf> = std::fs::read_dir(dir)
                .with_context(|| format!("reading model dir {}", dir.display()))?
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|path| {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    name.starts_with(prefix) && name.ends_with(suffix)
                })
                .collect();
            matches.sort();
            matches
                .pop()
                .ok_or_else(|| anyhow!("no {prefix}*{suffix} in {}", dir.display()))
        };

        let encoder = find("encoder", ".int8.onnx")?;
        let decoder = find("decoder", ".int8.onnx")?;
        let joiner = find("joiner", ".int8.onnx")?;
        let tokens = dir.join("tokens.txt");
        if !tokens.exists() {
            return Err(anyhow!("tokens.txt missing in {}", dir.display()));
        }

        Ok(Self {
            encoder,
            decoder,
            joiner,
            tokens,
        })
    }
}

pub struct AsrEngine {
    recognizer: OfflineRecognizer,
}

impl AsrEngine {
    pub fn new(files: &ModelFiles, threads: i32) -> Result<Self> {
        let config = OfflineRecognizerConfig {
            model_config: OfflineModelConfig {
                transducer: OfflineTransducerModelConfig {
                    encoder: Some(files.encoder.to_string_lossy().into_owned()),
                    decoder: Some(files.decoder.to_string_lossy().into_owned()),
                    joiner: Some(files.joiner.to_string_lossy().into_owned()),
                },
                tokens: Some(files.tokens.to_string_lossy().into_owned()),
                num_threads: threads,
                debug: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let recognizer = OfflineRecognizer::create(&config)
            .ok_or_else(|| anyhow!("could not create recognizer"))?;

        Ok(Self { recognizer })
    }

    /// Transcribes mono f32 samples at the given rate. The model expects 16 kHz.
    pub fn transcribe(&self, samples: &[f32], sample_rate: i32) -> Result<String> {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(sample_rate, samples);
        self.recognizer.decode(&stream);
        let result = stream.get_result().ok_or_else(|| anyhow!("no result"))?;
        Ok(result.text.trim().to_string())
    }
}

/// Default location for downloaded models.
pub fn default_model_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dictation-app")
        .join("models")
        .join("zipformer-en")
}

/// Convenience path used by the CLI: load a model directory and transcribe a WAV.
pub fn transcribe_path(wav: &Path, model_dir: &Path) -> Result<String> {
    let files = ModelFiles::discover(model_dir)?;
    let engine = AsrEngine::new(&files, 4)?;
    let (samples, rate) = read_wav(wav)?;
    engine.transcribe(&samples, rate)
}

/// Reads a WAV file into mono f32 samples plus its sample rate.
pub fn read_wav(path: &Path) -> Result<(Vec<f32>, i32)> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("opening {}", path.display()))?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let max = (1i32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<std::result::Result<_, _>>()?
        }
    };

    // Downmix to mono if the file is stereo.
    let samples = if spec.channels > 1 {
        samples
            .chunks(spec.channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
            .collect()
    } else {
        samples
    };

    Ok((samples, spec.sample_rate as i32))
}
