# Paste delays

`output.paste_delay_ms` is optional settling time before clipboard-paste. Default `0 ms`. Increase only when a specific target app fails without it. It does not apply to direct type, copy only, or external scripts.

`output.restore_delay_ms` runs after simulated paste input and before restoring previous clipboard. Default `250 ms`. Some applications process paste asynchronously; restoring immediately can make them read old clipboard instead of transcript. It applies only to `clipboard-paste` with `restore-after-paste`.

Both values accept `0..=2000` milliseconds. Existing valid configured values remain unchanged. Paste diagnostics log:

- ASR response to clipboard write.
- Clipboard write to paste input.
- Paste input to clipboard restoration.

Visible inserted feedback occurs after paste input, before clipboard restoration cleanup.
