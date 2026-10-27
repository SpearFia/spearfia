# 5. First FilmCraft push: links to a review timeline

- Status: proposed
- Date: 2026-10-26

## Context

The agent pipeline ends with: script written, VO files generated per section, B-roll found,
media linked to script words (`links.json`). The next step is a timeline the creator can
review in FilmCraft. `spearfia-sync` is a skeleton and `spearfia-adapter-filmcraft` only
finds the install. [filmcraft-recon.md](../filmcraft-recon.md) records what FilmCraft 0.9.0
offers: `timeline.place`, `sequence.applyVideoTransition`, `clip.rename`, `file.saveAs` and
the others are there; `command_batch` is not atomic, clips have no custom metadata, and
there are no change events.

This ADR covers the first **one-way push** only. Pull and three-way merge come later and
build on what this one writes.

## Decision

### 1. Compile (pure, `spearfia-sync`)

`compile(script, links, media, timebase) -> (TimelinePlan, Report)`, no I/O. `media` maps
each link's media path to its FilmCraft item id and duration (from step 2a). Rules:

1. **Anchor.** Each link is found again with `links::locate` (quote + line hint). Lost links
   go to `Report.lost` and are skipped, never guessed.
2. **Order.** Links sort by script position (line, first word).
3. **VO sets the timing.** Audio-kind links are laid back to back on `A1` from frame 0.
   Length = `out - in` when both are set, else the file's duration from `in`.
   Each VO clip **owns the script words from its anchor up to the next VO anchor**. That
   stretch is its *span*.
4. **Picture fills the span.** A picture link goes to the span that holds its first word.
   Its start is the span start plus the span length times its word offset in the span
   divided by the span's word count (a linear word-to-time map; analysis transcripts can
   replace it later). It runs until the next picture link on the same track, or the span
   end. Track by role: A-roll and screen on `V1`, B-roll `V2`, graphics `V3`. Picture
   placed with `audio: false`, so B-roll sound never fights the VO.
   Media shorter than its slot keeps its own length and the shortfall goes in
   `Report.short`. Picture links before the first VO anchor go to `Report.unplaced`.
5. **Transitions.** A `:transition[label]{#t1}` cue becomes a `transition` item on the
   first picture clip that starts at or after the cue's word, `edge: "in"`. Labels map to
   FilmCraft effects with a small table (`dissolve`/`cross dissolve` → `cross_dissolve`,
   `dip to black` → `dip_to_black`); `cut`, `hard cut` and `smash cut` add nothing; other
   labels go to `Report.unmapped`.
6. **IDs and timing.** `cue_id` is the link id (`k12`) for clips and the cue id (`t1`) for
   transitions. Times are frames in the project's `timebase`. `intent` holds the slot
   the script asked for, the item's own fields what was actually placed.

Music and SFX cues have no linked media yet and are not compiled. Effects are out of scope.

### 2. Apply (`spearfia-adapter-filmcraft`)

The adapter starts `filmcraft-cli mcp` **headless** (empty in-memory project) as a child
process and talks newline-delimited JSON-RPC over its stdio: `initialize`, then
`tools/call`. That is about a hundred lines on `serde_json` and `std::process`, so no MCP
client crate is added.

1. **Import.** `media_import {paths}` (absolute) for every distinct linked file, then
   `project_inspect` for item ids and durations → `media` for step 1.
2. **Compile** the plan (step 1) and write it to `sync/plans/<plan_id>.json`.
3. **Build.** `file.newSequence {name, fps, width, height, video: 3, audio: 1}`, then
   `command_batch` of `timeline.place {item, track, frame, sourceIn, duration, audio|video}`
   in plan order, then a second batch from the returned clip ids: `clip.rename {clip,
   name: "<file name> {#k12}"}` for each clip and `sequence.applyVideoTransition {clip,
   edge, effect, frames}` for each transition. Frames go to ticks as
   `frames * 254_016_000_000 * den / num` in `i128` (exact for the NTSC rates).
4. **Commit.** `file.saveAs {path: "sync/review-<plan_id>.fcproj"}`, then `sequence_inspect`
   saved as `sync/snapshots/<plan_id>.json`, the base snapshot for the future pull.

**Transaction = the save.** Nothing reaches disk until step 4. If any step fails, the
adapter kills the child, the in-memory project is gone, and the plan file is marked
failed in the report. No rollback code. Each push writes a **new** `.fcproj` and never
overwrites an earlier one, because the creator may already have edited it in FilmCraft.

Bridge mode (pushing into the running app) is later. It needs the rollback that headless
mode avoids.

### 3. Entry points

The plan and snapshot files are written through `spearfia-core` (its `write_json`), and the
push is one core command so the CLI (`spearfia push filmcraft`), the desktop button and the
MCP tool (`timeline_push`) share it. It refuses to run without the creator's production
handoff approval (`sync/approved-script.md`), like other production steps.

## What FilmCraft lacks (upstream, via `CaesarFia/filmcraft`)

| Gap | Workaround in this push | Upstream change |
|---|---|---|
| No per-clip metadata (recon "Timeline item metadata") | cue id in the clip name, `{#k12}` suffix. Visible, and lost if the user renames the clip | `TrackItem.meta` / `Transition.meta` (`BTreeMap<String, String>`, `serde(default, skip_serializing_if = "is_empty")`), `clip.setMeta {clips \| transition, key, value}` as one undo step, `meta` in `sequence_inspect`, carried over on razor, duplicate and paste |
| `command_batch` not atomic | headless build, save only at the end | `command_batch {atomic: true}` running the steps in `Session::edit_group` (one undo entry, rollback on error). Needed before bridge-mode push |
| No change events | not needed for push; pull polls `project_inspect.revision` | optional: MCP `resources/subscribe` + `notifications/resources/updated` on `filmcraft://document` |
| `fps` is a float | Spear Fia's rational timebase sent as `num / den`; `FrameRate::from_f64` snaps to the common rates | optional: accept `fps: {num, den}` in `file.newSequence` |

Only the first row blocks the pull. None blocks the push.

## Changes in `spearfia-spec` first

- **`links` schema.** `links.json` has none yet (`crates/core/src/links.rs` says so); the
  compiler's input should be pinned.
- **`timeline-plan`.** Replace the `$comment` TODO with: items apply in array order; tracks
  `V1..Vn` / `A1..An` are created as needed; a plan applies to a fresh sequence, all or
  nothing. Document transition `params`: `{effect, edge, attach: <cue_id of the clip>}`, and
  `source.out_frame`. Both fit the current schema (`additionalProperties: true`), so this is
  documentation, not a breaking change. Bump to `v1.1`.

## Consequences

- The creator gets a reviewable FilmCraft project from links alone, with no editor running.
- Re-pushing never destroys FilmCraft edits, but also never merges them: every push is a
  new file until the pull exists.
- Linear word timing is approximate. Picture cuts can land a word or two off until
  transcripts drive the word-to-time map.
- Clip names carry the cue ids until FilmCraft has clip metadata. Users who rename clips
  break the link for the future pull (the push itself is unaffected).

## Build order (when approved)

1. Spec: `links` schema, `timeline-plan` v1.1.
2. `spearfia-sync::compile` with tests over a small script and links fixture (VO spans,
   proportional picture starts, lost and unplaced links, transition mapping).
3. Adapter: stdio MCP client, import, build, save; one integration test against
   `filmcraft-cli mcp` that is skipped when `detect()` finds no CLI.
4. Core command, CLI subcommand, then the desktop button and MCP tool.
