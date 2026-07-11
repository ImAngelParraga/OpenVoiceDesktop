const { invoke } = window.__TAURI__.core;

const sections = {
  general: ["behavior", "audio"],
  asr: ["asr"],
  advanced: ["app", "output", "text", "post_processing", "ui", "updates", "portable"],
  history: ["history"],
  debug: ["debug", "diagnostics", "ipc"],
  about: ["onboarding"],
};
const labels = {
  general: "General",
  asr: "ASR Endpoint",
  advanced: "Advanced",
  history: "History",
  debug: "Debug",
  about: "About",
};

let documentModel;
let activeSection = "general";
const content = document.querySelector("#content");
const form = document.querySelector("#settings-form");
const status = document.querySelector("#status");

function flatten(value, prefix = "", output = new Map()) {
  for (const [key, child] of Object.entries(value)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (child !== null && typeof child === "object" && !Array.isArray(child)) {
      flatten(child, path, output);
    } else {
      output.set(path, child);
    }
  }
  return output;
}

function setPath(root, path, value) {
  const parts = path.split(".");
  const leaf = parts.pop();
  const owner = parts.reduce((current, part) => current[part], root);
  owner[leaf] = value;
}

function render() {
  form.replaceChildren();
  const allowed = sections[activeSection];
  const availability = new Map(documentModel.fields.map((field) => [field.field, field]));
  for (const [path, value] of flatten(documentModel.config)) {
    if (path === "schema_version" || !allowed.some((prefix) => path.startsWith(`${prefix}.`))) continue;
    const contract = availability.get(path);
    if (!contract) continue;
    const row = document.createElement("label");
    row.dataset.field = path;
    const name = document.createElement("span");
    name.textContent = path;
    const input = document.createElement("input");
    input.name = path;
    input.disabled = contract.status === "unavailable";
    if (path === "audio.selected_microphone") {
      const select = document.createElement("select");
      select.name = path;
      select.disabled = input.disabled;
      const defaultOption = document.createElement("option");
      defaultOption.value = "";
      defaultOption.textContent = "System default";
      defaultOption.selected = value == null;
      select.append(defaultOption);
      for (const device of documentModel.input_devices ?? []) {
        const option = document.createElement("option");
        option.value = device.name;
        option.textContent = device.is_default ? `${device.name} (default)` : device.name;
        option.selected = value === device.name;
        select.append(option);
      }
      select.addEventListener("change", () => {
        setPath(documentModel.config, path, select.value || null);
      });
      row.append(name, select);
      if (documentModel.input_device_fallback) {
        const note = document.createElement("small");
        note.textContent = documentModel.input_device_fallback;
        row.append(note);
      }
      form.append(row);
      continue;
    }
    if (typeof value === "boolean") {
      input.type = "checkbox";
      input.checked = value;
    } else if (typeof value === "number") {
      input.type = "number";
      input.step = "any";
      input.value = value;
    } else if (Array.isArray(value)) {
      input.type = "text";
      input.value = value.join(", ");
    } else {
      input.type = "text";
      input.value = value ?? "";
    }
    if (input.disabled) input.title = contract.reason ?? "Unavailable";
    input.addEventListener("change", () => {
      let next = input.value;
      if (input.type === "checkbox") next = input.checked;
      else if (typeof value === "number") next = Number(input.value);
      else if (Array.isArray(value)) next = input.value.split(",").map((part) => part.trim()).filter(Boolean);
      else if (value === null && input.value === "") next = null;
      setPath(documentModel.config, path, next);
    });
    row.append(name, input);
    form.append(row);
  }
}

for (const button of document.querySelectorAll("nav button")) {
  button.addEventListener("click", () => {
    document.querySelector("nav button.active")?.classList.remove("active");
    button.classList.add("active");
    activeSection = button.dataset.section;
    content.querySelector("h2").textContent = labels[activeSection];
    render();
    content.focus();
  });
}

document.querySelector("#save").addEventListener("click", async () => {
  status.textContent = "Saving…";
  try {
    const result = await invoke("save_settings", { request: { config: documentModel.config } });
    const applied = result.fields.filter((field) => field.status === "applied").length;
    const restart = result.fields.filter((field) => field.status === "restart-required").length;
    status.textContent = `Saved. ${applied} applied; ${restart} restart-required.`;
  } catch (error) {
    status.textContent = `Save failed: ${error}`;
  }
});

try {
  documentModel = await invoke("get_settings");
  status.textContent = "Settings loaded.";
  render();
} catch (error) {
  status.textContent = `Unable to load settings: ${error}`;
}
