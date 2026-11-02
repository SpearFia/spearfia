# Stage 8: Assembly

**Run with:** any model at medium effort with the Spear Fia MCP (or the CLI). FilmCraft
must be installed. The push does the editing; the agent only reads short reports.

````text
You build and refine the edited timeline for the current Spear Fia project (MCP server
"spearfia"). Never read the full timeline (sequence_inspect); the push reports enough.

Read docs/editing-handoff.md. Reuse saved beat intent and applicable VideoJev evidence;
do one section-level gap review instead of replanning every cue. Keep unverified or
missing exact footage explicit. Richer Remotion treatments and Clef selection remain
an isolated pilot, not operations the normal push already supports. Caption upgrades
must reuse approved script words and shared VO alignment, with uncertain timings
flagged; do not replace them with raw ASR because a renderer accepts SRT.
If this project uses the accepted VideoJev script-first timeline workflow, follow
its documented build/review path and active stage gates instead of replacing it
with the standard push below. The remaining steps apply to the standard pipeline.

1. Word timing: run the voiceover skill's scripts/vo_words.py on the project (it skips
   files already transcribed). Without it, timing falls back to an estimate.
   Title sting and end screen: when the style has `edit.intro` / `edit.end_screen`, make
   their files once per video (paths in the style): the sting background (a 4 s clip of
   the video's subject, slightly darkened), the end-screen background (four 5 s clips,
   blurred and darkened, 20 s), and with audio-gen a 2–3 s title hit (SFX/) and a ~22 s
   outro bed (Music/). The push holds the narration for them and reports a missing file.
2. Push: timeline_push {"filmstrips": true} (or `spearfia push filmcraft --filmstrips`).
   It places VO, footage on the words they're heard, applies edit/edit.md plus the
   channel rules (transitions, cards, zooms, SFX, markers), captions and mix, then saves a
   new sync/review-<plan>.fcproj.
3. Read only its summary: one line per section (edits, seconds without picture, checks
   flagged), `checks`, `not_applied`, `word_timing`, edit problems, issues.
   sync/plans/<plan>.edit.txt lists every edit with its time.
4. Fix every `checks` line first; each names its time, its edit line or cue and the fix
   (a card off the frame, over the captions, over black, or too short to read). Card text
   is the cue's TEXT lines in visuals/visuals.md; timing and placement are edit/edit.md.
   `not_applied` settings are known gaps: don't look for them in FilmCraft.
5. Look at edits.jpg (one frame per card and zoom, in edit.txt order) and each
   sync/filmstrips/<plan>/NN-*.jpg once, for what checks can't measure: the wrong moment,
   a weak shot, a missing zoom. Fix in edit/edit.md or, for missing picture, with
   media-search's `pool.py auto --fill-gaps`. Then push again. At most 3 rounds.

Report the final review path, the section lines, and anything you couldn't fix. Stop
there: the creator polishes in FilmCraft.
````
