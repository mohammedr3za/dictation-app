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

  el<HTMLInputElement>("#hotkey").value = settings.hotkey;
  el<HTMLSelectElement>("#language").value = settings.language;
  insertMode = settings.insert_mode;
  renderInsertMode();

  const info = await invoke<{ version: string; platform: string; arch: string }>("app_info");
  el("#version").textContent = info.version;
  el("#platform-detail").textContent = `${info.platform} · ${info.arch}`;
  el("#platform").textContent = `${info.platform} · local`;

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

  el("#save-general").addEventListener("click", async () => {
    settings.hotkey = el<HTMLInputElement>("#hotkey").value;
    settings.language = el<HTMLSelectElement>("#language").value;
    settings.insert_mode = insertMode;
    await invoke("save_settings", { settings });
    setStatus("Saved");
  });

  await listen<{ downloaded: number; total: number }>("model-progress", (event) => {
    const { downloaded, total } = event.payload;
    const percent = total ? Math.round((downloaded / total) * 100) : 0;
    el<HTMLDivElement>("#progress-fill").style.width = `${percent}%`;
    el("#progress-label").textContent = total
      ? `${percent}% of ${Math.round(total / 1_000_000)} MB`
      : `${Math.round(downloaded / 1_000_000)} MB`;
  });

  await load();
});
