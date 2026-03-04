# Comprehensive Analysis - common (Exhaustive)

## Part Classification

- Part: `common`
- Type: shared library
- Root: `crates/common`

## Exhaustive Source Coverage

- `crates/common/src/config.rs`
- `crates/common/src/protocol.rs`
- `crates/common/src/lib.rs`

## Responsibilities

### Configuration Contract Layer

- Defines full runtime config schema with defaults.
- Implements env-file + runtime-env overlay and validation logic.
- Provides path template expansion helpers for XDG/HOME paths.

### Protocol Contract Layer

- Defines versioned request/response envelopes for IPC.
- Defines command/status/error payload model.
- Contains protocol compatibility helper (`is_compatible_version`).

### Compatibility Guarantees

- Status payload uses serde defaults on newer optional fields for backward compatibility.
- Single protocol version constant shared across daemon and CLI.

## Conclusion

`common` is the schema authority for both configuration and IPC protocol and is the most coupling-sensitive crate in this workspace.
