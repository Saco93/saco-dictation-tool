# Development Guide - sttctl (Exhaustive)

## Build

```bash
cargo build -p sttctl
```

## Run Commands

```bash
cargo run -p sttctl -- status
cargo run -p sttctl -- ptt-press
cargo run -p sttctl -- ptt-release
cargo run -p sttctl -- toggle-continuous
cargo run -p sttctl -- replay-last-transcript
cargo run -p sttctl -- shutdown
```

## Compatibility Notes

- Keep protocol version and command mapping aligned with `common::protocol` and daemon server behavior.
- Validate against a running `sttd` instance after IPC-related changes.
