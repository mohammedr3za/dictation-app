# Dictation

Fast, local, cross-platform dictation. Hold a shortcut, speak, release — your words appear at the cursor.

Speech is transcribed **on your machine** with [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx). No audio is uploaded, ever. Cleanup with an LLM is optional and off by default.

Built with Tauri 2, Rust, and TypeScript, so the same app runs on macOS and Linux.

## Install

### macOS

1. Download `Dictation_*.dmg` from [Releases](../../releases/latest).
2. Open it and drag **Dictation** to Applications.
3. First launch: right-click the app → **Open** (the build is unsigned).

### Linux

Install with one command:

```bash
curl -fsSL https://raw.githubusercontent.com/mohammedr3za/dictation-app/main/install.sh | bash
```

That downloads the latest AppImage to `~/.local/share/dictation`, adds a `dictation` launcher in
`~/.local/bin`, and creates a desktop entry. If `~/.local/bin` is not on your `PATH`, the installer
tells you what to add.

Or install manually:

1. Download `Dictation_*.AppImage` from [Releases](../../releases/latest).
2. Make it executable and run:

```bash
chmod +x Dictation_*.AppImage
./Dictation_*.AppImage
```

Requires `webkit2gtk-4.1` and `libasound2`:

```bash
# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-0 libasound2
# Fedora
sudo dnf install webkit2gtk4.1 alsa-lib
```

## First run

Open **Model**, choose a speech model, and click **Download**. Models are fetched once from the
sherpa-onnx release mirror and stored in your app data directory.

| Model | Size | Notes |
|---|---|---|
| Zipformer English | ~293 MB | Fast, no punctuation |
| Parakeet TDT 0.6B v2 | ~600 MB | Most accurate English |

## Build from source

Prerequisites: Rust (via [rustup](https://rustup.rs)), Node 18+, and `cmake`.

```bash
git clone https://github.com/mohammedr3za/dictation-app.git
cd dictation-app
npm install
npm run tauri build
```

The bundle lands in `src-tauri/target/release/bundle/`.

For development:

```bash
npm run tauri dev
```

## Headless ASR check

```bash
./src-tauri/target/release/dictation-app \
  --transcribe-file path/to/audio.wav \
  --model-dir ~/Library/Application\ Support/dictation-app/models/zipformer-en
```

## Privacy

- Audio is transcribed locally and is never uploaded.
- Cleanup is off by default. When enabled, transcript text is sent to the API endpoint you configure.
- API keys are stored in the OS keychain (Keychain on macOS, Secret Service on Linux), never in the config file.

## Usage

1. Download a model from the **Model** tab.
2. Press **Alt+Command+D** (or click **Start recording**).
3. Speak.
4. Press the shortcut again to stop. The transcript is pasted at your cursor, or copied to the
   clipboard if you chose that mode.

On macOS the first recording asks for microphone access. On Linux, X11 is supported; Wayland
compositors may block global shortcuts and synthetic input.

## Status

Working:

- Settings UI on macOS and Linux
- On-device model download and management
- Offline transcription through sherpa-onnx
- Microphone capture, resampling to 16 kHz
- Global toggle shortcut (Alt+Command+D)
- Paste at cursor or copy to clipboard

Not yet implemented:

- Hold-to-talk (the current shortcut is press-to-start, press-to-stop)
- Optional LLM cleanup
- Tray icon

## License

MIT
