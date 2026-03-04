# Contribution Guidelines (Exhaustive-Derived)

No formal `CONTRIBUTING.md` exists; effective contribution contract is inferred from repository rules and tests.

## Mandatory Local Rules

- After any code change, build daemon release binary:
  - `cargo build --release -p sttd`
- Use Podman instead of Docker on this machine.
- Prefer dependency sync:
  - `uv sync --all-extras`

## Contract Safety Expectations

- Keep `common` protocol/config schema changes compatible with both `sttd` and `sttctl`.
- Maintain protocol version behavior (`PROTOCOL_VERSION`) and backward-compatible fields where intended.
- Preserve operational error-code semantics used by CLI and docs.

## Verification Expectations

- Run relevant integration tests under `crates/sttd/tests` for behavior-impacting changes.
- Keep release readiness docs aligned (validated by `release_readiness_docs.rs`).
- Update docs when command surface, provider behavior, or deployment contracts change.
