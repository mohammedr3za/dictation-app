import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type InsertMode = "paste" | "clipboard";

interface Settings {
  model_id: string;
  hotkey: string;
  language: string;
  insert_mode: InsertMode;
  cleanup_enabled: boolean;
  cleanup_base_url: string;
  cleanup_model: string;
}

interface ModelInfo {
  id: string;
  name: string;
  note: string;
  size_mb: number;
  installed: boolean;
}

let settings: Settings;
let insertMode: InsertMode = "paste";
let platform = "linux";

// The global shortcut plugin needs a non-modifier key, so a chord of only
// modifiers cannot be registered — capture rejects it here too.
const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

// Codes the shortcut parser accepts. Anything else is ignored during capture.
const CAPTURABLE_CODE =
  /^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-9]|2[0-4])|Space|Enter|Tab|Backquote|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Comma|Period|Slash|Arrow(Up|Down|Left|Right)|Numpad[0-9])$/;

function displayHotkey(accelerator: string): string {
  const parts = accelerator.split("+");
  const key = parts[parts.length - 1] ?? "";
  const isMac = platform === "macos";

  const labels: Record<string, string> = isMac
    ? { Control: "⌃", Alt: "⌥", Shift: "⇧", Command: "⌘" }
    : { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Command: "Super" };

  const modifiers = parts.slice(0, -1).map((modifier) => labels[modifier] ?? modifier);
  const keyLabel = key
    .replace(/^Key/, "")
    .replace(/^Digit/, "")
    .replace(/^Numpad/, "Num");

  return [...modifiers, keyLabel].join(isMac ? "" : "+");
}

function modifierNames(): string {
  return platform === "macos" ? "⌘, ⌥, ⌃ or ⇧" : "Ctrl, Alt, Shift or Super";
}

const el = <T extends HTMLElement>(selector: string): T =>
  document.querySelector(selector) as T;

function renderInsertMode() {
  document.querySelectorAll<HTMLElement>("#insert-mode button").forEach((button) => {
    button.classList.toggle("active", button.dataset.mode === insertMode);
  });
}

function setStatus(text: string) {
  el("#status").textContent = text;
}

// ---------- shortcut capture ----------

let capturing = false;

const hotkeyButton = (): HTMLButtonElement => el<HTMLButtonElement>("#hotkey");

function renderHotkey() {
  el("#hotkey-value").textContent = displayHotkey(settings.hotkey);
}

function startCapture() {
  capturing = true;
  hotkeyButton().classList.add("capturing");
  el("#hotkey-hint").textContent = "Hold a modifier and press a key…";
  hotkeyButton().focus();
  invoke("pause_hotkey", { paused: true }).catch(console.error);
}

function stopCapture(hint = "Click to change") {
  if (capturing) {
    invoke("pause_hotkey", { paused: false }).catch(console.error);
  }
  capturing = false;
  hotkeyButton().classList.remove("capturing");
  // Releasing Space or Enter on a focused button clicks it, which would
  // immediately start another capture.
  hotkeyButton().blur();
  el("#hotkey-hint").textContent = hint;
}

async function renderModels() {
  const models = await invoke<ModelInfo[]>("list_models");
  const container = el<HTMLDivElement>("#models");
  container.innerHTML = "";

  for (const model of models) {
    const card = document.createElement("div");
    card.className = "card" + (model.id === settings.model_id ? " selected" : "");

    const head = document.createElement("div");
    head.className = "card-head";
    head.innerHTML = `
      <div>
        <h3>${model.name}</h3>
        <p>${model.note}</p>
      </div>
      <span class="badge ${model.installed ? "ok" : ""}">${model.installed ? "Installed" : model.size_mb + " MB"}</span>`;
    card.appendChild(head);

    const actions = document.createElement("div");
    actions.className = "card-actions";

    if (model.installed) {
      const use = document.createElement("button");
      use.className = "primary";
      use.textContent = model.id === settings.model_id ? "Selected" : "Use this model";
      use.disabled = model.id === settings.model_id;
      use.addEventListener("click", async () => {
        settings.model_id = model.id;
        await invoke("save_settings", { settings });
        setStatus(`Using ${model.name}`);
        await renderModels();
      });
      actions.appendChild(use);
    } else {
      const download = document.createElement("button");
      download.className = "primary";
      download.textContent = "Download";
      download.addEventListener("click", async () => {
        download.disabled = true;
        el("#progress").classList.remove("hidden");
        setStatus(`Downloading ${model.name}…`);
        try {
          await invoke("download_model", { id: model.id });
          settings.model_id = model.id;
          await invoke("save_settings", { settings });
          setStatus(`${model.name} installed`);
        } catch (error) {
          setStatus("Download failed");
          console.error(error);
        }
        el("#progress").classList.add("hidden");
        await renderModels();
      });
      actions.appendChild(download);
    }

    card.appendChild(actions);
    container.appendChild(card);
  }
}

async function load() {
  settings = await invoke<Settings>("load_settings");

  const info = await invoke<{ version: string; platform: string; arch: string }>("app_info");
  platform = info.platform;
  el("#version").textContent = info.version;
  el("#platform-detail").textContent = `${info.platform} · ${info.arch}`;
  el("#platform").textContent = `${info.platform} · local`;

  el<HTMLSelectElement>("#language").value = settings.language;
  insertMode = settings.insert_mode;
  renderInsertMode();
  renderHotkey();

  await renderModels();
}

window.addEventListener("DOMContentLoaded", async () => {
  document.querySelectorAll<HTMLElement>(".nav").forEach((button) => {
    button.addEventListener("click", () => {
      document.querySelectorAll(".nav").forEach((b) => b.classList.remove("active"));
      document.querySelectorAll(".panel").forEach((p) => p.classList.remove("active"));
      button.classList.add("active");
      el(`#panel-${button.dataset.panel}`).classList.add("active");
    });
  });

  document.querySelectorAll<HTMLElement>("#insert-mode button").forEach((button) => {
    button.addEventListener("click", () => {
      insertMode = button.dataset.mode as InsertMode;
      renderInsertMode();
    });
  });

  hotkeyButton().addEventListener("click", () => {
    if (capturing) {
      stopCapture();
    } else {
      startCapture();
    }
  });

  window.addEventListener(
    "keydown",
    (event) => {
      if (!capturing) return;

      // Keep the chord from typing into the page or triggering web view shortcuts.
      event.preventDefault();
      event.stopPropagation();

      if (event.key === "Escape") {
        stopCapture();
        return;
      }
      if (MODIFIER_CODES.has(event.code)) return;

      if (!CAPTURABLE_CODE.test(event.code)) {
        el("#hotkey-hint").textContent = "That key can't be used — try another";
        return;
      }

      const modifiers: string[] = [];
      if (event.ctrlKey) modifiers.push("Control");
      if (event.altKey) modifiers.push("Alt");
      if (event.shiftKey) modifiers.push("Shift");
      if (event.metaKey) modifiers.push("Command");

      if (modifiers.length === 0) {
        el("#hotkey-hint").textContent = `Add ${modifierNames()} to the key`;
        return;
      }

      settings.hotkey = [...modifiers, event.code].join("+");
      renderHotkey();
      stopCapture("Unsaved — click Save");
    },
    true,
  );

  window.addEventListener(
    "keyup",
    (event) => {
      if (!capturing || !MODIFIER_CODES.has(event.code)) return;
      // The last modifier came up with no key in between, so they were pressed alone.
      if (!event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey) {
        el("#hotkey-hint").textContent = "Modifiers alone can't be a shortcut — add a key";
      }
    },
    true,
  );

  window.addEventListener("blur", () => {
    if (capturing) stopCapture();
  });

  el("#save-general").addEventListener("click", async () => {
    settings.language = el<HTMLSelectElement>("#language").value;
    settings.insert_mode = insertMode;
    try {
      await invoke("save_settings", { settings });
      setStatus("Saved");
      stopCapture();
    } catch (error) {
      // The setting is stored, but the combination is owned by another app.
      setStatus(`Shortcut inactive: ${error}`);
      el("#hotkey-hint").textContent = "In use by another app — click to change";
    }
  });

  await listen<{ downloaded: number; total: number }>("model-progress", (event) => {
    const { downloaded, total } = event.payload;
    const percent = total ? Math.round((downloaded / total) * 100) : 0;
    el<HTMLDivElement>("#progress-fill").style.width = `${percent}%`;
    el("#progress-label").textContent = total
      ? `${percent}% of ${Math.round(total / 1_000_000)} MB`
      : `${Math.round(downloaded / 1_000_000)} MB`;
  });

  const recordButton = el<HTMLButtonElement>("#record-toggle");
  const transcriptState = el("#transcript-state");
  let recording = false;

  recordButton.addEventListener("click", async () => {
    try {
      if (recording) {
        const text = await invoke<string>("stop_dictation");
        transcriptState.textContent = text ? text : "No speech detected.";
      } else {
        await invoke("start_dictation");
        transcriptState.textContent = "Listening…";
      }
    } catch (error) {
      transcriptState.textContent = `Error: ${error}`;
    }
  });

  await listen<boolean>("recording", (event) => {
    recording = event.payload;
    recordButton.textContent = recording ? "Stop recording" : "Start recording";
  });

  await listen<string>("transcript", (event) => {
    transcriptState.textContent = event.payload;
  });

  await listen<string>("dictation-error", (event) => {
    transcriptState.textContent = `Error: ${event.payload}`;
  });

  await load();
});
