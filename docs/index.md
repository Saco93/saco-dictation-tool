# master Documentation Index

**Type:** monorepo with 3 parts
**Primary Language:** Rust
**Architecture:** Daemon + CLI + Shared Contract
**Last Updated:** 2026-03-05 (exhaustive rescan)

## Project Overview

This documentation set was regenerated via exhaustive scan and is intended as the primary retrieval context for AI-assisted maintenance and feature work.

## Project Structure

### sttd (backend)

- Root: `crates/sttd`
- Entry: `crates/sttd/src/main.rs`
- Role: daemon runtime, provider orchestration, IPC server, output injection

### sttctl (cli)

- Root: `crates/sttctl`
- Entry: `crates/sttctl/src/main.rs`
- Role: command-line control plane for daemon

### common (library)

- Root: `crates/common`
- Entry: `crates/common/src/lib.rs`
- Role: shared config/protocol contract authority

## Quick Reference

- **Control plane:** Unix socket IPC with protocol envelopes (`common::protocol`)
- **Provider modes:** openrouter / whisper_local / whisper_server
- **Recovery paths:** audio input unavailable handling + retained transcript replay
- **Deployment style:** systemd user services

## Generated Documentation

### Core

- [Project Overview](./project-overview.md)
- [Source Tree Analysis](./source-tree-analysis.md)
- [Technology Stack](./technology-stack.md)
- [Architecture Patterns](./architecture-patterns.md)

### sttd

- [Architecture - sttd](./architecture-sttd.md)
- [API Contracts - sttd](./api-contracts-sttd.md)
- [Data Models - sttd](./data-models-sttd.md)
- [Component Inventory - sttd](./component-inventory-sttd.md)
- [Development Guide - sttd](./development-guide-sttd.md)
- [Comprehensive Analysis - sttd](./comprehensive-analysis-sttd.md)

### sttctl

- [Architecture - sttctl](./architecture-sttctl.md)
- [Component Inventory - sttctl](./component-inventory-sttctl.md)
- [Development Guide - sttctl](./development-guide-sttctl.md)
- [Comprehensive Analysis - sttctl](./comprehensive-analysis-sttctl.md)

### common

- [Architecture - common](./architecture-common.md)
- [Component Inventory - common](./component-inventory-common.md)
- [Development Guide - common](./development-guide-common.md)
- [Comprehensive Analysis - common](./comprehensive-analysis-common.md)

### Cross-Part and Operations

- [Integration Architecture](./integration-architecture.md)
- [Deployment Guide](./deployment-guide.md)
- [Contribution Guide](./contribution-guide.md)
- [Critical Folders Summary](./critical-folders-summary.md)
- [Project Parts Metadata](./project-parts.json)
- [Workflow State](./project-scan-report.json)
- [Validation Report](./documentation-validation-report.md)

## Existing Repository Documentation

- [README](../README.md)
- [OpenRouter Contract](./openrouter-contract.md)
- [Hyprland Guide](./hyprland.md)
- [Change Ledger](./CHANGE_LEDGER.md)
- [AC Traceability](./AC_TRACEABILITY.md)
- [Release Go/No-Go Checklist](./release-go-no-go-checklist.md)
- [Verification AC13](./verification/ac13-systemd-user-service-2026-03-04.md)
- [Verification AC1/AC6/AC10/AC12](./verification/ac1-ac6-ac10-ac12-closure-2026-03-04.md)

## Getting Started for AI-assisted Work

1. Start with [Project Overview](./project-overview.md).
2. Read [Integration Architecture](./integration-architecture.md).
3. For daemon/runtime work, use `architecture-sttd.md` + `api-contracts-sttd.md` + `data-models-sttd.md`.
4. For CLI work, use `architecture-sttctl.md`.
5. For cross-part contract changes, include `architecture-common.md`.
