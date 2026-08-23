# GNOME Wayland V1

OpenVoiceDesktop V1 uses a resident daemon to keep the microphone and ASR path low latency. GNOME runs `openvoice-desktop toggle` from a custom shortcut; that command sends a Unix-domain-socket message to the daemon.

## Required Helpers

```bash
sudo apt install wl-clipboard wtype libsecret-tools
```

- `wl-copy` and `wl-paste` handle clipboard save/set/restore.
- `wtype` sends `Ctrl+V` on Wayland.
- `dotool` and `ydotool` are supported Wayland fallback typing tools when installed.
- `xdotool` is the X11 fallback.
- `secret-tool` reads the token from Secret Service.

Paste fallback order is `wtype`, `dotool`, `ydotool`, then `xdotool`.
If none can paste into the focused app, OpenVoiceDesktop leaves the transcript in the clipboard and notifies the user.

## Shortcut

The daemon writes the GNOME custom keybinding itself from `behavior.shortcut`, at
startup and after every accepted save, so changing the shortcut in Settings is enough.
It appears under GNOME Settings > Keyboard > View and Customize Shortcuts > Custom
Shortcuts as `OpenVoiceDesktop Toggle` running `openvoice-desktop toggle`.

The first press starts recording. The second press stops, transcribes, and pastes.

A GNOME custom keybinding runs a command; it does not message an already-running app.
A stale entry left behind by another dictation tool will therefore start that tool on
every press. Check who owns a combination with:

```bash
dconf dump /org/gnome/settings-daemon/plugins/media-keys/
```
