# master Documentation Index

**Type:** monorepo with 3 parts  
**Primary Language:** Rust  
**Architecture:** Daemon + CLI + Shared Contract  
**Last Updated:** 2026-03-11

## Project Overview

This documentation set has been refreshed for the hosted-provider generalization that introduces canonical `openai_compatible` support, keeps `openrouter` as a compatibility alias, and documents DashScope `qwen3-asr-flash` as the primary hosted example.

## Quick Reference

- **Control plane:** Unix socket IPC with protocol envelopes (`common::protocol`)
- **Provider modes:** `openai_compatible` / legacy `openrouter` / `whisper_local` / `whisper_server`
- **Hosted request modes:** `auto` or `chat_completions`
- **Playback control:** best-effort `playerctl`/MPRIS pause-resume around recording sessions
- **Recovery paths:** audio input unavailable handling + retained transcript replay
- **Benchmark artifact:** `_bmad-output/implementation-artifacts/qwen3-asr-flash-benchmark-2026-03-11.md`

## Generated Documentation

### Core

- [Project Overview](./project-overview.md)
- [Source Tree Analysis](./source-tree-analysis.md)
- [Technology Stack](./technology-stack.md)
- [Architecture Patterns](./architecture-patterns.md)
- [Project Structure](./project-structure.md)

### sttd

- [Architecture - sttd](./architecture-sttd.md)
- [API Contracts - sttd](./api-contracts-sttd.md)
- [Data Models - sttd](./data-models-sttd.md)
- [Component Inventory - sttd](./component-inventory-sttd.md)
- [Development Guide - sttd](./development-guide-sttd.md)
- [Comprehensive Analysis - sttd](./comprehensive-analysis-sttd.md)

### Operations

- [Integration Architecture](./integration-architecture.md)
- [Deployment Guide](./deployment-guide.md)
- [Contribution Guide](./contribution-guide.md)

## Getting Started for AI-assisted Work

1. Start with [Project Overview](./project-overview.md).
2. For daemon/runtime work, read [Architecture - sttd](./architecture-sttd.md) and [API Contracts - sttd](./api-contracts-sttd.md).
3. For config or protocol changes, include `crates/common` and [Data Models - sttd](./data-models-sttd.md).
4. For hosted-provider work, check the benchmark artifact and README examples together.
