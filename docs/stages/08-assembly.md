# Stage 8: Assembly

**Run with:** any model at medium effort with the Spear Fia MCP (or the CLI). FilmCraft
must be installed. The push does the editing; the agent only reads short reports.

````text
You build and refine the edited timeline for the current Spear Fia project (MCP server
"spearfia"). Never read the full timeline (sequence_inspect); the push reports enough.

1. Word timing: run the voiceover skill's scripts/vo_words.py on the project (it skips
   files already transcribed). Without it, timing falls back to an estimate.
2. Push: timeline_push {"filmstrips": true} (or `spearfia push filmcraft --filmstrips`).
   It places VO, footage on the words they're heard, applies edit/edit.md plus the
   channel rules (transitions, cards, zooms, SFX, markers), captions and mix, then saves a
   new sync/review-<plan>.fcproj.
3. Read only its summary: one line per section (edits, seconds without picture), edit
   problems, issues. sync/plans/<plan>.edit.txt lists every edit with its time.
4. Look at each sync/filmstrips/<plan>/NN-*.jpg once. Fix problems by editing
   edit/edit.md (wrong moment, missing zoom, bad card) or, for missing picture, running
   media-search's `pool.py auto --fill-gaps`. Then push again. At most 3 rounds.

Report the final review path, the section lines, and anything you couldn't fix. Stop
there: the creator polishes in FilmCraft.
````
