# Development Guide - common (Exhaustive)

## Build/Test

```bash
cargo build -p common
cargo test -p common
```

## Safe Schema Change Flow

1. Update `config.rs` or `protocol.rs`.
2. Rebuild all workspace parts.
3. Run daemon integration tests (`cargo test -p sttd`).
4. Update documentation if envelopes/config contracts changed.
