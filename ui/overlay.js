const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const root = document.querySelector("#overlay");
const message = document.querySelector("#message");
const waveform = document.querySelector("#waveform");
const language = document.querySelector("#language");
const pause = document.querySelector("#pause");
const cancel = document.querySelector("#cancel");

// Keep the elements across polls so their heights can transition.
const bars = [];

function render(view) {
  root.hidden = !view.visible;
  root.dataset.theme = view.theme ?? "system";
  root.classList.toggle("expanded", view.style === "expanded");
  root.classList.toggle("bottom", view.position === "bottom");
  message.classList.toggle("visually-hidden", view.show_waveform);
  if (message.textContent !== view.message) message.textContent = view.message;
  cancel.hidden = !view.show_cancel;
  const paused = view.mode === "paused";
  pause.hidden = !(paused || view.mode === "recording");
  pause.textContent = paused ? "▶" : "Ⅱ";
  const pauseLabel = paused ? "Resume recording" : "Pause recording";
  pause.setAttribute("aria-label", pauseLabel);
  pause.title = pauseLabel;
  language.hidden = !view.show_language_control;
  language.textContent = view.language_label ?? "AUTO";
  waveform.hidden = !view.show_waveform;
  if (!view.show_waveform) return;
  if (!bars.length) {
    bars.push(...view.waveform.map(() => document.createElement("i")));
    waveform.append(...bars);
  }
  bars.forEach((bar, index) => {
    const level = view.waveform[index];
    bar.style.height = `${2 + Math.round(level * 0.18)}px`;
  });
}

pause.addEventListener("click", async () => {
  pause.disabled = true;
  try {
    const response = await invoke("pause_resume_recording");
    if (response.startsWith("error:")) throw new Error(response);
  } catch (error) {
    waveform.hidden = true;
    message.classList.remove("visually-hidden");
    message.textContent = "Could not pause/resume";
    pause.title = String(error);
  } finally {
    pause.disabled = false;
  }
});
cancel.addEventListener("click", () => invoke("cancel_transcription"));
language.addEventListener("click", () => invoke("cycle_transcription_language"));
await listen("overlay-state", (event) => render(event.payload));
