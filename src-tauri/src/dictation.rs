use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

use crate::asr::{self, AsrEngine};
use crate::config;
use crate::models;

/// Captures the default microphone into memory at the device's native rate.
pub struct Recorder {
    samples: Arc<Mutex<Vec<f32>>>,
    stream: cpal::Stream,
    sample_rate: u32,
    channels: u16,
}

impl Recorder {
    pub fn start() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("no microphone available"))?;
        let supported = device.default_input_config()?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels();
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        let samples = Arc::new(Mutex::new(Vec::<f32>::with_capacity(16000 * 30)));
        let sink = Arc::clone(&samples);

        let stream = match format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _: &_| {
                    if let Ok(mut buffer) = sink.lock() {
                        buffer.extend_from_slice(data);
                    }
                },
                |_| {},
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _: &_| {
                    if let Ok(mut buffer) = sink.lock() {
                        buffer.extend(data.iter().map(|v| *v as f32 / 32768.0));
                    }
                },
                |_| {},
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_input_stream(
                config,
                move |data: &[u16], _: &_| {
                    if let Ok(mut buffer) = sink.lock() {
                        buffer.extend(data.iter().map(|v| (*v as f32 - 32768.0) / 32768.0));
                    }
                },
                |_| {},
                None,
            )?,
            other => return Err(anyhow!("unsupported sample format {other:?}")),
        };

        stream.play()?;

        Ok(Self {
            samples,
            stream,
            sample_rate,
            channels,
        })
    }

    pub fn stop(self) -> (Vec<f32>, u32, u16) {
        drop(self.stream);
        let raw = self
            .samples
            .lock()
            .map(|buffer| buffer.clone())
            .unwrap_or_default();
        (raw, self.sample_rate, self.channels)
    }
}

/// Downmixes to mono and resamples to 16 kHz with linear interpolation.
pub fn to_mono_16k(samples: &[f32], rate: u32, channels: u16) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    let mono: Vec<f32> = if channels == 1 {
        samples.to_vec()
    } else {
        samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
            .collect()
    };

    if rate == 16_000 || mono.is_empty() {
        return mono;
    }

    let ratio = rate as f64 / 16_000.0;
    let out_len = (mono.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let source = i as f64 * ratio;
        let index = source as usize;
        let fraction = (source - index as f64) as f32;
        let a = mono.get(index).copied().unwrap_or(0.0);
        let b = mono.get(index + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * fraction);
    }
    out
}

/// Puts text on the clipboard and, when `paste` is true, sends the paste shortcut.
pub fn insert(text: &str, paste: bool) -> Result<()> {
    use arboard::Clipboard;
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut clipboard = Clipboard::new()?;
    let previous = clipboard.get_text().ok();
    clipboard.set_text(text)?;

    if paste {
        let mut enigo = Enigo::new(&Settings::default())?;
        #[cfg(target_os = "macos")]
        let modifier = Key::Meta;
        #[cfg(not(target_os = "macos"))]
        let modifier = Key::Control;

        enigo.key(modifier, Direction::Press)?;
        enigo.key(Key::Unicode('v'), Direction::Click)?;
        enigo.key(modifier, Direction::Release)?;

        std::thread::sleep(std::time::Duration::from_millis(350));
        if let Some(previous) = previous {
            let _ = clipboard.set_text(previous);
        }
    }

    Ok(())
}

/// Owns the recording session and the loaded recognizer.
#[derive(Default)]
pub struct Dictation {
    recorder: Mutex<Option<Recorder>>,
    engine: Mutex<Option<AsrEngine>>,
}

impl Dictation {
    pub fn start(&self) -> Result<()> {
        let mut slot = self.recorder.lock().map_err(|_| anyhow!("state poisoned"))?;
        if slot.is_some() {
            return Ok(());
        }
        *slot = Some(Recorder::start()?);
        Ok(())
    }

    pub fn cancel(&self) {
        if let Ok(mut slot) = self.recorder.lock() {
            *slot = None;
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recorder
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    pub fn stop(&self) -> Result<String> {
        let recorder = self
            .recorder
            .lock()
            .map_err(|_| anyhow!("state poisoned"))?
            .take()
            .ok_or_else(|| anyhow!("not recording"))?;

        let (samples, rate, channels) = recorder.stop();
        if samples.is_empty() {
            return Err(anyhow!("no audio captured"));
        }

        let pcm = to_mono_16k(&samples, rate, channels);
        let engine = self.engine_for_current_model()?;
        let engine = engine.as_ref().ok_or_else(|| anyhow!("no recognizer"))?;
        engine.transcribe(&pcm, 16_000)
    }

    fn engine_for_current_model(&self) -> Result<std::sync::MutexGuard<'_, Option<AsrEngine>>> {
        let mut guard = self.engine.lock().map_err(|_| anyhow!("state poisoned"))?;
        if guard.is_none() {
            let settings = config::load();
            let dir = models::model_dir(&settings.model_id);
            let files = asr::ModelFiles::discover(&dir)?;
            *guard = Some(AsrEngine::new(&files, 4)?);
        }
        Ok(guard)
    }
}
