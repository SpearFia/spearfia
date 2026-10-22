# 4. Native UI in Rust with egui

- Status: accepted
- Date: 2026-10-18
- Supersedes: [ADR 0002](0002-stack-tauri-rust-react.md)

## Context

ADR 0002 chose Tauri with a React + TypeScript UI. Spear Fia should be a native desktop
app written entirely in Rust, like FilmCraft, not a web page in a window.

## Decision

- **UI in Rust with egui/eframe**, version-matched to FilmCraft (0.36, wgpu renderer).
  `apps/desktop` is the `spearfia-desktop` binary.
- **No JavaScript toolchain** in this repo: no Tauri, React, Vite, pnpm or Node.
- Everything else from ADR 0002 stands: `spearfia-*` crates under `crates/`, the CLI in
  `apps/cli`, formats from `spearfia-spec` pinned in `schemas/`.

## Consequences

- One language and one build (`cargo`). Contributors need only Rust (plus MSVC Build
  Tools on Windows).
- Sharing UI code or patterns with FilmCraft's `ui-egui` crate is possible later.
- egui is immediate-mode: rich text editing for the script view will need care (custom
  widgets or a text-editing crate). Revisit if it blocks the script editor.
- `@spearfia/*` npm packages are no longer part of this repo.
