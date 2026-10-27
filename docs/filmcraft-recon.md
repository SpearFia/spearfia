# FilmCraft recon

What `spearfia-adapter-filmcraft` needs from FilmCraft, what exists, and what's still
open. Checked against FilmCraft 0.9.0 (`C:\dev\oss\filmcraft`, commit `1e71fd10`). File
references below are FilmCraft paths at that commit. The push design that uses these
answers is [ADR 0005](adr/0005-first-filmcraft-push.md).

## MCP tool list

**Exists.** `filmcraft-cli mcp` serves MCP on stdio, headless (`--demo`, `--project p.fcproj`)
or bridged to the running app (`--bridge 127.0.0.1:9876`, app started with
`filmcraft --control 9876`). See FilmCraft `docs/agents.md` §1 and `docs/control-protocol.md`.

Tools that matter for sync: `command_list`, `command_run`, `command_batch` (ordered steps,
`stop_on_error`), `doc_inspect`, `project_inspect`, `sequence_inspect` (tracks, clips with
ticks and frames, effects, transitions, markers), `media_import`, `render_frame`.
Edits made through `command_run` are undoable in FilmCraft.

- [x] The command IDs a timeline plan compiles to (all in `crates/engine/src/commands.rs`
      unless noted). Time params take `time` (ticks, 254 016 000 000/s), `frame` or
      `seconds`; `sourceIn` and `duration` take ticks only.

      | Plan step | Command | Params that matter |
      |---|---|---|
      | New project | `file.newProject` (l. 1140) | `name` |
      | New sequence | `file.newSequence` (l. 1173) | `name`, `width`, `height`, `fps` (a float, `FrameRate::from_f64`; no `num/den`), `video`, `audio` (track counts) |
      | Add tracks | `sequence.addTracks` (l. 2166) | `video`, `audio` counts |
      | Import media | `file.import` (l. 1332), or the `media_import` tool | `paths` (absolute); the new item ids come back, durations via `project_inspect` |
      | Place a clip on a track at a time | `timeline.place` (l. 2580, `place_one` l. 843) | `item`, `track` (`"V2"`), `audioTrack`, `frame`, `sourceIn`, `duration`, `video` / `audio` booleans, `insert`; returns `{clips: [ids]}`. Start and end snap to the frame grid |
      | Add audio (VO, music, SFX) | `timeline.place` with an audio track as `track` (sound only), e.g. `{"item":7,"track":"A1","frame":0}` | an audio track as `track` also takes a video file's sound alone |
      | Picture only (B-roll with sound muted) | `timeline.place` with `audio: false` | |
      | Add a transition | `sequence.applyVideoTransition` / `sequence.applyAudioTransition` (l. 2103 / 2112, `apply_transition` l. 4150) | `clip`, `edge` (`in`/`out`), `effect` (`cross_dissolve`, …), `frames`; returns `{transition: id}`. It sits on that clip edge: a cut if a clip abuts it, else a fade from/to nothing |
      | Apply an effect | `effects.apply` (l. 2924) | `clips`, `effect` (id or name) |
      | Tag a clip (fallback, see below) | `clip.rename` (l. 1784) | `clip`, `name` |
      | Save | `file.saveAs` / `file.save` (l. 1497 / 1493) | `path` |

- [x] `command_batch` is **not** atomic. It loops over `command_run`, and each edit is its
      own undo step (`crates/automation/src/server.rs` l. 341-364; the tool description says "each edit is
      its own undo step"). With `stop_on_error` the earlier steps stay applied. The engine does have an
      atomic group, `Session::edit_group` (`crates/engine/src/lib.rs` l. 876: one undo entry,
      full rollback on error or panic), but no command or tool exposes it to callers.
      `timeline.place {items: [...]}` uses it, so a list of items is one undo step, but only
      back to back on one track. So a transactional push needs either our own rollback
      (`edit.undo` × completed steps, checked against `history.list`) or a fresh
      project/sequence per push, or an upstream `command_batch {atomic: true}` (ADR 0005).

## Timeline item metadata

**Missing.** Two-way sync needs to store the script cue ID on each timeline item.

- [x] No clip, transition or effect carries custom key/value metadata.
      `TrackItem` (`crates/project/src/lib.rs` l. 511) has `name`, `label`, `markers` and
      fixed fields, but no free-form map; `Transition` (l. 709) and `EffectInstance` have
      none either. Free-form metadata exists only on **project items** (`ProjectItem.metadata`,
      l. 303, set with `metadata.set`, `crates/engine/src/panels.rs` l. 397). `metadata.set
      {clip}` writes the clip's *source item* (`metadata_target`, `panels.rs` l. 165), so two
      cues that use one file would share one value. Clip markers exist in the format, but no
      command writes them (`markers.add` writes sequence markers) and `sequence_inspect` does
      not report them (`commands.rs` l. 4262).

      What does survive save/load and edits: the **clip name** (`TrackItem.name`, serialized,
      settable with `clip.rename`, reported by `sequence_inspect`, copied to both halves on a
      razor cut). It is the interim carrier: name the clip `"<file name> {#cue}"` and parse
      the `{#…}` suffix back. The editor sees the name, and a user rename loses the tag.
- [x] Upstream change, proposed in ADR 0005: `TrackItem.meta: BTreeMap<String, String>` and
      `Transition.meta` (`#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]`, so
      old files load and unchanged files save byte-identical), a `clip.setMeta {clips | transition,
      key, value}` command (one undo step; empty value removes), `meta` in `sequence_inspect`,
      and carry-over on razor/duplicate/paste. Contribute via `CaesarFia/filmcraft`.

## Change events

**Missing.** No push notifications for document changes.

- [x] Confirmed: the control channel is strictly request/reply, one reply per request line
      (`docs/control-protocol.md` "Only requests are read"; `crates/ui-egui/src/control.rs`).
      The MCP server sends only `notifications/progress` for exports and handles
      `notifications/cancelled` (`crates/automation/src/long_job.rs`); there is no
      `resources/updated` or subscription support in `crates/automation/src`, though
      `filmcraft://document` is a resource.
- [x] Poll instead: `project_inspect` returns `revision` (`commands.rs` l. 4250), a counter
      that bumps on every edit, plus `dirty`. Poll that cheaply and call `sequence_inspect`
      only when it changed, then diff against the last-synced snapshot. The revision is
      per session (it restarts when the project is reopened), so the snapshot, not the
      revision, is what the merge compares against.

## Effect API

**Open.** Not needed for the first push. FilmCraft's effects are built-in WGSL shaders
(`crates/gpu/src/*.wgsl`); no external effect plugin API found yet.

- [ ] TODO: how are effects registered and parameterized? Could `plugins/focus-loupe/wgsl`
      load as an effect without forking?
- [ ] TODO: OFX support status (see FilmCraft `docs/roadmap.md`).
