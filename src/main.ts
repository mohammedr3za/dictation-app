import { invoke } from "@tauri-apps/api/core";

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


let settings: Settings;
let insertMode: InsertMode = "paste";

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

async function load() {
  settings = await invoke<Settings>("load_settings");

  el<HTMLInputElement>("#hotkey").value = settings.hotkey;
  el<HTMLSelectElement>("#language").value = settings.language;
  el<HTMLInputElement>("#cleanup-enabled").checked = settings.cleanup_enabled;
  el<HTMLInputElement>("#cleanup-url").value = settings.cleanup_base_url;
  el<HTMLInputElement>("#cleanup-model").value = settings.cleanup_model;
  insertMode = settings.insert_mode;
  renderInsertMode();

  const hasKey = await invoke<boolean>("has_api_key");
  el("#key-state").textContent = hasKey
    ? "A key is stored in your system keychain."
    : "No key stored.";

  const info = await invoke<{ version: string; platform: string; arch: string }>("app_info");
  el("#version").textContent = info.version;
  el("#platform-detail").textContent = `${info.platform} · ${info.arch}`;
  el("#platform").textContent = `${info.platform} · local`;
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

  el("#save-general").addEventListener("click", async () => {
    settings.hotkey = el<HTMLInputElement>("#hotkey").value;
    settings.language = el<HTMLSelectElement>("#language").value;
    settings.insert_mode = insertMode;
    await invoke("save_settings", { settings });
    setStatus("Saved");
  });

  el("#save-cleanup").addEventListener("click", async () => {
    settings.cleanup_enabled = el<HTMLInputElement>("#cleanup-enabled").checked;
    settings.cleanup_base_url = el<HTMLInputElement>("#cleanup-url").value;
    settings.cleanup_model = el<HTMLInputElement>("#cleanup-model").value;
    await invoke("save_settings", { settings });

    const key = el<HTMLInputElement>("#cleanup-key").value;
    if (key) {
      await invoke("save_api_key", { key });
      el<HTMLInputElement>("#cleanup-key").value = "";
      el("#key-state").textContent = "A key is stored in your system keychain.";
    }
    setStatus("Saved");
  });

  await load();
});
