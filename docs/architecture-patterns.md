# Architecture Patterns

## Repository Pattern

- **Monorepo Cargo Workspace**
  - Three parts with clear responsibilities: daemon, CLI, shared contracts.
  - Shared dependency/lint policy enforced at workspace root.

## Runtime Architectural Style

### sttd

- **Service-Centric Daemon**
  - Long-running process exposed via local IPC boundary.
- **Adapter Pattern for Providers**
  - Distinct adapters for `openrouter`, `whisper_local`, `whisper_server`.
- **Pipeline-style Processing**
  - Input capture -> VAD/state coordination -> provider transcription -> output injection.
- **Stateful Orchestration**
  - Explicit runtime state model with mode transitions and recovery behavior.

### sttctl

- **Thin Command Controller**
  - Parses user intent and delegates all stateful work to daemon.
- **Contract-Driven Integration**
  - Reuses shared protocol to avoid command drift.

### common

- **Shared Kernel / Contract Library**
  - Central source of truth for config and protocol contracts.
  - Prevents daemon/client schema divergence.

## Deployment/Operations Pattern

- **User-level systemd services** as primary deployment mode.
- Optional split-service local inference (`whisper-server.service`) for persistent model serving.

## Architecture Summary

The system emphasizes local control, explicit contracts, and operational reliability through strict startup validation and service hardening defaults.
