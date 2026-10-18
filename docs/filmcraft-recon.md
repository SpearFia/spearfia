# FilmCraft recon

What `spearfia-adapter-filmcraft` needs from FilmCraft, what exists, and what's still
open. Checked against FilmCraft 0.9.0 (`C:\dev\oss\filmcraft`).

## MCP tool list

**Exists.** `filmcraft-cli mcp` serves MCP on stdio, headless (`--demo`, `--project p.fcproj`)
or bridged to the running app (`--bridge 127.0.0.1:9876`, app started with
`filmcraft --control 9876`). See FilmCraft `docs/agents.md` §1 and `docs/control-protocol.md`.

Tools that matter for sync: `command_list`, `command_run`, `command_batch` (ordered steps,
`stop_on_error`), `doc_inspect`, `project_inspect`, `sequence_inspect` (tracks, clips with
ticks and frames, effects, transitions, markers), `media_import`, `render_frame`.
Edits made through `command_run` are undoable in FilmCraft.

- [ ] TODO: list the exact command IDs a timeline plan compiles to (place clip, add
      transition, apply effect, add audio).
- [ ] TODO: does `command_batch` apply atomically (one undo step), or do we need our own
      rollback for a transactional push?

## Timeline item metadata

**Open.** Two-way sync needs to store the script cue ID on each timeline item.

- [ ] TODO: can a clip, transition or effect carry custom key/value metadata that survives
      save/load and edits? `sequence_inspect` exposes marker names and comments; that is
      a fallback, not the plan.
- [ ] TODO: if not, propose a small upstream change (contribute via `CaesarFia/filmcraft`).

## Change events

**Open.** No change-notification mechanism found in the docs; the only notifications are
export progress and cancellation.

- [ ] TODO: confirm whether the control channel can push "document changed" events.
- [ ] TODO: otherwise, poll `sequence_inspect` and diff against the last-synced snapshot.

## Effect API

**Open.** FilmCraft's effects are built-in WGSL shaders (`crates/gpu/src/*.wgsl`); no
external effect plugin API found yet.

- [ ] TODO: how are effects registered and parameterized? Could `plugins/focus-loupe/wgsl`
      load as an effect without forking?
- [ ] TODO: OFX support status (see FilmCraft `docs/roadmap.md`).
