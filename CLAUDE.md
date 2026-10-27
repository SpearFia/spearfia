# spearfia

Script-first, AI-assisted content creation (shorts to long-form YouTube), paired with FilmCraft.

## Layout

- `apps/desktop/`: `spearfia-desktop`, the native desktop app (egui/eframe 0.36, like FilmCraft). All Rust, no web UI.
- `apps/cli/`: `spearfia-cli`, binary `spearfia`.
- `crates/<name>/`: `spearfia-<name>` libraries: `core` (command layer, undo, project model),
  `script`, `catalog` (`src/sources/`), `vision`, `analysis`, `sync`, `ai`, `mcp`.
- `crates/adapters/filmcraft/`: `spearfia-adapter-filmcraft`. A `resolve/` adapter comes later.
- `schemas/`: pinned copy of `spearfia-spec` schemas. Don't edit here; see `schemas/README.md`.
- `examples/sample-project/`: a minimal project folder.
- `docs/`: `architecture.md`, `filmcraft-recon.md`, `adr/`. Start with `pipeline.md` (the
  end-to-end production plan, what is built and what is missing), then `channel-workflow.md`,
  `agents.md` (MCP/HTTP tools) and `visual-script.md`.

## Commands

```powershell
cargo run -p spearfia-desktop     # run the desktop app
cargo run -p spearfia-cli         # run the CLI
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All UI, CLI and MCP actions go through the command layer in `spearfia-core`. Don't add a
second path that edits project state directly.

## Naming

| Context | Form |
| --- | --- |
| Prose, UI, README titles | Spear Fia |
| GitHub org, URLs, crates, packages, IDs | `spearfia` |
| Rust crates | `spearfia-<name>` |
| npm packages | `@spearfia/<name>` |
| Asset and plugin IDs | `spearfia/<asset>@<semver>`, e.g. `spearfia/focus-loupe@0.1.0` |
| Schema IDs | `https://spearfia.dev/schemas/<name>/v1.json` |

Never write the second word of the name in all capitals.

## Rules

- Never commit media. Media lives on the external drive, not under `C:\dev`.
- The schemas in `spearfia/spearfia-spec` are the source of truth for every format.
  Change the schema there first, then the code or data that uses it.
- `main` is protected: work on `feat/`, `fix/` or `exp/` branches and open a PR.
- New files are dual licensed MIT OR Apache-2.0.
