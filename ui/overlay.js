const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const root = document.querySelector("#overlay");
const message = document.querySelector("#message");
const waveform = document.querySelector("#waveform");
const language = document.querySelector("#language");
const pause = document.querySelector("#pause");
const cancel = document.querySelector("#cancel");

// Rebuilding the bars on every poll threw away the CSS transition with the
// element that carried it, which is what made the waveform jump instead of
// move. They are created once and only their height changes.
const BAR_COUNT = 4;
const bars = Array.from({ length: BAR_COUNT }, () => document.createElement("i"));
waveform.append(...bars);

function render(view) {
  root.hidden = !view.visible;
  root.dataset.theme = view.theme ?? "system";
  root.classList.toggle("expanded", view.style === "expanded");
  root.classList.toggle("bottom", view.position === "bottom");
  message.textContent = view.message;
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
  const level = view.level_bucket ?? 0;
  bars.forEach((bar, index) => {
    bar.style.height = `${Math.max(3, 4 + ((index + level) % BAR_COUNT) * 3)}px`;
  });
}

pause.addEventListener("click", async () => {
  pause.disabled = true;
  try {
    const response = await invoke("pause_resume_recording");
    if (response.startsWith("error:")) throw new Error(response);
  } catch (error) {
    message.textContent = "Could not pause/resume";
    pause.title = String(error);
  } finally {
    pause.disabled = false;
  }
});
cancel.addEventListener("click", () => invoke("cancel_transcription"));
language.addEventListener("click", () => invoke("cycle_transcription_language"));
await listen("overlay-state", (event) => render(event.payload));
