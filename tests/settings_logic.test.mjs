import assert from "node:assert/strict";
import test from "node:test";
import { asrModelChoices, CUSTOM_ASR_MODEL, deviceFallback, normalizeFieldValue, numericSpecs, selectedAsrModelChoice, summarizeSave } from "../ui/settings_logic.mjs";

test("ASR model picker lists deployed engines and preserves custom IDs", () => {
  assert.deepEqual(asrModelChoices.map((choice) => choice.value), ["whisper-1", "qwen3-asr-1.7b"]);
  assert.equal(selectedAsrModelChoice("whisper-1"), "whisper-1");
  assert.equal(selectedAsrModelChoice("qwen3-asr-1.7b"), "qwen3-asr-1.7b");
  assert.equal(selectedAsrModelChoice("my-custom-model"), CUSTOM_ASR_MODEL);
  assert.equal(normalizeFieldValue("asr.model", "whisper-1", "my-custom-model"), "my-custom-model");
});

test("Save summary counts changed fields only and handles no-change Save", () => {
  assert.equal(summarizeSave([]).short, "No changes");
  assert.equal(summarizeSave([{ field: "ui.theme", status: "applied" }], { "ui.theme": "Theme" }).short, "Saved Theme");
  const mixed = summarizeSave([
    { field: "ui.theme", status: "applied" },
    { field: "asr.model", status: "pending", reason: "Applies after dictation" },
  ]);
  assert.equal(mixed.detail, "Saved: 1 applied, 1 pending.");
  assert.equal(mixed.exceptions.length, 1);
});

test("numeric fields reject blank, fractions for integers, and out-of-range values", () => {
  assert.equal(numericSpecs["history.limit"].step, 1);
  assert.throws(() => normalizeFieldValue("history.limit", 100, ""));
  assert.throws(() => normalizeFieldValue("history.limit", 100, "1.5"));
  assert.throws(() => normalizeFieldValue("audio.recording_output_volume", 0.3, "1.2"));
  assert.equal(normalizeFieldValue("audio.recording_output_volume", 0.3, "0.25"), 0.25);
});

test("optional language clears to null after prior value", () => {
  assert.equal(normalizeFieldValue("asr.language", "en", ""), null);
  assert.equal(normalizeFieldValue("asr.language", null, "es"), "es");
  assert.equal(normalizeFieldValue("asr.prompt", "old", "  "), null);
});

test("missing saved audio device stays visible with fallback warning", () => {
  const devices = [{ name: "Speakers", is_default: true }];
  assert.match(deviceFallback("Old headset", devices, "Output device"), /Old headset.*Speakers/);
  assert.equal(deviceFallback("Speakers", devices, "Output device"), null);
});
