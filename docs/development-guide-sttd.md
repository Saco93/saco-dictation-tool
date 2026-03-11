# Development Guide - sttd

## Setup

```bash
uv sync --all-extras
mkdir -p ~/.config/sttd
cp config/sttd.example.toml ~/.config/sttd/sttd.toml
cp config/sttd.env.example ~/.config/sttd/sttd.env
```

## Hosted Qwen Path

Recommended Phase 1 hosted setup:

- `provider.kind = "openai_compatible"`
- `base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"`
- `model = "qwen3-asr-flash"`
- `request_mode = "chat_completions"`
- `capability_probe = false`
- `STTD_PROVIDER_API_KEY` in `sttd.env`

For bilingual English/Mandarin benchmarking, prefer `provider.language_hints = ["zh", "en"]`.

## Historical Local Whisper Baseline

To reproduce the historical pre-change baseline for comparison:

1. switch to `provider.kind = "whisper_local"`
2. comment out hosted-only `language_hints` and `request_mode`
3. set `provider.language = "en"`
4. keep an English-only `.en` local model

## Validation

Recommended commands after provider/config changes:

```bash
cargo test -p common --lib
cargo test -p sttd --lib --test mode_transitions
cargo test -p sttd --test provider_contract
cargo test -p sttd --test ipc_flow
cargo test -p sttd --test device_recovery
cargo build --release -p sttd
```

## Benchmark Artifact

Record manual benchmark output in:

- `_bmad-output/implementation-artifacts/qwen3-asr-flash-benchmark-2026-03-11.md`
