<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Editing intent, visual evidence and captions

Subsequent creator instruction permits isolated contextual auditions from the
frozen Star 4 v3 snapshot: four moments, pop alternatives and existing audio,
followed by a separate proxy preview after layout selections. This is a specific
preview exception; general deployment, GTA History and HQ finalization remain
disabled. Preserve the archived review and current Stage 6 job controls.

Current creator policy (2026-11-01): continue development on complete short videos
of at most two minutes, using pop motion only. Deployment is disabled independently
of calibration. Do not apply experimental editing to the current John Wick
project, GTA History or large videos. Preserve the earlier Star 4 reviews.

The completed historical creator review contained 111 labels, but only thirteen
windows had actual Clef responses. All thirteen chose keep; the creator wanted a
pop graphic in nine of those. The other 98 were deterministic planner fallbacks,
not model predictions. Extra choices came from supplied Visual Script intent and
timing checks, so their presence cannot measure model opportunity detection.
The skills experiment's `editing-study` path now asks graphic usefulness on every
beat without the treatment menu, then independently judges supplied pop treatment
fit where candidates exist. It reports each component separately, never applies
edits, and does not interpret absent candidates as model keep decisions.

Design requirement recorded 2026-11-01 while reviewing the isolated editing pilot.
Reuse the approved script, Visual Script, recorded narration and existing VideoJev
work. The complete handoff described below is **not implemented**. Do not describe
the pilot's three manually selected treatments or shadow packets as an automatic
editor. No production approvals are granted by this document.

## Existing groundwork

| Existing part | What it supplies | What is still missing |
| --- | --- | --- |
| [Visual Script prompt](stages/04-visual-script.md) | Cues per beat/1-3 sentences; exact visual requirements, text and a small executable edit draft | Consistent purpose, reveal order, constraints and deliberate holds carried into editing |
| [Visual Script](visual-script.md) | Stable cue IDs, exact narration anchors, references and `ACTION:`, `GRAPHIC:`, `CONTINUITY:` paragraphs | A compiler from that intent and actual visual evidence to richer treatment candidates |
| `spearfia-sync::Timing` | Approved script words aligned to ASR; read-only `editing_words` bridge for the worker | Acoustic verification of uncertain spans |
| FilmCraft adapter captions | Script spelling at aligned times; upgraded worker shares phrase pages with native captions | Native animated word emphasis and production integration |
| VideoJev workflow in the skills repository | Script-first beats, source groups, temporal grounding, choice checks and saved per-question results | An editing-side consumer of that evidence, tied to the actual selected footage |
| [Remotion pilot](../tools/editing-pilot/README.md) | Three treatments, four motion presets, appearance profiles, script-aligned caption styles and separate reviews | Live calibrated Clef, attached VideoJev observations and production integration |

At inspection, the original John Wick project's Visual Script had 0 `ACTION:`
paragraphs, despite 45 `GRAPHIC:` paragraphs. The formats can hold richer intent;
existing plans and prompts do not consistently supply it. This is an observation,
not permission to rewrite that project.

The separately developed VideoJev implementation is in the `spearfia/skills`
repository, local worktree `C:\dev\spearfia\skills-videojev`, branch
`feat/videojev-media`. Read its `videojev-media/SCRIPT-FIRST.md`, `STAGE-1.md`
and `STAGE-4.md` before using its workflow. This is not an app-monorepo module.
The local demo at `D:\SpearFia\projects\cwi-003-jev-demo` has a receipt for its
670-sentence interpretation (300 specific, 370 general), matching the inspected
beat file and saved narration hashes. Its beat JSON retains an old draft status
string: validate the actual receipt and source hashes, not that label alone.
This receipt never authorizes another project or a changed script.

## Required planning handoff

The script stage already supplies chapter beats. During Visual Script planning,
reuse them and record the editorial intent at useful beats. Use existing paragraph
roles and string-valued cue details; this does not introduce executable syntax or
change a public schema. New formal contracts must be specified in `spearfia-spec`
before adding runtime consumers.

- Purpose: explain, compare, establish evidence, orient, emphasize, sustain mood,
  or intentionally keep the current presentation. Intentional holds count as plans.
- Narration anchor and reveal order: exact words that trigger each change; resolve
  frames against the recorded voice later, without inventing timings before VO.
- Content and evidence: exact labels/numbers, source references, and whether imagery
  must demonstrate a specific event or only provide subject/mood context.
- Visual behavior: what persists, changes state, enters or leaves. Preserve useful
  scene continuity across several spoken beats; a new sentence need not reset it.
- Constraints: required assets, reading time, caption space, continuity with nearby
  cues and any prohibited implication. Keep missing evidence explicit.
- Sound intent when useful: movement, new information or significant state change;
  final palette, level and transient placement belong to the channel/render stages.

For example, this is planning prose within a correctly anchored existing cue:

```text
VISUAL: Contrast winning the immediate fight with escaping the wider search.
ACTION: Reveal "Fight won" at "He wins the fight"; reveal "Still wanted" at
"he is still wanted" and move emphasis while keeping the first statement visible.
TEXT: Fight won / Still wanted
GRAPHIC: Paired comparison; retain the established background context.
CONTINUITY: Reserve the caption band; leave before the next comparison begins.
PRODUCTION NOTE: This is a narration comparison, not proof that the shot shows an escape.
```

The editing agent first checks this handoff for gaps. Make one editorial pass per
section to fill missing intent, retaining cue IDs and exact/subject/mood requirements.
Do not regenerate completed planning sentence by sentence or rescore unchanged
material on each render. Where an approved script-first interpretation exists,
preserve it; do not replace it with legacy Subject pools or old cue-duration quotas.
Future planning writes still go through Spear Fia's revision-checked operations.
Do not edit a frozen plan or its approval receipt to add editing metadata.

## VideoJev supplies visual evidence; Clef judges the treatment

The Visual Script says what is wanted. VideoJev scores what matches supplied
questions in the footage. Keep those two records separate when they disagree.
Temporal grounding and multiple-choice video QA are supported by the
[upstream model](https://github.com/MCG-NJU/VideoJev). Neither model generates the
needed labels, explanations or code.

Construct Clef's visual input mechanically from the VideoJev question, supplied option
labels, probabilities, source/time bounds and review status. A selected option is a
scored observation, not a generated description or a proven fact. Do not ask VideoJev
to write prose or use another model to paraphrase every saved result.

1. Resolve the actual assigned source clip, source interval, crop, handles and
   timeline mapping. A different shot or crop can invalidate earlier observations.
2. Reuse the existing beat interpretation and relevant per-question results with
   their full curves, options, source bounds, model revision and review status.
   Record source/asset fingerprints, sample rate and question identity. Scores from
   another query or variant are not interchangeable.
3. For a missing fact that matters, ask a bounded question or locate the described
   event with the existing resident VideoJev worker. Group work by physical source
   window so encoding can be reused; do not query every general filler shot. General
   footage in the accepted script-first workflow remains query-free unless a new
   editorial need justifies a question.
4. Retain positive, absent and unclear outcomes. New choice questions always include
   `none of these`; a missing cached result means untested, not absent. Preserve older
   results and their original options. Known bad sources/outros and rejected regions
   remain excluded. No new inference is authorized simply by writing this document.
5. Give Clef the intent, relevant visual scores and their status/uncertainty, local
   narration, available assets, reading time, neighboring edits/sounds and channel
   criteria. Let it judge eligible treatments versus keeping the current footage and
   `none of these`. Missing context or conflicting evidence goes to editorial review.

The local VideoJev tests demonstrate real false positives, so a high score is not
independent verification. Preserve named-scene review requirements. The tested
resident path uses Ubuntu WSL2 and context 32,768; do not silently switch to Windows
SDPA or a smaller context for speed. Stage 0b measured roughly 3.4-3.9 seconds per
warm single grounding query on its larger windows, not Clef's subsecond latency.
Encoding reuse helps, but saved query results avoid the grounding cost as well.

Temporal scores do not supply tracked pixel boxes or audio judgments. Use a separate
frame inspection, OCR or tracker when placement requires exact subject coordinates,
text exclusion zones or object motion. Sparse sampling cannot establish a precise
impact frame or subsecond sound cue; inspect the short candidate locally at suitable
frame resolution. Keep probabilities and timing uncertainty visible to downstream code.
Clef thresholds remain advisory. Calibration cannot override the creator's
separate disabled-deployment policy.

## Shared engine, channel pack, episode binding

Share cue/word timing, evidence handling, layout/readability checks, motion primitives,
caption planning, sound synchronization, rendering, caching and review. Keep fonts,
colors, treatment variants, pacing, permitted intensity, source presentation and sound
palettes in channel profiles. Episode plans supply words, claims, sources and assets.
CrazyWhatIfs and a CFB news channel can use the same comparison or caption engine with
different styling and editorial policies. Never put a channel's heading or a video's
specific narration phrase into the reusable engine. The upgraded renderer accepts
profiles and episode labels separately; the legacy Star 4 sampler is still episode-specific.

## Caption upgrade requirements

The main pipeline already has script-correct words: `crates/sync/src/lib.rs` aligns
script words to ASR using longest common subsequences, interpolating unmatched words.
`crates/adapters/filmcraft/src/edit.rs` uses those words for native captions. The pilot
originally grouped raw ASR text by six words/32 characters. The optional `upgrade`
path now reuses the main aligner and a measured phrase plan. Keep reusing that path;
do not create another transcription/alignment pipeline for each visual style.

Alignment is not acoustic forced alignment: interpolated words and end times inferred
from the next word can include silence. Add explicit quality flags and pause handling;
review doubtful names, negations, numbers and script/VO mismatches. Do not present an
unspoken script word as measured speech or silently fix mismatches with a visual style.
Any later force-alignment work should target the uncertain spans and preserve originals.

Use one word/timing source for editable native captions, optional animated Remotion
overlays and SRT/VTT export. Adopt the official
[`@remotion/captions` data format](https://www.remotion.dev/docs/captions/caption)
at the render boundary. Preserve script word IDs and timings separately from styles.
Phrase grouping should respect punctuation, names, negations, pauses, measured width
and reading time. ASR character limits alone are not a caption design.

The worker now supports a clean phrase style, active-word accent and restrained
emphasis, auditioned on the same 24 seconds. Creator selection remains pending. Keep
font weight stable, wrapping predictable, contrast legible and the caption area clear
of graphics. A news channel can choose calmer presentation with the same data/engine.
Use code for word highlighting and fit calculations. Clef may choose among supplied
emphasis or placement candidates when editorial judgment adds value; do not call it
for every spoken word. All choice sets include `none of these`.

Acceptance: accurate approved wording that matches the recorded speech; timing and
pause checks; no chopped names/negations; stable readable layout at phone size; no
graphic collisions; repeatable rendering; correct editable/exported captions; creator
audition. A decorative SRT renderer alone does not satisfy this list. Keep native
captions editable, and prevent double display when an animated version is active.

See the [pinned community-source review](research/remotion-repo-review.md) for concrete
reuse candidates and limitations. No third-party code or new dependency was imported
during that review. The existing production timeline and pilot outputs are preserved.
