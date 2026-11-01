# Editing treatment pilot

Experimental, optional Remotion worker for a read-only audiovisual audition. The Rust app and its existing edit parser are unchanged. Code is MIT OR Apache-2.0; source and generated media stay outside Git.

Three data-controlled treatments: progressive explanation card, paired comparison, and source/context card. Each has measured narration anchors and independently scheduled sound transients. The source-card demonstration explicitly quotes narration, not independently verified film evidence. No assets or sounds are copied from the reference video.

See the [initial treatment catalog](treatments.md) for fit criteria, sound behavior, Clef's role and the creator review questions.

## Prepare and review

```powershell
npm ci --ignore-scripts
python prepare.py --project D:\SpearFia\projects\cwi-003-john-wick-gta6-six-stars --out D:\SpearFia\pilots\editing-treatments-20261101
npm run studio -- --public-dir D:\SpearFia\pilots\editing-treatments-20261101\assets --props D:\SpearFia\pilots\editing-treatments-20261101\pilot.json
npm run render -- --folder D:\SpearFia\pilots\editing-treatments-20261101 --variant baseline
npm run render -- --folder D:\SpearFia\pilots\editing-treatments-20261101 --variant restrained
npm run render -- --folder D:\SpearFia\pilots\editing-treatments-20261101 --variant expressive
python filmcraft.py --folder D:\SpearFia\pilots\editing-treatments-20261101
```

Use a **fresh external output folder** for preparation. It refuses existing outputs and stale script/channel approval or voice observations. The sample begins at the measured “That is the answer…” anchor in Star 4 and lasts 90 seconds. Its baseline derives from the saved assembly plan, rather than claiming to reproduce all FilmCraft effects. Tiny saved-plan gaps become explicitly recorded holds; larger gaps remain unresolved. `prepared.json` records input hashes and every source segment. Original project files and approval records are never edited.

`pilot.json` is an experimental production-side input, **not a new public Spear Fia schema** or executable `edit/edit.md` syntax. Edit its labels, reveal frames and sound selections, then render again. `src/model.ts` validates ranges, ordering, provenance and relative assets. It reserves the lower screen area for shared captions. Captions currently use cached ASR observations and require review. Sound events bind an asset's recorded peak/transient to the visual target; actual perceptual synchronization still needs listening.

Rendering saves hash-addressed outputs and verification records, reuses verified matches and refuses to overwrite modified results. Review the same voice/footage with baseline, restrained and expressive versions. Six original procedural sounds are audition placeholders, not creator-approved assets or Stable Audio generations. Music uses an existing project bed at a proposed low review level. This first pilot does not implement automatic ducking or claim a finished mix.

`filmcraft.py` creates a **separate**, hash-addressed `filmcraft-<hash>/editing-audition.fcproj` through the installed FilmCraft CLI, then reopens it to verify track counts, timing, gain and duration. V1 contains individual prepared shot segments, V2 contains three rendered inserts, A1 is narration, A2 is music, A3 onward holds individual sound cues, and C1 contains editable draft captions. Overlapping sound tails get separate tracks so placing one effect cannot cut off another. The inserts' internal shapes and text are editable in Remotion via `pilot.json`; rerun the command after changes to create a new audition. The original source handles and any brief holds remain recorded in `prepared.json`. Existing audition directories are preserved; a matching verified project is reused.

`clef-packets.json` contains contextual choices with “none of these”, candidate/context hashes and criteria. `src/decisions.ts` validates returned distributions and reports provisional threshold eligibility. It **never applies** a choice: no Clef endpoint or calibration is assumed. Writing and rendering invoke no LLM. A live adapter and independently verified clip selection remain later work.

## Checks

```powershell
npm test
npm run check
python -m unittest discover -s test -p "test_*.py"
```

Review actual picture and sound before approving any treatment. Technical checks do not establish editing quality, evidence truth or audience response.
