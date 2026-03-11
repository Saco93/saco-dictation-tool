# master - Project Overview

**Date:** 2026-03-11  
**Type:** monorepo (3 parts)  
**Architecture:** Daemon + CLI + Shared Contract

## Executive Summary

`saco-dictation-tool` is a Rust workspace delivering a local-first dictation system. `sttd` runs as a daemon handling audio capture, bounded playback coordination, transcription orchestration, and transcript output delivery. `sttctl` provides command-line control, and `common` centralizes configuration and protocol contracts.

## Architecture Highlights

- local IPC control plane with versioned envelope contracts
- canonical hosted provider strategy via `openai_compatible`
- legacy hosted compatibility alias via `openrouter`
- local fallback paths via `whisper_local` and `whisper_server`
- final-only runtime injection semantics
- guardrail-rich runtime state machine

## Operational Highlights

- systemd user-service deployment contract in `config/*.service`
- startup capability validation for providers
- bilingual hosted benchmarking procedure documented for DashScope `qwen3-asr-flash`
- integration tests enforce runtime, provider, and documentation contracts
