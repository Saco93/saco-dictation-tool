# Comprehensive Analysis - sttctl (Exhaustive)

## Part Classification

- Part: `sttctl`
- Type: CLI application
- Root: `crates/sttctl`

## Exhaustive Source Coverage

- `crates/sttctl/src/main.rs`

## Command Surface

Supported subcommands map one-to-one to protocol commands:

- `ptt-press`
- `ptt-release`
- `toggle-continuous`
- `replay-last-transcript`
- `status`
- `shutdown`

## Runtime Behavior

- Resolves socket path from explicit `--socket-path` or config-derived `Config::load_for_control_client`.
- Sends `RequestEnvelope` over Unix socket.
- Renders `Ack` and `Status` responses to stdout.
- Converts `ResponseKind::Err` into CLI failure with code/message/retryable context.

## Integration Dependencies

- Protocol/data contract from `common`
- IPC client function from `sttd::ipc::send_request`

## Conclusion

`sttctl` is a thin and deterministic control client; correctness depends on protocol stability and daemon compatibility.
