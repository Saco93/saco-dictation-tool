# saco-dictation-tool

Rust workspace for a Hyprland-native speech-to-text daemon (`sttd`) and CLI control tool (`sttctl`) using local Whisper (`whisper.cpp`) by default, with OpenRouter-compatible STT as an optional provider.

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

3. Install local whisper runtime and model (example on Arch Linux):

```bash
yay -S whisper.cpp whisper.cpp-model-small.en-q5_1
```

4. Verify `STTD_WHISPER_MODEL_PATH` in `~/.config/sttd/sttd.env` points to your installed model.

5. Choose provider mode:

- `STTD_PROVIDER_KIND=whisper_local` for direct `whisper-cli` invocation
- `STTD_PROVIDER_KIND=whisper_server` for persistent local inference (lower per-request overhead)
- `STTD_PROVIDER_KIND=openrouter` plus `STTD_OPENROUTER_API_KEY` for remote OpenRouter STT

Startup now performs strict provider capability validation before capture starts:
- `openrouter`: model ID must be speech/audio-capable; optional catalog probe runs when `capability_probe=true`.
- `whisper_local`: `.en`-only model files require an English `language` configuration.
- `whisper_server`: when `capability_probe=true`, daemon probes `/inference` readiness and rejects unsupported language contracts at startup.

6. Run daemon:

```bash
cargo run -p sttd -- --config ~/.config/sttd/sttd.toml
```

7. Send commands:

```bash
cargo run -p sttctl -- status
cargo run -p sttctl -- ptt-press
cargo run -p sttctl -- ptt-release
cargo run -p sttctl -- toggle-continuous
cargo run -p sttctl -- replay-last-transcript
```

`replay-last-transcript` retries inserting the most recently retained transcript when output backends recover.
Use `sttctl status` to inspect `has_retained_transcript`, `last_output_error_code`, and `last_audio_error_code` before replay.
When `last_audio_error_code=ERR_AUDIO_INPUT_UNAVAILABLE`, daemon is still running but capture input is currently unavailable; fix microphone/backend availability and retry dictation.

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

For persistent local inference, also install `config/whisper-server.service` and run:

```bash
cp config/whisper-server.service ~/.config/systemd/user/whisper-server.service
systemctl --user daemon-reload
systemctl --user enable --now whisper-server.service
systemctl --user status whisper-server.service
```

## Contract and operations docs

- [OpenRouter adapter contract](docs/openrouter-contract.md)
- [Hyprland integration guide](docs/hyprland.md)
- [Provider-mode change ledger](docs/CHANGE_LEDGER.md)
- [Acceptance criteria traceability](docs/AC_TRACEABILITY.md)
