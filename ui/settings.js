import { CUSTOM_ASR_MODEL, deviceFallback, modelCatalogIsCurrent, normalizeFieldValue, numericSpecs, selectedAsrModelChoice, summarizeSave } from "./settings_logic.mjs";

const { invoke } = window.__TAURI__.core;
const settingsWindow = window.__TAURI__.window.getCurrentWindow();

const layoutSectionIds = {
  general: "general",
  asr: "asr-endpoint",
  advanced: "advanced",
  history: "history",
  debug: "debug",
  about: "about",
};
const groupHeadings = {
  "behavior.shortcut": "Keyboard shortcuts",
  "audio.selected_microphone": "Recording",
  "audio.audio_feedback": "Sound feedback",
  "audio.sample_rate_hz": "Audio format",
  "app.start_hidden": "Startup and appearance",
  "output.paste_method": "Text insertion",
  "text.append_trailing_space": "Transcript processing",
};
const translations = {
  en: {
    general: "General", asr: "ASR Endpoint", advanced: "Advanced", history: "History", debug: "Debug", about: "About",
    save: "Save settings", settings: "Settings", loading: "Loading settings…", loaded: "Settings loaded.",
  },
  de: {
    general: "Allgemein", asr: "ASR-Endpunkt", advanced: "Erweitert", history: "Verlauf", debug: "Debug", about: "Über",
    save: "Einstellungen speichern", settings: "Einstellungen", loading: "Einstellungen werden geladen…", loaded: "Einstellungen geladen.",
  },
  es: {
    general: "General", asr: "Endpoint ASR", advanced: "Avanzado", history: "Historial", debug: "Depuración", about: "Acerca de",
    save: "Guardar configuración", settings: "Configuración", loading: "Cargando configuración…", loaded: "Configuración cargada.",
  },
  fr: {
    general: "Général", asr: "Point de terminaison ASR", advanced: "Avancé", history: "Historique", debug: "Débogage", about: "À propos",
    save: "Enregistrer les paramètres", settings: "Paramètres", loading: "Chargement des paramètres…", loaded: "Paramètres chargés.",
  },
  ja: {
    general: "一般", asr: "ASR エンドポイント", advanced: "詳細", history: "履歴", debug: "デバッグ", about: "情報",
    save: "設定を保存", settings: "設定", loading: "設定を読み込み中…", loaded: "設定を読み込みました。",
  },
};

let documentModel;
let savedConfig;
let historyPage = 0;
const historyPageSize = 25;
let activeSection = "general";
const content = document.querySelector("#content");
const form = document.querySelector("#settings-form");
const status = document.querySelector("#status");
const saveButton = document.querySelector("#save");
const saveResult = document.querySelector("#save-result");
const saveDetails = document.querySelector("#save-details");
const saveDetailList = document.querySelector("#save-detail-list");
let saveInProgress = false;
let saveFeedbackTimer;
let logReadInProgress = false;
let asrModels = [];
let asrModelsEndpoint = "";
let asrModelStatus = "Loading available models…";
let asrModelRequest = 0;

function locale() {
  return translations[documentModel?.config?.ui?.language] ? documentModel.config.ui.language : "en";
}

function translate(key) {
  return translations[locale()][key] ?? translations.en[key] ?? key;
}

function applyLocale() {
  const selected = locale();
  document.documentElement.lang = selected;
  document.title = `OpenVoiceDesktop ${translate("settings")}`;
  for (const button of document.querySelectorAll("nav button")) {
    button.textContent = translate(button.dataset.section);
  }
  content.querySelector("h2").textContent = translate(activeSection);
  if (!saveInProgress) saveButton.textContent = translate("save");
}

function clearTransientSaveFeedback(keepDetails = false) {
  clearTimeout(saveFeedbackTimer);
  if (saveResult.dataset.state === "success") {
    saveResult.textContent = "";
    saveResult.removeAttribute("data-state");
    if (!keepDetails) {
      saveDetails.hidden = true;
      saveDetails.open = false;
      saveDetailList.replaceChildren();
    }
  }
}

function applyTheme(theme) {
  const selected = ["system", "light", "dark"].includes(theme) ? theme : "system";
  document.documentElement.dataset.theme = selected;
  document.documentElement.style.colorScheme = selected === "system" ? "light dark" : selected;
}

function setPath(root, path, value) {
  const parts = path.split(".");
  const leaf = parts.pop();
  const owner = parts.reduce((current, part) => current[part], root);
  owner[leaf] = value;
  updateDirtyIndicator();
}

function getPath(root, path) {
  return path.split(".").reduce((current, part) => current?.[part], root);
}

function updateDirtyIndicator() {
  const changed = JSON.stringify(documentModel.config) !== JSON.stringify(savedConfig);
  document.querySelector("#dirty").hidden = !changed;
}

function activeControls() {
  const sectionId = layoutSectionIds[activeSection];
  return documentModel.layout.sections.find((section) => section.id === sectionId)?.controls ?? [];
}

function fieldCopy(control) {
  const copy = document.createElement("span");
  copy.className = "field-copy";
  const name = document.createElement("span");
  name.className = "field-name";
  name.textContent = control.label;
  const help = document.createElement("small");
  help.className = "field-help";
  help.textContent = control.description;
  copy.append(name, help);
  return copy;
}

function addFieldNote(row, text, className = "field-note") {
  if (!text) return;
  const note = document.createElement("small");
  note.className = className;
  note.textContent = text;
  row.append(note);
}

function feedbackDependent(path) {
  return [
    "audio.selected_output_device",
    "audio.audio_feedback_volume",
    "audio.sound_theme",
    "audio.custom_start_sound",
    "audio.custom_stop_sound",
    "audio.custom_error_sound",
  ].includes(path);
}

function recordingOutputDependent(path) {
  return path === "audio.recording_output_volume"
    && documentModel.config.audio.recording_output_mode !== "reduced";
}

function outputDelayUnavailable(path) {
  const output = documentModel.config.output;
  if (path === "output.paste_delay_ms" && (
    output.paste_method !== "clipboard-paste"
    || output.clipboard_handling === "copy-only"
  )) return "Used only by clipboard-paste output.";
  if (path === "output.restore_delay_ms" && (
    output.paste_method !== "clipboard-paste"
    || output.clipboard_handling !== "restore-after-paste"
  )) return "Used only when clipboard-paste restores the previous clipboard.";
  return null;
}

function selectOptions(path) {
  if (path === "audio.recording_output_mode") return ["unchanged", "reduced", "muted"];
  if (path === "audio.sound_theme") return ["default", "minimal", "custom"];
  if (path === "app.overlay_position") return ["top", "bottom"];
  if (path === "app.overlay_style") return ["compact", "expanded"];
  if (path === "ui.theme") return ["system", "light", "dark"];
  if (path === "ui.language") return (documentModel.locale_catalog?.supported ?? []).map((locale) => locale.code);
  if (path === "output.paste_method") return ["clipboard-paste", "direct-type", "copy-only", "external-script"];
  if (path === "output.typing_tool") return (documentModel.supported_typing_tools ?? []).map((tool) => String(tool));
  if (path === "output.clipboard_handling") return ["restore-after-paste", "leave-transcript", "copy-only"];
  if (path === "history.retention") return ["never", "failed-only", "always"];
  if (path === "debug.log_level") return ["error", "warn", "info", "debug", "trace"];
  return null;
}

function updateAsrModelPicker() {
  const row = form.querySelector('[data-field="asr.model"]');
  if (!row) return;
  const select = row.querySelector("select");
  const customInput = row.querySelector("input");
  const model = documentModel.config.asr.model;
  select.replaceChildren();
  for (const id of asrModels) {
    const option = document.createElement("option");
    option.value = id;
    option.textContent = id;
    select.append(option);
  }
  const customOption = document.createElement("option");
  customOption.value = CUSTOM_ASR_MODEL;
  customOption.textContent = "Custom model…";
  select.append(customOption);
  select.value = selectedAsrModelChoice(model, asrModels);
  const custom = select.value === CUSTOM_ASR_MODEL;
  customInput.value = custom ? model : "";
  customInput.hidden = !custom;
  customInput.required = custom;
  customInput.disabled = select.disabled || !custom;
  customInput.setCustomValidity(custom && !model.trim() ? "Enter a model ID." : "");
  row.querySelector(".model-status").textContent = asrModelStatus;
}

async function refreshAsrModels() {
  const baseUrl = documentModel.config.asr.base_url.trim();
  const request = ++asrModelRequest;
  if (baseUrl !== asrModelsEndpoint) {
    asrModels = [];
    asrModelsEndpoint = baseUrl;
  }
  if (baseUrl !== savedConfig.asr.base_url.trim()) {
    asrModelStatus = "Save this endpoint to load its models.";
    updateAsrModelPicker();
    return;
  }
  asrModelStatus = "Loading available models…";
  updateAsrModelPicker();
  try {
    const models = await invoke("list_asr_models", { baseUrl });
    if (!modelCatalogIsCurrent(request, asrModelRequest, baseUrl, documentModel.config.asr.base_url)) return;
    asrModels = models;
    asrModelStatus = `${models.length} transcription model${models.length === 1 ? "" : "s"} available from server.`;
  } catch (error) {
    if (!modelCatalogIsCurrent(request, asrModelRequest, baseUrl, documentModel.config.asr.base_url)) return;
    asrModels = [];
    asrModelStatus = `Model list unavailable: ${error}. Your saved model is still editable.`;
  }
  updateAsrModelPicker();
}

function render() {
  const focused = form.contains(document.activeElement) ? document.activeElement.name : null;
  const invalid = new Map(Array.from(form.querySelectorAll("input:invalid"))
    .map((input) => [input.name, [input.value, input.validationMessage]]));
  form.replaceChildren();
  const availability = new Map(documentModel.fields.map((field) => [field.field, field]));
  for (const control of activeControls()) {
    const path = control.config_key;
    if (!path) continue;
    if (path.startsWith("audio.custom_") && documentModel.config.audio.sound_theme !== "custom") continue;
    if (path === "asr.bearer_token") continue;
    if (path.startsWith("updates.")) continue;
    const contract = availability.get(path);
    if (!contract) continue;
    const group = groupHeadings[path];
    if (group) {
      const heading = document.createElement("h3");
      heading.textContent = group;
      form.append(heading);
    }
    const value = getPath(documentModel.config, path);
    const row = document.createElement(path === "asr.model" ? "div" : "label");
    if (path === "asr.model") row.className = "model-row";
    row.dataset.field = path;
    row.title = control.description;
    const unavailableReason = contract.status === "unavailable"
      ? contract.reason ?? "Unavailable in this build."
      : feedbackDependent(path) && !documentModel.config.audio.audio_feedback
        ? "Enable Audio feedback to use this option."
        : recordingOutputDependent(path)
          ? "Select reduced output mode to set recording volume."
          : outputDelayUnavailable(path);
    const isTextArea = control.kind === "text-area";
    const input = document.createElement(isTextArea ? "textarea" : "input");
    input.name = path;
    input.disabled = unavailableReason !== null;
    if (path === "asr.model") {
      const picker = document.createElement("div");
      picker.className = "model-picker";
      const select = document.createElement("select");
      select.name = path;
      select.setAttribute("aria-label", "ASR model");
      select.disabled = input.disabled;
      const customInput = document.createElement("input");
      customInput.type = "text";
      customInput.setAttribute("aria-label", "Custom ASR model ID");
      customInput.placeholder = "Model ID from your ASR server";
      select.addEventListener("change", () => {
        clearTransientSaveFeedback();
        const custom = select.value === CUSTOM_ASR_MODEL;
        customInput.hidden = !custom;
        customInput.disabled = select.disabled || !custom;
        customInput.required = custom;
        customInput.setCustomValidity(custom && !customInput.value.trim() ? "Enter a model ID." : "");
        setPath(documentModel.config, path, custom ? customInput.value.trim() : select.value);
        if (custom) customInput.focus();
      });
      customInput.addEventListener("input", () => {
        clearTransientSaveFeedback();
        customInput.setCustomValidity(customInput.value.trim() ? "" : "Enter a model ID.");
        setPath(documentModel.config, path, customInput.value.trim());
      });
      const refresh = document.createElement("button");
      refresh.type = "button";
      refresh.textContent = "Refresh models";
      refresh.addEventListener("click", refreshAsrModels);
      picker.append(select, customInput, refresh);
      row.append(fieldCopy(control), picker);
      addFieldNote(row, asrModelStatus, "field-note model-status");
      addFieldNote(row, unavailableReason ? `Unavailable: ${unavailableReason}` : null, "field-note unavailable-reason");
      form.append(row);
      updateAsrModelPicker();
      continue;
    }
    if (path === "audio.selected_microphone" || path === "audio.selected_output_device") {
      const select = document.createElement("select");
      select.name = path;
      select.disabled = input.disabled;
      const defaultOption = document.createElement("option");
      defaultOption.value = "";
      defaultOption.textContent = path === "audio.selected_microphone" ? "System default" : "System default output";
      defaultOption.selected = value == null;
      select.append(defaultOption);
      for (const device of (path === "audio.selected_microphone"
        ? documentModel.input_devices ?? []
        : documentModel.output_devices ?? [])) {
        const option = document.createElement("option");
        option.value = device.name;
        option.textContent = device.is_default ? `${device.name} (default)` : device.name;
        option.selected = value === device.name;
        select.append(option);
      }
      const devices = path === "audio.selected_microphone"
        ? documentModel.input_devices ?? []
        : documentModel.output_devices ?? [];
      if (value && !devices.some((device) => device.name === value)) {
        const missing = document.createElement("option");
        missing.value = value;
        missing.textContent = `${value} (unavailable; using default)`;
        missing.selected = true;
        select.append(missing);
      }
      select.addEventListener("change", () => {
        clearTransientSaveFeedback();
        setPath(documentModel.config, path, select.value || null);
        render();
      });
      row.append(fieldCopy(control), select);
      addFieldNote(row, unavailableReason ? `Unavailable: ${unavailableReason}` : null, "field-note unavailable-reason");
      addFieldNote(row, deviceFallback(value, devices, path === "audio.selected_microphone" ? "Microphone" : "Output device"));
      form.append(row);
      continue;
    }
    const displayValue = path === "audio.sample_rate_hz"
      ? documentModel.audio_encoding.sample_rate_hz
      : path === "audio.channels"
        ? documentModel.audio_encoding.channels
        : value;
    const options = selectOptions(path);
    if (options) {
      const select = document.createElement("select");
      select.name = path;
      select.disabled = input.disabled;
      for (const optionValue of options) {
        const option = document.createElement("option");
        option.value = optionValue;
        option.textContent = path === "ui.language"
          ? (documentModel.locale_catalog?.supported ?? []).find((locale) => locale.code === optionValue)?.label ?? optionValue
          : optionValue || "System default";
        option.selected = String(displayValue ?? "") === optionValue;
        select.append(option);
      }
      select.addEventListener("change", () => {
        clearTransientSaveFeedback();
        setPath(documentModel.config, path, select.value);
        if (path === "ui.language") applyLocale();
        if (path === "ui.theme") applyTheme(select.value);
        if (["audio.recording_output_mode", "audio.sound_theme", "output.paste_method", "output.clipboard_handling", "ui.language"].includes(path)) render();
      });
      row.append(fieldCopy(control), select);
      addFieldNote(row, unavailableReason ? `Unavailable: ${unavailableReason}` : null, "field-note unavailable-reason");
      form.append(row);
      continue;
    }
    if (!isTextArea) input.type = "text";
    if (isTextArea) {
      input.value = displayValue ?? "";
    } else if (typeof displayValue === "boolean") {
      input.type = "checkbox";
      input.checked = displayValue;
    } else if (typeof displayValue === "number") {
      input.type = control.kind === "slider" ? "range" : "number";
      const spec = numericSpecs[path];
      if (spec) {
        input.min = String(spec.min);
        input.max = String(spec.max);
        input.step = String(spec.step);
      }
      input.value = displayValue;
      if (input.type === "number") input.required = true;
    } else if (Array.isArray(displayValue)) {
      input.type = "text";
      input.value = displayValue.join(", ");
    } else {
      input.type = "text";
      input.value = displayValue ?? "";
    }
    if (invalid.has(path) && !input.disabled) {
      const [raw, message] = invalid.get(path);
      input.value = raw;
      input.setCustomValidity(message);
    }
    if (path.startsWith("audio.custom_")) input.required = true;
    if (input.disabled) input.title = unavailableReason;
    const errorNote = document.createElement("small");
    errorNote.className = "field-error";
    errorNote.hidden = true;
    const commitValue = () => {
      clearTransientSaveFeedback();
      try {
        const next = normalizeFieldValue(path, value, input.value, input.checked);
        input.setCustomValidity("");
        errorNote.textContent = "";
        errorNote.hidden = true;
        setPath(documentModel.config, path, next);
        if (path === "audio.audio_feedback" || path === "debug.live_log_viewer") render();
      } catch (error) {
        input.setCustomValidity(error.message);
        errorNote.textContent = error.message;
        errorNote.hidden = false;
      }
    };
    input.addEventListener(input.type === "checkbox" ? "change" : "input", commitValue);
    if (path === "asr.base_url") input.addEventListener("change", refreshAsrModels);
    if (input.type === "range") {
      const slider = document.createElement("span");
      slider.className = "slider-control";
      const output = document.createElement("output");
      output.textContent = String(displayValue);
      input.addEventListener("input", () => { output.textContent = input.value; });
      slider.append(input, output);
      row.append(fieldCopy(control), slider);
    } else {
      row.append(fieldCopy(control), input);
    }
    addFieldNote(row, unavailableReason ? `Unavailable: ${unavailableReason}` : null, "field-note unavailable-reason");
    row.append(errorNote);
    form.append(row);
  }
  if (activeSection === "history") renderHistoryActions();
  if (activeSection === "debug") renderDebugActions();
  if (activeSection === "asr") renderAsrActions();
  if (activeSection === "about") renderAbout();
  if (activeSection === "general") renderDeviceRefresh();
  renderOnboarding();
  if (focused) form.elements.namedItem(focused)?.focus?.({ preventScroll: true });
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  saveButton.click();
});


function actionButton(label, command, args = {}, afterAction) {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  button.addEventListener("click", async () => {
    button.disabled = true;
    status.textContent = `${label}…`;
    try {
      status.textContent = await invoke(command, args);
      if (afterAction) await afterAction();
    } catch (error) {
      status.textContent = `${label} failed: ${error}`;
    } finally {
      button.disabled = false;
    }
  });
  return button;
}

function renderDeviceRefresh() {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = "Refresh audio devices";
  button.addEventListener("click", async () => {
    button.disabled = true;
    status.textContent = "Refreshing audio devices…";
    try {
      const inventory = await invoke("refresh_audio_devices");
      documentModel.input_devices = inventory.input_devices;
      documentModel.output_devices = inventory.output_devices;
      status.textContent = "Audio devices refreshed. Unsaved selections were kept.";
      render();
    } catch (error) {
      status.textContent = `Audio-device refresh failed: ${error}`;
      button.disabled = false;
    }
  });
  form.append(button);
}

async function refreshLiveLogs() {
  const viewer = form.querySelector(".live-log-viewer");
  if (!viewer || logReadInProgress || document.visibilityState !== "visible") return;
  logReadInProgress = true;
  try {
    viewer.textContent = (await invoke("read_live_logs")).join("\n");
  } catch (error) {
    viewer.textContent = `Log refresh failed: ${error}`;
  } finally {
    logReadInProgress = false;
  }
}

setInterval(() => {
  if (activeSection === "debug" && documentModel?.config?.debug?.live_log_viewer) refreshLiveLogs();
}, 2000);

function renderHistoryActions() {
  const toolbar = document.createElement("div");
  toolbar.className = "history-actions";
  toolbar.append(
    actionButton("Copy last transcript", "copy_last_transcript"),
    actionButton("Open recordings folder", "open_recordings_folder"),
  );
  form.append(toolbar);
  const list = document.createElement("section");
  list.className = "history-list";
  form.append(list);
  populateHistory(list);
}

let historyRequest = 0;
async function refreshHistory() {
  const request = ++historyRequest;
  try {
    const entries = await invoke("list_history");
    if (request !== historyRequest) return;
    documentModel.history_entries = entries;
    const list = form.querySelector(".history-list");
    if (list) populateHistory(list);
  } catch (error) {
    status.textContent = `Could not refresh history: ${error}`;
  }
}

function populateHistory(list) {
  list.replaceChildren();
  const entries = documentModel.history_entries;
  const pages = Math.max(1, Math.ceil(entries.length / historyPageSize));
  historyPage = Math.min(historyPage, pages - 1);
  if (!entries.length) {
    list.textContent = "No transcripts in history.";
    return;
  }
  const end = entries.length - historyPage * historyPageSize;

  for (const entry of entries.slice(Math.max(0, end - historyPageSize), end).reverse()) {
    const row = document.createElement("article");
    row.className = "history-entry";
    const text = document.createElement("p");
    text.textContent = entry.text || (entry.status === "error" ? (entry.audio_path ? "Transcription failed — audio retained for retry." : "Transcription failed — no retained audio.") : "Empty transcript");
    const retry = entry.status === "error";
    const args = { timestampUnixMs: entry.timestamp_unix_ms };
    const copy = actionButton(retry ? "Retry transcription" : "Copy transcript", "retry_transcript", args, retry ? refreshHistory : null);
    copy.disabled = retry ? !entry.audio_path : !entry.text;
    const save = actionButton(entry.saved ? "Saved" : "Save transcript", "save_transcript", args, refreshHistory);
    save.disabled = entry.saved;
    row.append(text, copy, save, actionButton("Delete", "delete_transcript", args, refreshHistory));
    list.append(row);
  }
  if (pages > 1) {
    const pager = document.createElement("div");
    pager.className = "history-actions";
    for (const [label, offset] of [["Newer", -1], ["Older", 1]]) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = label;
      button.disabled = historyPage + offset < 0 || historyPage + offset >= pages;
      button.onclick = () => { historyPage += offset; populateHistory(list); };
      pager.append(button);
    }
    pager.append(`Page ${historyPage + 1} of ${pages}`);
    list.append(pager);
  }
}

function renderDebugActions() {
  const toolbar = document.createElement("div");
  toolbar.className = "debug-actions";
  toolbar.append(
    actionButton("Open log directory", "open_log_directory"),
    actionButton("Open app data directory", "open_app_data_directory"),
  );
  form.append(toolbar);
  if (documentModel.config.debug.live_log_viewer) {
    const viewer = document.createElement("pre");
    viewer.className = "live-log-viewer";
    viewer.textContent = (documentModel.live_log_lines ?? []).join("\n");
    form.append(viewer);
    refreshLiveLogs();
  }
}

function renderAsrActions() {
  const statusLine = document.createElement("p");
  statusLine.textContent = documentModel.asr_token_status?.display ?? "Token status unavailable";
  const group = document.createElement("section");
  group.className = "credentials";
  const heading = document.createElement("h3");
  heading.textContent = "Authentication";
  const token = document.createElement("input");
  token.type = "password";
  token.placeholder = "New ASR token";
  token.setAttribute("aria-label", token.placeholder);
  token.autocomplete = "new-password";
  const fallback = document.createElement("input");
  fallback.type = "checkbox";
  const fallbackLabel = document.createElement("label");
  fallbackLabel.append(fallback, " Allow plaintext fallback");
  const save = document.createElement("button");
  save.type = "button";
  save.textContent = "Store token";
  save.addEventListener("click", async () => {
    save.disabled = true;
    try {
      const tokenStatus = await invoke("set_asr_token", {
        token: token.value,
        plaintextFallback: fallback.checked,
      });
      token.value = "";
      documentModel.asr_token_status = tokenStatus;
      status.textContent = "ASR token stored.";
      const currentStatus = form.querySelector(".token-status");
      if (currentStatus) currentStatus.textContent = tokenStatus.display;
      if (activeSection === "asr") refreshAsrModels();
    } catch (error) {
      status.textContent = `Token save failed: ${error}`;
    } finally {
      save.disabled = false;
    }
  });
  const test = document.createElement("button");
  test.type = "button";
  test.textContent = "Test endpoint";
  test.addEventListener("click", async () => {
    if (JSON.stringify(savedConfig.asr) !== JSON.stringify(documentModel.config.asr)) {
      status.textContent = "Save your ASR settings before testing this endpoint.";
      return;
    }
    test.disabled = true;
    status.textContent = "Testing the saved endpoint…";
    try {
      const result = await invoke("test_endpoint");
      // A silent clip goes up, so `sanitized_body` carries whatever the model
      // emits for silence. Label it, or the reading looks like a bad transcript.
      status.textContent = `Endpoint check (silent test clip, not your microphone): HTTP ${result.http_status} in ${result.latency_ms} ms - server replied ${result.sanitized_body}`;
    } catch (error) {
      status.textContent = `Endpoint test failed: ${error}`;
    } finally {
      test.disabled = false;
    }
  });
  statusLine.className = "token-status";
  group.append(heading, statusLine, token, fallbackLabel, save, test);
  form.append(group);
}

function renderAbout() {
  const values = [
    ["Version", documentModel.about.version],
    ["License", documentModel.about.license],
    ["Repository", documentModel.about.repository_url],
    ["Config path", documentModel.about.config_path],
    ["App data path", documentModel.about.app_data_path],
    ["Logs path", documentModel.about.logs_path],
    ["Acknowledgement", (documentModel.about.acknowledgements ?? []).join(", ")],
  ];
  for (const [label, value] of values) {
    const row = document.createElement("p");
    row.className = "about-value";
    row.textContent = `${label}: ${value}`;
    form.append(row);
  }
}

function renderOnboarding() {
  const onboarding = documentModel.config.onboarding;
  if (activeSection !== "general" || (onboarding.completed && !onboarding.show_permission_repair)) return;
  const panel = document.createElement("section");
  panel.className = "onboarding-panel";
  const title = document.createElement("h3");
  title.textContent = onboarding.completed ? "Repair permissions" : "Finish setup";
  const steps = document.createElement("ol");
  for (const step of documentModel.onboarding_flow.steps ?? []) {
    const item = document.createElement("li");
    item.textContent = String(step).replaceAll("-", " ");
    steps.append(item);
  }
  const permissions = document.createElement("input");
  permissions.type = "checkbox";
  const permissionLabel = document.createElement("label");
  permissionLabel.append(permissions, " Microphone/accessibility permissions confirmed");
  const finish = document.createElement("button");
  finish.type = "button";
  finish.textContent = onboarding.completed ? "Confirm repair" : "Complete setup";
  finish.addEventListener("click", async () => {
    if (JSON.stringify(savedConfig) !== JSON.stringify(documentModel.config)) {
      status.textContent = "Save your settings before completing setup.";
      return;
    }
    finish.disabled = true;
    const command = onboarding.completed ? "repair_permissions" : "complete_onboarding";
    try {
      status.textContent = await invoke(command, { permissionsConfirmed: permissions.checked });
      for (const config of [documentModel.config, savedConfig]) {
        config.onboarding.completed = true;
        config.onboarding.show_permission_repair = false;
      }
      form.querySelector(".onboarding-panel")?.remove();
    } catch (error) {
      status.textContent = `Setup failed: ${error}`;
    } finally {
      finish.disabled = false;
    }
  });
  panel.append(title, steps, permissionLabel, finish);
  form.prepend(panel);
}

for (const button of document.querySelectorAll("nav button")) {
  button.addEventListener("click", () => {
    if (!documentModel || !form.reportValidity()) return;
    const previous = document.querySelector("nav button.active");
    previous?.classList.remove("active");
    previous?.removeAttribute("aria-current");
    button.setAttribute("aria-current", "page");
    button.classList.add("active");
    activeSection = button.dataset.section;
    content.querySelector("h2").textContent = translate(activeSection);
    render();
    if (activeSection === "asr") refreshAsrModels();
    if (activeSection === "history") refreshHistory();
    content.focus();
    document.querySelector(".section-scroll").scrollTop = 0;
  });
}

window.addEventListener("focus", () => {
  if (documentModel && activeSection === "asr") refreshAsrModels();
  if (documentModel && activeSection === "history") refreshHistory();
});

saveButton.addEventListener("click", async () => {
  if (saveInProgress) return;
  if (!documentModel) return;
  if (!form.reportValidity()) return;
  saveInProgress = true;
  const submittedConfig = JSON.parse(JSON.stringify(documentModel.config));
  clearTimeout(saveFeedbackTimer);
  status.textContent = "";
  saveResult.textContent = "Saving settings…";
  saveResult.dataset.state = "busy";
  saveDetails.hidden = true;
  saveDetailList.replaceChildren();
  saveButton.disabled = true;
  saveButton.setAttribute("aria-busy", "true");
  saveButton.textContent = "Saving…";
  try {
    await new Promise(requestAnimationFrame);
    const result = await invoke("save_settings", { request: { config: submittedConfig } });
    savedConfig = submittedConfig;
    updateDirtyIndicator();
    if (activeSection === "asr") refreshAsrModels();
    const labels = Object.fromEntries(documentModel.layout.sections.flatMap((section) =>
      section.controls.filter((control) => control.config_key).map((control) => [control.config_key, control.label])));
    const summary = summarizeSave(result.fields, labels);
    saveResult.textContent = summary.detail;
    saveResult.dataset.state = "success";
    saveDetails.hidden = summary.exceptions.length === 0;
    for (const field of summary.exceptions) {
      const item = document.createElement("li");
      item.textContent = `${labels[field.field] ?? field.field}: ${field.reason ?? field.status}`;
      saveDetailList.append(item);
    }
    saveFeedbackTimer = setTimeout(() => clearTransientSaveFeedback(true), 6000);
    applyLocale();
    applyTheme(documentModel.config.ui.theme);
  } catch (error) {
    saveResult.textContent = `Save failed: ${error}`;
    saveResult.dataset.state = "error";
  } finally {
    saveInProgress = false;
    saveButton.disabled = false;
    saveButton.setAttribute("aria-busy", "false");
    saveButton.textContent = translate("save");
  }
});

async function ensureRecordingService() {
  try {
    await invoke("ensure_recording_service");
  } catch (error) {
    status.textContent = `Recording service unavailable: ${error}`;
  }
}

await settingsWindow.onFocusChanged(({ payload: focused }) => {
  if (focused) void ensureRecordingService();
});

try {
  documentModel = await invoke("get_settings");
  savedConfig = JSON.parse(JSON.stringify(documentModel.config));
  saveButton.disabled = false;
  applyLocale();
  applyTheme(documentModel.config.ui.theme);
  status.textContent = translate("loaded");
  render();
  await ensureRecordingService();
} catch (error) {
  status.textContent = `Unable to load settings: ${error}`;
}
