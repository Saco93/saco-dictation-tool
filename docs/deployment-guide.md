# Deployment Guide (Exhaustive)

## User Service Install

```bash
cp config/sttd.service ~/.config/systemd/user/sttd.service
systemctl --user daemon-reload
systemctl --user enable --now sttd.service
systemctl --user status sttd.service
```

## Optional Persistent Inference

```bash
cp config/whisper-server.service ~/.config/systemd/user/whisper-server.service
systemctl --user daemon-reload
systemctl --user enable --now whisper-server.service
systemctl --user status whisper-server.service
```

## Config Files Expected

- `~/.config/sttd/sttd.toml`
- `~/.config/sttd/sttd.env`

## Runtime Paths

- socket: `${XDG_RUNTIME_DIR}/sttd/sttd.sock`
- debug wav: `${XDG_CACHE_HOME:-~/.cache}/sttd/debug-wav` (if enabled)

## Operational Checks

- `sttctl status` returns protocol version and state payload
- service restart behavior is `on-failure`
