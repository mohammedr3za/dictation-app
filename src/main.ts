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
  prompt_enabled: boolean;
  prompt_base_url: string;
  prompt_model: string;
}

interface ModelInfo {
  id: string;
  name: string;
  note: string;
  size_mb: number;
  recommended: boolean;
  installed: boolean;
}

let settings: Settings;
let insertMode: InsertMode = "paste";
let platform = "linux";

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

// Keys that type a character. With one modifier they collide with typing or
// app shortcuts (⌘X, ⇧K, Ctrl+C), so they need two.
const CHARACTER_CODE =
  /^(Key[A-Z]|Digit[0-9]|Numpad[0-9]|Backquote|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Comma|Period|Slash)$/;

function modifierLabels(): Record<string, string> {
  return platform === "macos"
    ? { Control: "⌃", Alt: "⌥", Shift: "⇧", Command: "⌘" }
    : { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Command: "Super" };
}

function displayHotkey(accelerator: string): string {
  const parts = accelerator.split("+");
  const labels = modifierLabels();
  const separator = platform === "macos" ? "" : "+";

  // A modifier-only shortcut such as "Alt+Command" has no key part.
  const last = parts[parts.length - 1] ?? "";
  if (last in labels) {
    return parts.map((modifier) => labels[modifier] ?? modifier).join(separator);
  }

  const modifiers = parts.slice(0, -1).map((modifier) => labels[modifier] ?? modifier);
  const keyLabel = last
    .replace(/^Key/, "")
    .replace(/^Digit/, "")
    .replace(/^Numpad/, "Num");
  return [...modifiers, keyLabel].join(separator);
}

function modifierNames(): string {
  return platform === "macos" ? "⌘, ⌥, ⌃ or ⇧" : "Ctrl, Alt, Shift or Super";
}

function heldModifiers(event: KeyboardEvent): string[] {
  const modifiers: string[] = [];
  if (event.ctrlKey) modifiers.push("Control");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (event.metaKey) modifiers.push("Command");
  return modifiers;
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
// The most modifiers held at once during this capture; a modifier-only
// shortcut is taken from it once they are all released.
let peakModifiers: string[] = [];

const hotkeyButton = (): HTMLButtonElement => el<HTMLButtonElement>("#hotkey");

function renderHotkey() {
  el("#hotkey-value").textContent = displayHotkey(settings.hotkey);
}

function showHotkeyProblem(problem: string | null) {
  const line = el("#hotkey-problem");
  line.textContent = problem ?? "";
  line.classList.toggle("hidden", !problem);
}

async function renderPromptKeyState() {
  const saved = await invoke<boolean>("has_prompt_key");
  el("#prompt-key-state").textContent = saved
    ? "A key is saved in the Keychain."
    : "No key saved.";
  el<HTMLInputElement>("#prompt-key").placeholder = saved
    ? "Saved — type to replace"
    : "Paste an API key";
}

function showPromptProblem(problem: string | null) {
  const line = el("#prompt-problem");
  line.textContent = problem ?? "";
  line.classList.toggle("hidden", !problem);
}

function startCapture() {
  capturing = true;
  peakModifiers = [];
  hotkeyButton().classList.add("capturing");
  el("#hotkey-hint").textContent = "Press modifiers, with or without a key…";
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

function captured(accelerator: string) {
  settings.hotkey = accelerator;
  renderHotkey();
  stopCapture("Unsaved — click Save");
}

// ---------- models ----------

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
        <h3>${model.name}${model.recommended ? ' <span class="tag">Recommended</span>' : ""}</h3>
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
        await invoke("save_settings", { settings }).catch(console.error);
        setStatus(`Using ${model.name}`);
        await renderModels();
      });
      actions.appendChild(use);

      // Two clicks, so a stray one cannot throw away a 460 MB download.
      const remove = document.createElement("button");
      remove.className = "secondary";
      remove.textContent = "Delete";
      let armed = false;
      remove.addEventListener("click", async () => {
        if (!armed) {
          armed = true;
          remove.textContent = "Click again to delete";
          remove.classList.add("danger");
          setTimeout(() => {
            armed = false;
            remove.textContent = "Delete";
            remove.classList.remove("danger");
          }, 3000);
          return;
        }
        remove.disabled = true;
        try {
          await invoke("delete_model", { id: model.id });
          setStatus(`${model.name} deleted`);
        } catch (error) {
          setStatus("Delete failed");
          console.error(error);
        }
        await renderModels();
      });
      actions.appendChild(remove);
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
          await invoke("save_settings", { settings }).catch(console.error);
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
  showHotkeyProblem(await invoke<string | null>("hotkey_status"));

  el<HTMLInputElement>("#prompt-enabled").checked = settings.prompt_enabled;
  el<HTMLInputElement>("#prompt-base-url").value = settings.prompt_base_url;
  el<HTMLInputElement>("#prompt-model").value = settings.prompt_model;
  await renderPromptKeyState();

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

      const modifiers = heldModifiers(event);
      if (MODIFIER_CODES.has(event.code)) {
        if (modifiers.length >= peakModifiers.length) peakModifiers = modifiers;
        return;
      }

      if (!CAPTURABLE_CODE.test(event.code)) {
        el("#hotkey-hint").textContent = "That key can't be used — try another";
        return;
      }
      if (modifiers.length === 0) {
        el("#hotkey-hint").textContent = `Add ${modifierNames()} to the key`;
        return;
      }
      if (CHARACTER_CODE.test(event.code) && modifiers.length < 2) {
        el("#hotkey-hint").textContent =
          `Letters need two modifiers, like ${displayHotkey("Alt+Command+KeyK")}`;
        return;
      }

      captured([...modifiers, event.code].join("+"));
    },
    true,
  );

  window.addEventListener(
    "keyup",
    (event) => {
      if (!capturing || !MODIFIER_CODES.has(event.code)) return;
      // Wait until the last modifier is released, with no key pressed meanwhile.
      if (heldModifiers(event).length > 0) return;

      const chord = peakModifiers;
      peakModifiers = [];
      if (chord.length >= 2) {
        captured(chord.join("+"));
      } else {
        el("#hotkey-hint").textContent =
          `Use two modifiers, like ${displayHotkey("Alt+Command")}, or add a key`;
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
      showHotkeyProblem(null);
      stopCapture();
    } catch (error) {
      // The setting is stored, but the shortcut could not be activated.
      setStatus("Saved, shortcut inactive");
      showHotkeyProblem(String(error));
    }
  });

  el("#save-prompt").addEventListener("click", async () => {
    const keyInput = el<HTMLInputElement>("#prompt-key");
    const next: Settings = {
      ...settings,
      prompt_enabled: el<HTMLInputElement>("#prompt-enabled").checked,
      prompt_base_url: el<HTMLInputElement>("#prompt-base-url").value.trim(),
      prompt_model: el<HTMLInputElement>("#prompt-model").value.trim(),
    };
    try {
      await invoke("save_prompt_settings", {
        promptEnabled: next.prompt_enabled,
        promptBaseUrl: next.prompt_base_url,
        promptModel: next.prompt_model,
      });
    } catch (error) {
      showPromptProblem(String(error));
      setStatus("Prompt settings not saved");
      return;
    }
    settings = next;
    if (keyInput.value) {
      try {
        await invoke("save_prompt_key", { key: keyInput.value });
        keyInput.value = "";
      } catch (error) {
        showPromptProblem(`Settings saved, key not saved: ${error}`);
        setStatus("Settings saved, key not saved");
        await renderPromptKeyState();
        return;
      }
    }
    showPromptProblem(null);
    setStatus("Saved");
    await renderPromptKeyState();
  });

  el("#remove-prompt-key").addEventListener("click", async () => {
    try {
      await invoke("save_prompt_key", { key: "" });
      el<HTMLInputElement>("#prompt-key").value = "";
      showPromptProblem(null);
      await renderPromptKeyState();
      setStatus("API key removed");
    } catch (error) {
      showPromptProblem(String(error));
      setStatus("Could not remove the API key");
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
    if (recording) transcriptState.textContent = "Listening…";
  });

  await listen<string>("transcript", (event) => {
    transcriptState.textContent = event.payload;
  });

  await listen<string>("dictation-error", (event) => {
    transcriptState.textContent = `Error: ${event.payload}`;
  });

  await listen<{ state: string; message?: string | null }>("prompt-status", (event) => {
    const { state, message } = event.payload;
    if (state === "structuring") {
      transcriptState.textContent = "Structuring prompt…";
      setStatus("Structuring prompt…");
    } else if (state === "failed") {
      setStatus(message ?? "Prompt structuring failed");
    } else {
      setStatus("Prompt ready");
    }
  });

  await load();
});
