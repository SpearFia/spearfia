# Architecture

A summary of the design. Decisions that change it get an ADR in `docs/adr/`.

## CLI production coordination

`spearfia-ai` adapts noninteractive Claude Code and Codex processes. The desktop and
`spearfia pipeline` call `spearfia-ops::production` for assignments, checkpoints, creator
gates, output verification, cancellation and recovery. It attaches project-specific MCP
tools without rewriting global agent settings. Agent undo cannot change creator stage
assignments. The artifact runner adds service adapters and durable jobs; `spearfia-analysis` validates
measured narration alignment, and `spearfia-sync` compiles executable captions, graphics,
native recipes and mix automation. Direct model-provider APIs and FilmCraft pull remain
separate work. See [production.md](production.md) and [execution](production-execution.md).

## One command layer

Every command goes through `spearfia-ops`. A command computes a plan (new text, summary,
base version). Applying it holds the project edit lock, refuses stale writes and records
the change in `sync/history.jsonl`, so undo and redo work the same from the desktop, the
CLI or an agent, across processes and restarts. Undo and redo refuse to run over a later
edit. The history keeps the newest 100 changes (32 MiB).

- Transitions (desktop, CLI) and media links (desktop, MCP/HTTP) are `ops` commands.
- MCP/HTTP Script and Visual Script edits are validated by `spearfia-core::editing` (paths,
  ranges, cue operations), which returns a proposal that `ops` applies with origin `agent`.
- Revisions everywhere are content versions (`spearfia_core::version`), so an agent, the
  CLI and the app agree on them.

Typing in the desktop editors saves directly under the same lock and keeps its own text
undo; clean external changes (including `ops` changes) enter that undo too. Moving media
files rewrites link paths outside the history, so an older link change can't be undone
past it.

## Intent here, realization in the editor

Spear Fia owns the intent: the script, its cues, the vision and the links between script
and footage. The editor (FilmCraft first) owns the realization: exact timing and the
finished timeline.

## Two-way FilmCraft sync

Every script cue has a stable ID (`{#m12}`), stored in the editor's timeline item metadata.

- **Push** compiles a timeline plan (`timeline-plan` schema) and applies it to the editor
  in one transaction.
- **Pull** maps editor changes back to the script by ID. A three-way merge against the
  last-synced snapshot resolves conflicts.
- Cues keep the timing the script asked for (intent) separate from the timing the editor
  actually has (actual).

Lives in `spearfia-sync` and `spearfia-adapter-filmcraft`. The first, one-way push
(links → review timeline) is [ADR 0005](adr/0005-first-filmcraft-push.md); what FilmCraft
offers and lacks is in [filmcraft-recon.md](filmcraft-recon.md).

## Per-project vision

A vision is layered: app defaults → preset → the project's `vision.yaml` → a section's
`:vision{}` override. Its weights bias catalog search and the AI's choices. Lives in
`spearfia-vision`.

## AI-friendly metadata

- Analysis sidecars hold transcripts, beats, shots, energy and events (`analysis` schema).
- Time is frames plus a rational timebase, and everything has an ID.
- Each sidecar records the analyzer and version that produced it, so it can be regenerated.

Lives in `spearfia-analysis`.

## Plugins expose capabilities

- An asset's capabilities have typed inputs, a `requires` list of the analysis they need,
  and `binds` that connect parameters to analysis data (for example a face-following
  loupe binds `center` to `face_track.center`).
- Recipes compose capabilities into looks.
- All of it is exposed as MCP tools with progressive disclosure, so the AI calls
  `apply_look("facecam-pip")` instead of rebuilding masks by hand.

Lives in `spearfia-catalog` and `spearfia-mcp`; descriptors live in `spearfia/plugins`.

## Pluggable catalog sources

Local folders now; a Git-based community registry and third-party APIs later. Every asset
has a namespaced ID (`spearfia/<asset>@<semver>`), a required license, a source and a hash.

## Text-editable script

The script is a plain `script.md` (see `spearfia-spec/docs/script-directives.md`). Spear Fia
watches it for external edits and writes it back round-trip safe: text it did not change
stays byte-for-byte the same. Lives in `spearfia-script`.

## Channel knowledge and creator approval

Channel guides are shared Markdown in the app config's `channels/<id>/` folders. A
video gets a `channel.md` snapshot of those guides, independent of its Vision, so later
channel lessons do not silently change an in-progress brief. These are ordinary Markdown
documents, with no change to the project JSON schema. The core channel operations back
both the desktop editor and read-only CLI; stale saves and oversized inputs are rejected.

The creator's approval records the exact saved script and channel brief in
`sync/approved-script.md` and `sync/approved-channel.md`. Production handoff compares the
current content with both records and refuses changed or unapproved content. Agents
can read/check the handoff but have no CLI approval operation. This is a local workflow
gate, not a security boundary against arbitrary filesystem writes.

See [agents.md](agents.md) for implemented agent entry points and the current boundaries
of implemented MCP/API editing and the remaining generation and FilmCraft pull scaffolds.

## Visual Script

Optional `visuals/visuals.md` holds visual directions with stable cue IDs, narration anchors, production methods, references and independent asset/review states. The desktop reuses the document editor. Core operations preserve unknown prose/metadata and back CLI exports plus the implemented project-bound MCP stdio/local HTTP editing tools. See [the Visual Script guide](visual-script.md) for current behavior and revision handling.
