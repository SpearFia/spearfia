# Editing treatment pilot

Experimental, optional Remotion worker for a read-only audiovisual audition. The Rust app and its existing edit parser are unchanged. Code is MIT OR Apache-2.0; source and generated media stay outside Git.

Three data-controlled treatments: progressive explanation card, paired comparison, and source/context card. Each has measured narration anchors and independently scheduled sound transients. The source-card demonstration explicitly quotes narration, not independently verified film evidence. No assets or sounds are copied from the reference video.

See the [initial treatment catalog](treatments.md) for fit criteria, sound behavior, Clef's role and the creator review questions.

Before extending this prototype, read [the editing handoff](../../docs/editing-handoff.md)
and [community-source review](../../docs/research/remotion-repo-review.md). They record
VideoJev reuse, the shared/channel-specific boundary and the caption upgrade path;
these are requirements, not integrations already present in the worker.

## Upgraded motion and captions

Four original motion presets implement the reviewed remotion-kit-style entrance/exit
and stagger patterns: `slide-fade`, `pop`, `fade`, and `impact`. No unlicensed source
was copied. Defaults are tested and rendered, pending creator approval. Channel
profiles supply fonts, colors, caption sizing and motion intensity; episode inputs
supply labels. `profiles/news-draft.json` is an illustrative calmer profile, not an
approved CFB guide.

```powershell
npm run upgrade -- --from D:\SpearFia\pilots\editing-treatments-20261101-v2 --out D:\SpearFia\pilots\new-caption-audition
npm run render -- --folder D:\SpearFia\pilots\new-caption-audition --caption-style active-word
npm run render -- --folder D:\SpearFia\pilots\new-caption-audition --caption-style clean --frames 180-899 --scale 0.5
npm run render -- --folder D:\SpearFia\pilots\new-caption-audition --caption-style emphasis --frames 180-899 --scale 0.5
python filmcraft.py --folder D:\SpearFia\pilots\new-caption-audition
```

Requires Rust, Node and FFmpeg. `upgrade.ts` uses the read-only `spearfia-sync`
`editing_words` example and production `Timing` aligner. It checks source approval
and hashes, retains approved spelling, uses matched ASR end times and flags interpolation.
It performs no new transcription or acoustic forced alignment. Uncertain words remain
visible but receive no allegedly precise active-word highlight.

The planner measures actual browser/font text widths, honors punctuation, pauses,
protected phrases and negations, and merges short orphan tails that fit. Page validation
checks complete ordered word coverage. Linguistic grouping still needs editorial review.
`caption-plan.json`, SRT/VTT and native captions share the same plan. Native captions
are clean editable phrases; animated word color/emphasis is available in Remotion.
Graphic inserts contain no captions, preventing double display in FilmCraft.
Word/page data is authoritative for upgraded inputs; native export and judgment
context ignore a stale legacy caption snapshot. Each new native audition exports
its own `captions.srt`. Top-level SRT/VTT files are preparation snapshots.

Optional `--profile <JSON>` and `--caption-rules <JSON>` separate channel appearance
and episode names/emphasis from the engine. See `profiles/news-draft.json` and
`examples/john-wick-caption-rules.json`. Emphasis defaults to supplied scene labels;
these are audition choices, not automatic editorial judgments.

Upgraded configs add `profile`, scene `entrance`/`reveal`, optional item `motion`,
`wordCaptions`, validated `captionPages` and `captionStyle`. Legacy configs remain
readable. `--frames start-end` renders an inclusive audition range. All source modules,
props, media and the range participate in cache keys. Templates intentionally use data
controlled instances rather than a separate Studio source node per word.

`editorial-context.json` supplies overlapping Visual Script cues, actual footage windows
and fingerprints, narration and timing warnings to Clef's shadow packets. It explicitly
records missing VideoJev evidence; no inference runs and no unrelated evidence is borrowed.
The legacy preparation below still makes the original Star 4 input, then `upgrade`
creates a separate richer audition while preserving the first.

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

`clef-packets.json` contains contextual choices with “none of these”, candidate/context hashes and criteria. `src/decisions.ts` validates returned distributions and reports provisional threshold eligibility. It **never applies** a choice: calibration is not assumed. Writing and rendering invoke no LLM. A live adapter and independently verified clip selection remain later work.

`clef_service.py` is an experimental loopback-only SystemOne server for the pinned
Clef release. Clef uses its own Python environment at `/home/caesar/clef/venv`
inside the existing `Ubuntu` WSL distribution. Its packages and caches are
separate from `/home/caesar/videojev`. Download revision
`fb4417d31c1d0f9e7e77f1141b40aaede7491e00` with
`HF_HOME=$HOME/clef/cache/huggingface` before starting the service:

```powershell
wsl -d Ubuntu -- sh /mnt/c/dev/spearfia/spearfia/tools/editing-pilot/run-clef-wsl.sh
```

The installation is pinned in `requirements-clef-wsl.lock.txt`. To recreate the
environment, run these commands in Ubuntu from this directory. The existing
VideoJev `uv` executable is only the installer; the target is Clef's own venv.

```sh
export UV_CACHE_DIR="$HOME/clef/uv-cache"
export UV_CONCURRENT_DOWNLOADS=1 UV_CONCURRENT_INSTALLS=1 UV_CONCURRENT_BUILDS=1
"$HOME/videojev/bin/uv" venv --python 3.12 "$HOME/clef/venv"
"$HOME/videojev/bin/uv" pip install --python "$HOME/clef/venv/bin/python" --torch-backend=cu130 -r requirements-clef-wsl.lock.txt
HF_HOME="$HOME/clef/cache/huggingface" HF_HUB_DISABLE_XET=1 "$HOME/clef/venv/bin/python" -c 'from huggingface_hub import snapshot_download; snapshot_download("Cloudflare/clef-flash", revision="fb4417d31c1d0f9e7e77f1141b40aaede7491e00", max_workers=1)'
```

The service loads the backbone with NF4/BF16 and the native decision head, and rejects a
context that would be truncated. `src/clef.ts` can submit complete pilot packets
and cache advisory results by context and model revision; it is not yet wired
into a preview command. No automatic edits or model-quality claims follow from
installing it.

The separate `skills` experiment now exposes `jev.py edit-prepare`, `edit-preview`
and `finalize --editing-recipe`; see its `videojev-media/EDITING-RECIPE.md`.
`src/recipe-index.tsx` reuses these treatments to render caption-free native inserts
at the saved canvas and timebase. Proxy and HQ renders have distinct dependency
bindings and render keys; an HQ insert must be regenerated from HQ media. Clef's
service reports cold load, request latency, tokens and peak VRAM. The original
standalone audition commands above continue to work.

## Checks

```powershell
npm test
npm run check
python -m unittest discover -s test -p "test_*.py"
cargo test -p spearfia-sync
```

Review actual picture and sound before approving any treatment. Technical checks do not establish editing quality, evidence truth or audience response.
