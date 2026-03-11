# Architecture Patterns

## Repository Pattern

- monorepo Cargo workspace with three clear parts: daemon, CLI, shared contracts

## Runtime Patterns

- service-centric daemon exposed through a local IPC boundary
- adapter pattern for providers:
  - generic hosted `openai_compatible`
  - compatibility shim `openrouter`
  - local `whisper_local`
  - HTTP `whisper_server`
- gated recording lifecycle around playback pause/resume
- final-only utterance processing pipeline
- stateful orchestration with explicit recovery semantics

## Contract Patterns

- shared config and protocol authority in `crates/common`
- additive compatibility for legacy hosted aliases
- exact request-shape tests for provider contracts
