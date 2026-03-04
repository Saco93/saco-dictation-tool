# Deployment Configuration (Exhaustive)

## Primary Deployment Model

- Linux user-session services via systemd user units.

## sttd Service Contract (`config/sttd.service`)

Key directives:

- `EnvironmentFile=%h/.config/sttd/sttd.env`
- `ExecStart=/usr/bin/env sttd --config %h/.config/sttd/sttd.toml`
- `Restart=on-failure`, `RestartSec=2`
- Hardening: `NoNewPrivileges=true`, `ProtectSystem=strict`, `ProtectHome=read-only`, `PrivateTmp=true`
- Writable paths scoped to `%h/.config/sttd %h/.cache/sttd %t/sttd`

## whisper-server Service Contract (`config/whisper-server.service`)

- Optional persistent inference companion
- Reads `STTD_WHISPER_MODEL_PATH` from env file
- Binds `127.0.0.1:8080`
- Restart on failure

## Deployment Commands

```bash
cp config/sttd.service ~/.config/systemd/user/sttd.service
cp config/whisper-server.service ~/.config/systemd/user/whisper-server.service
systemctl --user daemon-reload
systemctl --user enable --now sttd.service
systemctl --user status sttd.service
```

Optional persistent inference:

```bash
systemctl --user enable --now whisper-server.service
systemctl --user status whisper-server.service
```

## CI/CD and Infra Scan Result

No `.github/workflows`, `.gitlab-ci.yml`, `Dockerfile`, `k8s`, `terraform` assets were detected in current repository root.
