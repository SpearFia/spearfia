# 2. Stack: Tauri, Rust and React

- Status: superseded by [ADR 0004](0004-native-ui-egui.md)
- Date: 2026-10-18

## Context

Spear Fia is a desktop app that works with large local media, drives FilmCraft (itself
written in Rust), and exposes its functionality to AI agents over MCP.

## Decision

- **Core logic in Rust**: `spearfia-*` crates in one Cargo workspace under `crates/`.
- **Desktop shell in Tauri 2** (`apps/desktop`) with a **React + TypeScript** UI built by Vite.
- **CLI** (`apps/cli`) on the same crates as the desktop app.
- **pnpm workspaces** for JavaScript packages, published as `@spearfia/*`.
- **Formats** are JSON Schemas in `spearfia/spearfia-spec`, pinned in `schemas/`.

## Consequences

Contributors need Rust (stable, see `rust-toolchain.toml`), Node 20+ and pnpm. On Windows,
Rust builds need the MSVC Build Tools. The UI never touches media directly; it calls
Tauri commands backed by `spearfia-core`'s command layer.
