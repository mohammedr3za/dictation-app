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
sherpa-onnx release mirror.

| Model | Download | On disk | Notes |
|---|---|---|---|
| Parakeet TDT 0.6B v2 (recommended) | 460 MB | 630 MB | Most accurate, punctuation and capitals |
| Zipformer English | 293 MB | 70 MB | Fastest, lower accuracy, all capitals, no punctuation |

Models live here; delete one with the **Delete** button in the Model tab, or remove its folder:

| OS | Models | Settings |
|---|---|---|
| macOS | `~/Library/Application Support/dictation-app/models/` | `~/Library/Application Support/dictation-app/settings.json` |
| Linux | `~/.local/share/dictation-app/models/` | `~/.config/dictation-app/settings.json` |

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
2. Tap **Option+Command** (⌥⌘; Alt+Super on Linux), or click **Start recording**.
3. Speak.
4. Tap it again to stop. The transcript is pasted at your cursor, or copied to the clipboard if
   you chose that mode.

A tap means pressing both keys and letting go with nothing else in between, so shortcuts such as
⌥⌘Esc keep working. To change it, click the shortcut in **General** and press a new one: two or
more modifiers alone, or modifiers plus a key (letters need two modifiers).

### Permissions

macOS asks for these the first time they are needed. Allow **Dictation** in System Settings →
Privacy & Security:

| Permission | Needed for |
|---|---|
| Microphone | Recording |
| Input Monitoring | Modifier-only shortcuts such as ⌥⌘ |
| Accessibility | Pasting at the cursor |

The builds are unsigned, so after an update macOS may list Dictation again as a new app. If the
shortcut or paste stops working after updating, switch Dictation off and on again in those lists.

On Linux, X11 is supported. Wayland does not let apps watch the keyboard or type into other apps,
so use a shortcut with a key, the in-app button, and clipboard mode there.

To check that a modifier-only shortcut is seen, without the app window:

```bash
dictation --watch-hotkey Alt+Command   # prints "tap" each time
```

## Status

Working:

- Settings UI on macOS and Linux
- On-device model download and management
- Offline transcription through sherpa-onnx
- Microphone capture, resampling to 16 kHz
- Global toggle shortcut, including modifier-only chords (default Option+Command)
- Paste at cursor or copy to clipboard
- Model download, switching, and deletion

Not yet implemented:

- Hold-to-talk (the current shortcut is tap-to-start, tap-to-stop)
- Optional LLM cleanup
- Tray icon

## License

MIT
