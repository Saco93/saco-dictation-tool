# saco-dictation-tool

Rust workspace for a Hyprland-native speech-to-text daemon (`sttd`) and CLI control tool (`sttctl`) using OpenRouter-compatible STT models.

## Crates

- `common`: shared config + IPC protocol contracts
- `sttd`: daemon runtime (IPC server, state machine, provider adapter, output backends)
- `sttctl`: command-line control client

## Quick start

1. Sync dependencies:

```bash
uv sync --all-extras
```

2. Copy config templates:

```bash
mkdir -p ~/.config/sttd
cp config/sttd.example.toml ~/.config/sttd/sttd.toml
cp config/sttd.env.example ~/.config/sttd/sttd.env
```

3. Set `STTD_OPENROUTER_API_KEY` in `~/.config/sttd/sttd.env`.

4. Run daemon:

```bash
cargo run -p sttd -- --config ~/.config/sttd/sttd.toml
```

5. Send commands:

```bash
cargo run -p sttctl -- status
cargo run -p sttctl -- ptt-press
cargo run -p sttctl -- ptt-release
cargo run -p sttctl -- toggle-continuous
```

## Privacy defaults

- Transcript persistence disabled by default.
- Transcript text should be redacted in normal logs.
- Debug WAV capture is disabled by default and, when enabled, is bounded by TTL + size cap.

## systemd user service

Install `config/sttd.service` into `~/.config/systemd/user/sttd.service`, then:

```bash
systemctl --user daemon-reload
systemctl --user enable --now sttd.service
systemctl --user status sttd.service
```

## Contract and operations docs

- [OpenRouter adapter contract](docs/openrouter-contract.md)
- [Hyprland integration guide](docs/hyprland.md)
