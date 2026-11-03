<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Motion scene cues

**Status (2026-11-02): planning rules adopted, render stage not built.** The Visual Script
stage ([stages/04-visual-script.md](stages/04-visual-script.md)) writes scene cues by this
doc; each channel's idioms, density and slam budget live in its style.md § Visuals.
Rendering (§ 7) waits for the cwi-005 pilot. Promote to `spearfia-spec` only if code will
parse the new fields. Nothing here changes a schema, a skill or a frozen project.

## 1. Why

Some beats carry structured information that footage cannot show: a count, a date, a
ranking, a comparison, a process. A motion scene is a designed full-frame picture that
replaces footage for that beat. It must be planned in the Visual Script so each spoken
beat has exactly one picture owner. If it is planned after the edit, `media-search` has
already filled the span with footage, and the scene either covers it (wasted search) or
overlaps it (redundant picture).

Reference: three Glooby videos studied on 2026-11-02 (frames, cut counts, audio
transients). Observed, not copied: one fixed style, a bespoke scene per beat, few hard
cuts, entrances that carry a sound, and exact figures that count up. No graphic, sound
or curve from those videos is reused.

## 2. Scope and non-goals

- In: planning contract, cue encoding, anchors, render and link contract, guards.
- Out: the overlay treatments in `tools/editing-pilot` (pop on footage). They keep their own
  locks: deployment disabled, pop entrances. A scene stage is separate and needs its own
  creator go-ahead. Since 2026-11-03 the scene families are allowed in experiments and
  videos of any length may be used.
- Out: writing claims. A scene shows numbers and labels the Visual Script supplies. It never
  invents or verifies a figure.
- Out: a template catalog. Idioms (§ 4) are vocabulary for the writer; the renderer builds
  each scene for its data.

## 3. Picture owner rule

Every narration span has one owner, set by the cue's `method`:

| Owner | `method` | Filled by | Notes |
| --- | --- | --- | --- |
| Footage | `LIVE footage` | `media-search` | Default |
| Overlay on footage | `LIVE footage` plus `TEXT:`/`GRAPHIC:` treatment | editing stage | Footage stays visible |
| Motion scene | `Remotion template` | scene render | Replaces footage for its span |
| Card | `Generated artwork` / generated card | the push | Existing |
| Hold | `Intentional hold` | nobody | Existing |

Choose a scene only when all three hold (from the philosophy: earn the interruption):

1. The sentence states structured information: numbers, dates, an order, a comparison,
   a relationship or a process.
2. Footage cannot show that information, or would only illustrate the mood.
3. The beat is complex enough to deserve the viewer's full attention, yet short enough
   to read once.

Otherwise keep footage and, if needed, an overlay. A scene may span several spoken beats
(one cue, up to about 20 s) when the picture persists and changes state. A new sentence
need not reset it. Target density is a creator decision; start low.

## 4. Scene idioms (vocabulary, not templates)

Observed across the references. The writer names one in `GRAPHIC:`. The "needs" column is
what the cue must supply.

| Idiom | Use for | Needs |
| --- | --- | --- |
| counter | one exact figure | value, unit, source, estimate flag |
| bars / race | comparing 2–6 amounts | table, unit, order of reveal |
| timeline | dates, delays, durations | axis range, events, which moved |
| calendar | a single date | date, what happens |
| ranking | ordered list, chart positions | rows, metric, source |
| map / scale | size or place comparisons | areas, units, reference |
| balance | two sides weighed | two labelled sides |
| flow / tree | cause and effect, process | nodes, edges, order |
| source document | a quote or report with provenance | exact text, source |
| versus card | two sides, one still and one stat each | both stills (film and scene), both stats, source |
| tier list | a row of portraits marked out or sorted | each still (film and scene), order of marks |

New idioms are added to this table when a real cue needs one, not in advance. A channel
may allow only a subset (its style.md § Visuals); writers use only the allowed ones.

## 5. Cue encoding

Uses only existing mechanisms: the cue record, paragraph roles and string-valued
`details`. No schema change. Use the fields the desktop form already offers:

- `visual_type`: `Chart` or `Diagram` for data scenes, otherwise `Scene`.
- `method`: `Remotion template`.
- `details` for `Chart`/`Diagram`: `Data table`, `Units`, `Population`, `Sources`,
  `Approximation flags`, `Axes / scales`, `Relationships`, `Reveal order`.
- `details` for `Remotion template`: `Template / rig / revision` (idiom name and draft
  revision), `Template parameters`, `Props`, `Missing assets / preparation tasks`.
- Body: `VISUAL:` intent, `TEXT:` literal labels and numbers, `ACTION:` reveal anchors,
  `GRAPHIC:` idiom and what persists, `CONTINUITY:` camera, persistence and caption
  space, `PRODUCTION NOTE:` evidence caveats and sound intent.

Reveals use the existing grammar: `ACTION: Reveal "<exact unit>" at "<exact spoken words>"`.
Anchors must be unique in the narration. A missing anchor stays a flag; no time is invented
before the voiceover exists.

Worked example (illustrative values):

```markdown
## Squadron 42 slips
<!-- visual-cue {"id":"v41","anchor":{"quote":"VO: Grand Theft Auto 6 is still coming November 19. Squadron 42 was due in the fourth quarter of 2026, and now it is not coming until the second quarter of 2027.","line":212,"lines":2,"kind":"VO","before":"","after":""},"visual_type":"Diagram","method":"Remotion template","asset_state":"missing","review":"draft","timing":"","details":{"Template / rig / revision":"timeline@draft1","Template parameters":"axis 2025-2029","Data table":"GTA VI | Nov 19 2026 | fixed; Squadron 42 | Q4 2026 -> Q2 2027 | moved","Units":"calendar dates and quarters","Sources":"to cite before approval","Approximation flags":"none","Axes / scales":"x: time, linear","Reveal order":"axis; GTA VI; Squadron 42 old date; DELAYED; Squadron 42 new date"},"references":[]} -->

VISUAL: Full-frame timeline replaces footage; no footage needed under it.
TEXT: GTA VI · NOV 19 / SQUADRON 42 · Q4 2026 / DELAYED / NOW · Q2 2027
ACTION: Reveal "GTA VI · NOV 19" at "November 19"; reveal "SQUADRON 42 · Q4 2026" at "fourth quarter of 2026"; reveal "DELAYED" at "not coming"; reveal "NOW · Q2 2027" at "second quarter of 2027".
GRAPHIC: Idiom timeline. The axis persists; the Squadron 42 badge slides from its old date to the new one after DELAYED lands.
CONTINUITY: Reserve the caption band. Leave before the next cue.
PRODUCTION NOTE: Dates are as announced; cite the source. Sound: pop on each label, whoosh on the slide, one slam on DELAYED.
```

## 6. Pipeline placement

| Stage | Change |
| --- | --- |
| 4 Visual Script | Writer assigns an owner per beat (§ 3) and writes scene cues (§ 5). Creator reviews scene cues here, so the plan is approved before any render. |
| 6 Voiceover | Unchanged. Word timings resolve the `ACTION:` anchors. |
| **6b Scene render (new)** | Compile each scene cue to render props, resolve reveal frames from word timings, render, link. |
| 7 Media search | Unchanged. It already fills only `LIVE footage` cues. |
| 8 Assembly | Unchanged. A linked scene clip covers its span like any picture. |
| 9 Music and SFX | Reads the scene's sound-cue list onto A3. |
| 9b Channel-style edit | Overlays apply to `LIVE footage` cues only. |

## 7. Render and link contract

Input: the cue, the channel profile, resolved reveal frames. Output, per scene:

- `scenes/<cue-id>.mp4`, full canvas, no audio.
- `scenes/<cue-id>.sfx.json`: `[{frame, role, weight}]`, generated from the same beat
  frames as the animation. `role` is `pop`, `whoosh` or `hit`; `weight` is `tick`, `pop` or
  `slam`. This is how sound stays locked to the picture.
- `scenes/<cue-id>.props.json`: the exact compiled inputs and their hash, so a re-render
  is reproducible and a changed cue is detected.

Link with `links_add`: `quote` equals the cue's anchor quote, `role` `graphics`, `kind`
`picture`. Mark the cue `asset_state: ready` only after the link exists.

Style comes from the channel profile (for CrazyWhatIfs, `profiles/crazywhatifs.json`:
colours, type, pop spring), never hard-coded in a scene. Entrances use the approved pop;
the slam level is rare, about once per section.

## 8. Guards against redundant footage

1. **Owner exclusivity.** A lint reports any narration span covered by two owners, or by
   none, before assembly.
2. **Search.** `media-search` skips non-`LIVE footage` cues. No change needed.
3. **Gap fill (known trap).** `pool.py auto --fill-gaps` fills any uncovered stretch, even
   under unlinked cues. Scenes must be rendered and linked before it runs, or `gaps()`
   must skip cues whose method is `Remotion template`. The second is a small code change
   and is the safer default.
4. **Staleness.** If a scene cue or its narration changes after render, its props hash no
   longer matches and the link is reported stale, not silently reused.

## 9. Checks per scene

- Readable in one second at phone size; makes sense muted.
- Every number has a source, or shows an estimate flag on screen.
- The picture says what the sentence says, nothing more (no implied claim).
- Swap test: another channel could not use it unchanged.

## 10. Open decisions for the creator

Decided 2026-11-02 for CrazyWhatIfs: stills from the films are allowed inside scenes
(moving footage inside a scene is still v2); about one scene per 3-4 minutes; this doc
lives in `docs/` until code parses the fields. Still open:

1. **Authorization.** The editing path is locked. The scene render stage needs its own
   explicit go-ahead after the pilot.
2. **Pilot video.** The two-minute limit was lifted on 2026-11-03; the pilot can be the
   full cwi-005 video or an excerpt.
3. **Still sourcing.** Who extracts stills (media-search from its clips, or the scene
   stage) and where they are stored.

## 11. Not verified

- How `spearfia-sync` places a cue whose method is `Remotion template` (read only the
  `Card` path and `pool.py`).
- Whether the desktop form preserves unknown `details` keys. This draft avoids new keys.
- The channel `production.md` and `style.md` on `D:\SpearFia`; no channel rules were read.
