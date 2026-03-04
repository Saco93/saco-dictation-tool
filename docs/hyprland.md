# Hyprland Integration Guide

## Runtime requirements

Install tools used by output backends:
- `wtype` (primary typed insertion)
- `wl-clipboard` (`wl-copy` fallback)

Ensure microphone capture is available for your PipeWire/ALSA setup.

## Canonical paths

- Config: `${XDG_CONFIG_HOME:-~/.config}/sttd/sttd.toml`
- Env file: `${XDG_CONFIG_HOME:-~/.config}/sttd/sttd.env`
- Socket: `${XDG_RUNTIME_DIR}/sttd/sttd.sock`

## Example Hyprland keybinds

Push-to-talk on press/release:

```ini
bind = ,F9,exec,sttctl ptt-press
bindr = ,F9,exec,sttctl ptt-release
```

Toggle continuous mode:

```ini
bind = SUPER,F9,exec,sttctl toggle-continuous
```

Status and shutdown can be bound similarly:

```ini
bind = SUPER,F10,exec,sttctl status
bind = SUPER,SHIFT,F10,exec,sttctl shutdown
bind = SUPER,F11,exec,sttctl replay-last-transcript
```

Use `sttctl replay-last-transcript` to retry output insertion for a transcript retained after backend failure.

## systemd user service operations

```bash
mkdir -p ~/.config/sttd
cp config/sttd.example.toml ~/.config/sttd/sttd.toml
cp config/sttd.env.example ~/.config/sttd/sttd.env
systemctl --user daemon-reload
systemctl --user enable --now sttd.service
systemctl --user status sttd.service
journalctl --user -u sttd.service -f
```

If `STTD_PROVIDER_KIND=whisper_server`, also run:

```bash
cp config/whisper-server.service ~/.config/systemd/user/whisper-server.service
systemctl --user daemon-reload
systemctl --user enable --now whisper-server.service
systemctl --user status whisper-server.service
journalctl --user -u whisper-server.service -f
```

## Troubleshooting

- `ERR_PROTOCOL_VERSION`: client and daemon protocol versions differ.
- `ERR_OUTPUT_BACKEND_UNAVAILABLE`: install `wtype` or `wl-copy`, or change output mode.
- retained transcript replay: run `sttctl status` and check `has_retained_transcript=true`; if `last_output_error_code=ERR_OUTPUT_BACKEND_UNAVAILABLE`, restore output tooling then run `sttctl replay-last-transcript`.
- input device unavailable: `last_audio_error_code=ERR_AUDIO_INPUT_UNAVAILABLE` means daemon stayed alive but capture device/backend was unavailable; restore PipeWire/ALSA device availability or fix `STTD_INPUT_DEVICE`, then retry capture.
- provider mode mismatch: verify `STTD_PROVIDER_KIND` (env) and `[provider].kind` (toml) are aligned.
- openrouter auth/provider failures: verify `STTD_OPENROUTER_API_KEY` when `STTD_PROVIDER_KIND=openrouter`.
- whisper_local failures: verify `STTD_WHISPER_CMD` is installed and `STTD_WHISPER_MODEL_PATH` exists.
- startup capability validation failure: startup now fails fast when provider model/language contract is incompatible; check configured `language`, model choice/path, and provider-specific startup probe logs.
- whisper_server failures: verify `STTD_PROVIDER_BASE_URL`, `/inference` readiness, and `whisper-server.service` status/logs.
- socket not reachable: check `${XDG_RUNTIME_DIR}` and service logs.
