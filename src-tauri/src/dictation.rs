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

        crate::logfile::line(format!(
            "mic: {} at {sample_rate} Hz, {channels} ch, {format:?}",
            device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "unknown".into())
        ));

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

/// Asks macOS to list this app under Privacy & Security → Accessibility, which
/// pasting needs. Shows the system prompt while the app is not yet allowed and
/// returns whether it is.
#[cfg(target_os = "macos")]
pub fn request_accessibility() -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::CFString;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    }

    let options = CFDictionary::from_CFType_pairs(&[(
        CFString::new("AXTrustedCheckOptionPrompt"),
        CFBoolean::true_value(),
    )]);
    // SAFETY: the dictionary outlives the call.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

#[cfg(not(target_os = "macos"))]
pub fn request_accessibility() -> bool {
    true
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

struct LoadedEngine {
    model_id: String,
    engine: AsrEngine,
}

/// Owns the recording session and the loaded recognizer.
#[derive(Default)]
pub struct Dictation {
    recorder: Mutex<Option<Recorder>>,
    engine: Mutex<Option<LoadedEngine>>,
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
        // A peak of zero means the OS handed over silence, which is what a
        // denied microphone permission looks like.
        let peak = pcm.iter().fold(0f32, |peak, sample| peak.max(sample.abs()));
        crate::logfile::line(format!("captured {:.1} s, peak level {peak:.3}", pcm.len() as f32 / 16_000.0));

        let started = std::time::Instant::now();
        let loaded = self.engine_for_current_model()?;
        let loaded = loaded.as_ref().ok_or_else(|| anyhow!("no recognizer"))?;
        let text = loaded.engine.transcribe(&pcm, 16_000)?;
        crate::logfile::line(format!(
            "transcribed with {} in {} ms: {} chars",
            loaded.model_id,
            started.elapsed().as_millis(),
            text.chars().count()
        ));
        Ok(text)
    }

    /// Loads the selected model ahead of the first dictation, which would
    /// otherwise wait for it (about two seconds for Parakeet).
    pub fn preload(&self) -> Result<()> {
        self.engine_for_current_model().map(|_| ())
    }

    /// Frees the recognizer if it was loaded from `model_id`.
    pub fn unload_model(&self, model_id: &str) {
        if let Ok(mut guard) = self.engine.lock() {
            if guard.as_ref().is_some_and(|loaded| loaded.model_id == model_id) {
                *guard = None;
            }
        }
    }

    /// Returns the recognizer for the model selected in settings, loading it
    /// when nothing is loaded yet or the selection changed.
    fn engine_for_current_model(&self) -> Result<std::sync::MutexGuard<'_, Option<LoadedEngine>>> {
        let mut guard = self.engine.lock().map_err(|_| anyhow!("state poisoned"))?;
        let model_id = config::load().model_id;
        if guard.as_ref().map(|loaded| loaded.model_id.as_str()) != Some(model_id.as_str()) {
            // Drop the old model first so two are never in memory at once.
            *guard = None;
            if !models::is_installed(&model_id) {
                return Err(anyhow!("No speech model installed. Download one in the Model tab."));
            }
            let files = asr::ModelFiles::discover(&models::model_dir(&model_id))?;
            *guard = Some(LoadedEngine {
                engine: AsrEngine::new(&files, 4)?,
                model_id,
            });
        }
        Ok(guard)
    }
}
