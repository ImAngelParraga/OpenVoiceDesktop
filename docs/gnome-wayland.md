# GNOME Wayland V1

OpenVoiceDesktop V1 uses a resident daemon to keep the microphone and ASR path low latency. GNOME runs `openvoice-desktop toggle` from a custom shortcut; that command sends a Unix-domain-socket message to the daemon.

## Required Helpers

```bash
sudo apt install wl-clipboard wtype libsecret-tools
```

- `wl-copy` and `wl-paste` handle clipboard save/set/restore.
- `wtype` sends `Ctrl+V` on Wayland.
- `secret-tool` reads the token from Secret Service.

If `wtype` cannot paste into the focused app, OpenVoiceDesktop leaves the transcript in the clipboard and notifies the user.

## Shortcut

Configure GNOME Settings > Keyboard > View and Customize Shortcuts > Custom Shortcuts:

- Name: `OpenVoiceDesktop Toggle`
- Command: `openvoice-desktop toggle`
- Shortcut: `Ctrl+Space`

The first press starts recording. The second press stops, transcribes, and pastes.
