export const numericSpecs = Object.freeze({
  "asr.timeout_seconds": { min: 1, max: 300, step: 1 },
  "audio.audio_feedback_volume": { min: 0, max: 1, step: 0.01 },
  "audio.recording_output_volume": { min: 0, max: 1, step: 0.01 },
  "output.paste_delay_ms": { min: 0, max: 2000, step: 1 },
  "output.restore_delay_ms": { min: 0, max: 2000, step: 1 },
  "history.limit": { min: 1, max: 10000, step: 1 },
  "debug.recording_buffer_ms": { min: 0, max: 1500, step: 1 },
});

export function normalizeFieldValue(path, previous, raw, checked = false) {
  if (typeof previous === "boolean") return checked;
  if (typeof previous === "number") {
    const spec = numericSpecs[path];
    const value = raw.trim() === "" ? NaN : Number(raw);
    if (!Number.isFinite(value) || (spec && (
      value < spec.min || value > spec.max || (spec.step === 1 && !Number.isInteger(value))
    ))) {
      throw new Error(`${path} must be a number${spec ? ` from ${spec.min} to ${spec.max}${spec.step === 1 ? " with no fraction" : ""}` : ""}.`);
    }
    return value;
  }
  if (Array.isArray(previous)) return raw.split(",").map((part) => part.trim()).filter(Boolean);
  if (path === "asr.prompt") return raw.trim() === "" ? null : raw;
  if (path === "asr.language" || (previous === null && raw === "")) return raw.trim() || null;
  return raw;
}

export function deviceFallback(selected, devices, kind) {
  if (!selected || devices.some((device) => device.name === selected)) return null;
  const systemDefault = devices.find((device) => device.is_default)?.name;
  return `${kind} ${JSON.stringify(selected)} is unavailable; using ${systemDefault ? JSON.stringify(systemDefault) : "system default when available"}.`;
}

export function summarizeSave(fields, labels = {}) {
  if (fields.length === 0) {
    return { short: "No changes", detail: "No changes to save.", exceptions: [] };
  }
  const counts = { applied: 0, pending: 0, "restart-required": 0, unavailable: 0 };
  for (const field of fields) counts[field.status] = (counts[field.status] ?? 0) + 1;
  const exceptions = fields.filter((field) => field.reason || field.status !== "applied");
  if (fields.length === 1) {
    const field = fields[0];
    const label = labels[field.field] ?? field.field;
    const state = field.status === "applied" ? "saved" : field.status.replaceAll("-", " ");
    return { short: `Saved ${label}`, detail: `${label}: ${state}.`, exceptions };
  }
  const parts = [
    counts.applied && `${counts.applied} applied`,
    counts.pending && `${counts.pending} pending`,
    counts["restart-required"] && `${counts["restart-required"]} need restart`,
    counts.unavailable && `${counts.unavailable} unavailable`,
  ].filter(Boolean);
  return { short: `Saved ${fields.length} settings`, detail: `Saved: ${parts.join(", ")}.`, exceptions };
}
