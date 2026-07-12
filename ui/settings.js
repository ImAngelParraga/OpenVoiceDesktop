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
const translations = {
  en: {
    general: "General", asr: "ASR Endpoint", advanced: "Advanced", history: "History", debug: "Debug", about: "About",
    save: "Save settings", loading: "Loading settings…", loaded: "Settings loaded.",
  },
  de: {
    general: "Allgemein", asr: "ASR-Endpunkt", advanced: "Erweitert", history: "Verlauf", debug: "Debug", about: "Über",
    save: "Einstellungen speichern", loading: "Einstellungen werden geladen…", loaded: "Einstellungen geladen.",
  },
  es: {
    general: "General", asr: "Endpoint ASR", advanced: "Avanzado", history: "Historial", debug: "Depuración", about: "Acerca de",
    save: "Guardar configuración", loading: "Cargando configuración…", loaded: "Configuración cargada.",
  },
  fr: {
    general: "Général", asr: "Point de terminaison ASR", advanced: "Avancé", history: "Historique", debug: "Débogage", about: "À propos",
    save: "Enregistrer les paramètres", loading: "Chargement des paramètres…", loaded: "Paramètres chargés.",
  },
  ja: {
    general: "一般", asr: "ASR エンドポイント", advanced: "詳細", history: "履歴", debug: "デバッグ", about: "情報",
    save: "設定を保存", loading: "設定を読み込み中…", loaded: "設定を読み込みました。",
  },
};

let documentModel;
let activeSection = "general";
const content = document.querySelector("#content");
const form = document.querySelector("#settings-form");
const status = document.querySelector("#status");

function locale() {
  return translations[documentModel?.config?.ui?.language] ? documentModel.config.ui.language : "en";
}

function translate(key) {
  return translations[locale()][key] ?? translations.en[key] ?? key;
}

function applyLocale() {
  const selected = locale();
  document.documentElement.lang = selected;
  document.title = `OpenVoiceDesktop ${translate("about")}`;
  for (const button of document.querySelectorAll("nav button")) {
    button.textContent = translate(button.dataset.section);
  }
  content.querySelector("h2").textContent = translate(activeSection);
  document.querySelector("#save").textContent = translate("save");
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
}

function getPath(root, path) {
  return path.split(".").reduce((current, part) => current?.[part], root);
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
  if (path === "audio.selected_output_device") {
    return ["", ...(documentModel.output_devices ?? []).map((device) => device.name)];
  }
  return null;
}

function render() {
  form.replaceChildren();
  const availability = new Map(documentModel.fields.map((field) => [field.field, field]));
  for (const control of activeControls()) {
    const path = control.config_key;
    if (!path) continue;
    if (path === "asr.bearer_token") continue;
    if (path.startsWith("updates.")) continue;
    const contract = availability.get(path);
    if (!contract) continue;
    const value = getPath(documentModel.config, path);
    const row = document.createElement("label");
    row.dataset.field = path;
    row.title = control.description;
    const unavailableReason = contract.status === "unavailable"
      ? contract.reason ?? "Unavailable in this build."
      : feedbackDependent(path) && !documentModel.config.audio.audio_feedback
        ? "Enable Audio feedback to use this option."
        : recordingOutputDependent(path)
          ? "Select reduced output mode to set recording volume."
        : null;
    const input = document.createElement(control.kind === "text-area" ? "textarea" : "input");
    input.name = path;
    input.disabled = unavailableReason !== null;
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
      select.addEventListener("change", () => {
        setPath(documentModel.config, path, select.value || null);
      });
      row.append(fieldCopy(control), select);
      addFieldNote(row, unavailableReason ? `Unavailable: ${unavailableReason}` : null, "field-note unavailable-reason");
      if (documentModel.input_device_fallback) {
        addFieldNote(row, documentModel.input_device_fallback);
      }
      if (documentModel.output_device_fallback && path === "audio.selected_output_device") {
        addFieldNote(row, documentModel.output_device_fallback);
      }
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
        setPath(documentModel.config, path, select.value);
        if (path === "audio.recording_output_mode") render();
      });
      row.append(fieldCopy(control), select);
      addFieldNote(row, unavailableReason ? `Unavailable: ${unavailableReason}` : null, "field-note unavailable-reason");
      form.append(row);
      continue;
    }
    if (typeof displayValue === "boolean") {
      input.type = "checkbox";
      input.checked = displayValue;
    } else if (typeof displayValue === "number") {
      input.type = "number";
      input.step = "any";
      input.value = displayValue;
    } else if (Array.isArray(displayValue)) {
      input.type = "text";
      input.value = displayValue.join(", ");
    } else {
      input.type = "text";
      input.value = displayValue ?? "";
    }
    if (input.disabled) input.title = unavailableReason;
    input.addEventListener("change", () => {
      let next = input.value;
      if (input.type === "checkbox") next = input.checked;
      else if (typeof value === "number") next = Number(input.value);
      else if (Array.isArray(value)) next = input.value.split(",").map((part) => part.trim()).filter(Boolean);
      else if (value === null && input.value === "") next = null;
      setPath(documentModel.config, path, next);
      if (path === "audio.audio_feedback") render();
      if (path === "ui.language") {
        applyLocale();
        render();
      }
      if (path === "ui.theme") applyTheme(documentModel.config.ui.theme);
    });
    row.append(fieldCopy(control), input);
    addFieldNote(row, unavailableReason ? `Unavailable: ${unavailableReason}` : null, "field-note unavailable-reason");
    form.append(row);
  }
  if (activeSection === "history") renderHistoryActions();
  if (activeSection === "debug") renderDebugActions();
  if (activeSection === "asr") renderAsrActions();
  if (activeSection === "about") renderAbout();
  renderOnboarding();
}

function actionButton(label, command, args = {}) {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  button.addEventListener("click", async () => {
    try {
      status.textContent = await invoke(command, args);
      documentModel = await invoke("get_settings");
      render();
    } catch (error) {
      status.textContent = `${label} failed: ${error}`;
    }
  });
  return button;
}

function renderHistoryActions() {
  const toolbar = document.createElement("div");
  toolbar.className = "history-actions";
  toolbar.append(
    actionButton("Copy last transcript", "copy_last_transcript"),
    actionButton("Open recordings folder", "open_recordings_folder"),
  );
  form.append(toolbar);
  for (const entry of [...(documentModel.history_entries ?? [])].reverse()) {
    const row = document.createElement("article");
    row.className = "history-entry";
    const text = document.createElement("p");
    text.textContent = entry.text;
    row.append(
      text,
      actionButton("Retry", "retry_transcript", { timestampUnixMs: entry.timestamp_unix_ms }),
      actionButton("Save", "save_transcript", { timestampUnixMs: entry.timestamp_unix_ms }),
      actionButton("Delete", "delete_transcript", { timestampUnixMs: entry.timestamp_unix_ms }),
    );
    form.append(row);
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
  }
}

function renderAsrActions() {
  const statusLine = document.createElement("p");
  statusLine.textContent = documentModel.asr_token_status?.display ?? "Token status unavailable";
  const token = document.createElement("input");
  token.type = "password";
  token.placeholder = "New ASR token";
  const fallback = document.createElement("input");
  fallback.type = "checkbox";
  const fallbackLabel = document.createElement("label");
  fallbackLabel.append(fallback, " Allow plaintext fallback");
  const save = document.createElement("button");
  save.type = "button";
  save.textContent = "Store token";
  save.addEventListener("click", async () => {
    try {
      status.textContent = await invoke("set_asr_token", {
        token: token.value,
        plaintextFallback: fallback.checked,
      });
      token.value = "";
      documentModel = await invoke("get_settings");
      render();
    } catch (error) {
      status.textContent = `Token save failed: ${error}`;
    }
  });
  const test = document.createElement("button");
  test.type = "button";
  test.textContent = "Test endpoint";
  test.addEventListener("click", async () => {
    try {
      const result = await invoke("test_endpoint");
      status.textContent = `HTTP ${result.http_status}; ${result.latency_ms} ms; ${result.sanitized_body}`;
    } catch (error) {
      status.textContent = `Endpoint test failed: ${error}`;
    }
  });
  form.append(statusLine, token, fallbackLabel, save, test);
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
  if (onboarding.completed && !onboarding.show_permission_repair) return;
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
    const command = onboarding.completed ? "repair_permissions" : "complete_onboarding";
    try {
      status.textContent = await invoke(command, { permissionsConfirmed: permissions.checked });
      documentModel = await invoke("get_settings");
      render();
    } catch (error) {
      status.textContent = `Setup failed: ${error}`;
    }
  });
  panel.append(title, steps, permissionLabel, finish);
  form.prepend(panel);
}

for (const button of document.querySelectorAll("nav button")) {
  button.addEventListener("click", () => {
    document.querySelector("nav button.active")?.classList.remove("active");
    button.classList.add("active");
    activeSection = button.dataset.section;
    content.querySelector("h2").textContent = translate(activeSection);
    render();
    content.focus();
  });
}

document.querySelector("#save").addEventListener("click", async () => {
  const activeSectionBeforeSave = activeSection;
  status.textContent = "Saving…";
  try {
    const result = await invoke("save_settings", { request: { config: documentModel.config } });
    const applied = result.fields.filter((field) => field.status === "applied").length;
    const restart = result.fields.filter((field) => field.status === "restart-required").length;
    status.textContent = `Saved. ${applied} applied; ${restart} restart-required.`;
  } catch (error) {
    status.textContent = `Save failed: ${error}`;
  } finally {
    activeSection = activeSectionBeforeSave;
    await settingsWindow.show();
    await settingsWindow.setFocus();
  }
});

try {
  documentModel = await invoke("get_settings");
  applyLocale();
  applyTheme(documentModel.config.ui.theme);
  status.textContent = translate("loaded");
  render();
} catch (error) {
  status.textContent = `Unable to load settings: ${error}`;
}
