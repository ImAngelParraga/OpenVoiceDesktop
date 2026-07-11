const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const root = document.querySelector("#overlay");
const message = document.querySelector("#message");
const waveform = document.querySelector("#waveform");
const cancel = document.querySelector("#cancel");

function render(view) {
  root.hidden = !view.visible;
  root.dataset.theme = view.theme ?? "system";
  root.classList.toggle("expanded", view.style === "expanded");
  root.classList.toggle("bottom", view.position === "bottom");
  message.textContent = view.message;
  cancel.hidden = !view.show_cancel;
  waveform.replaceChildren();
  if (!view.show_waveform) return;
  const level = view.level_bucket ?? 0;
  for (let index = 0; index < 10; index += 1) {
    const bar = document.createElement("i");
    bar.style.height = `${Math.max(3, 5 + ((index + level) % 5) * 4)}px`;
    waveform.append(bar);
  }
}

cancel.addEventListener("click", () => invoke("cancel_transcription"));
await listen("overlay-state", (event) => render(event.payload));
