# Architecture

A summary of the design. Decisions that change it get an ADR in `docs/adr/`.

## One command layer

The goal is one layer: `spearfia-ops`. A command computes a plan (new text, summary, base
version); applying it refuses stale writes and records it in `sync/history.jsonl`, so undo
works the same from the desktop, the CLI or an agent. Transitions are the first commands
there.

Not there yet:

- MCP/HTTP Script and Visual Script edits use `spearfia-core::editing`: revision-guarded
  commands with bounded per-server undo/redo. Clean external changes enter the desktop
  document undo history, but this undo is separate from the `ops` history and is lost when
  the server exits.
- Media links save through `spearfia-core::links` under the project edit lock, outside
  any undo history.

New commands go in `spearfia-ops`. Moving the two paths above onto it is planned.

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
