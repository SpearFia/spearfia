# Production pipeline

The end-to-end plan Spear Fia serves: a mostly automated YouTube workflow in which
different AI agents handle each stage and the creator reviews at fixed gates. Spear Fia is
the shared memory between agents (channel guides, script, Visual Script, media links) and
the place the creator steps in to review. This page says what each stage does, what Spear
Fia provides for it today, and what is still missing.

The creator's working plan lives outside this repo (an Obsidian canvas). This page is the
version agents should follow; update it when the plan changes.

## Stages

| # | Stage | Output | Spear Fia support | Status |
| --- | --- | --- | --- | --- |
| 1 | Channel DNA | Channel guides for a reference channel | Channel library (`channels/<id>/*.md`); the `channel-dna` skill writes into it | Skill drafted |
| 2 | Ideas and ranking | Ranked ideas with title and thumbnail concepts | `ideas.md` and `packaging.md` in the channel guides; `channel_read` | Guides only |
| 3 | Script | `script/script.md` | Script editor; MCP `document_read`/`document_edit`; creator approval | Built |
| 4 | Visual Script | `visuals/visuals.md` cues tied to narration | Visual Script editor; MCP `visuals_edit`; exports | Built |
| 5 | Thumbnail concept | A text description, no image yet | `packaging.md` | Guides only |
| 6 | Voiceover | One audio file per script part, linked to its words | MCP `media_list`, `links_add` (voiceover folders link as audio) | Linking built; generation external |
| 7 | Media search | Footage and images per cue, linked to words | MCP `links_add`; Visual Script references | Linking built; search skill missing |
| 8 | Assembly | A FilmCraft review timeline | `spearfia-sync`, `spearfia-adapter-filmcraft` | Missing (scaffold) |
| 9 | Channel-style edit | Music, SFX, transitions, captions, camera moves | Catalog recipes and looks (planned) | Missing |
| 10 | Thumbnail and description | Finished thumbnail and description | None (PhotoCraft or ComfyUI) | External |
| 11 | Review and lessons | Creator review; dated notes in `lessons.md` | Channel guides | Manual |

Stages 1–5 are planning. Stages 6–10 are production and need the creator's approval of
the saved script first (see [channel workflow](channel-workflow.md)). Spear Fia never
publishes.

## Creator gates

1. Pick the idea (after stage 2).
2. Approve the saved script in the desktop (after stage 3). Production handoffs refuse
   unapproved or changed scripts. Agents can never grant approval.
3. Review the rough cut in FilmCraft (after stage 9), before thumbnail and upload.

## Stage notes

### 1. Channel DNA

- Skill: `channel-dna` in the [skills](https://github.com/spearfia/skills) repo
  (`channel-dna/SKILL.md`, with `references/dna-checklist.md` and
  `references/channel-docs.md`). For script depth it uses the separate
  `learn-channel-scripts` skill when available.
- Steps: collect with `yt-dlp`/`ffmpeg` (no model) → analyze one dimension at a time
  (cheaper model) → synthesize rules (strongest model) → write into the channel folder →
  hand-test.
- Research evidence, transcripts and frames stay in a study folder **outside** the channel
  library. Every channel guide is copied into every video snapshot and agent brief, so the
  guides hold compact rules only (about 40 KB per channel).
- DNA writes the standard documents (`channel.md`, `style.md`, `staples.md`, `tools.md`,
  `production.md`, `lessons.md`) and adds `ideas.md` and `packaging.md`.

### 2. Ideas and ranking

- `ideas.md` holds the channel's idea prompt, a list of topics already covered, and a
  ranking rubric: fit, outlier potential, novelty, feasibility (can we get the visuals and
  facts?) and packaging strength.
- Rank by packaging: each idea comes with a working title and a one-line thumbnail
  concept.
- Some channels need a research stage first (for example, a weekly college football news
  channel). Each channel records extra stages in its `production.md`.

### 3–4. Script and Visual Script

- Built. See [agents.md](agents.md) for the read → edit → verify loop and
  [visual-script.md](visual-script.md) for the cue format.
- Consider generating several hooks rather than several full scripts, and judging them
  against the DNA rubric.

### 6. Voiceover

- Generated with VoiceStudio (an external open-source app) using the voice recorded in the
  channel's `tools.md`.
- One file per script part. Use `##` sections or paragraphs as parts and name files after
  them, so linking and timing are mechanical.
- Link each file to its words with `links_add`. Files in a voiceover media folder link as
  audio.
- Gap: Visual Script timing is still estimated at 150 wpm. Real VO durations should
  replace the estimate.

### 7. Media search

- Needs a free, reliable media search skill. Suggested source order: the channel's
  staples → free stock (Pexels, Pixabay, Wikimedia Commons, Internet Archive) → `yt-dlp`
  sources → ComfyUI generation as the fallback.
- Keep a log of used clips to avoid repeats, and tag downloads so the library grows across
  videos. Exact shot matching is not required; footage that fits the topic is fine.
- Risk: film, TV and sports footage draws Content ID claims, and fully automated channels
  face YouTube's reused-content monetization policy. Prefer generated graphics, Remotion
  and licensed stock where possible.

### 8–9. Assembly and channel-style edit

- Assembly compiles links and script order into a `timeline-plan` and applies it through
  FilmCraft's MCP. VO sets the timing; picture links fill the same spans. Open questions
  are in [filmcraft-recon.md](filmcraft-recon.md).
- Editing style from the DNA should become recipes or looks the editing agent applies,
  not prose it interprets.
- Sound effects may come from ComfyUI with Stable Audio.

## Agent assignments

The creator's current choice, by stage. Any capable agent can run any stage through
Spear Fia's MCP/HTTP tools; these are preferences, not requirements.

| Stage | Agent |
| --- | --- |
| Channel DNA | Opus 5.5 (synthesis) with a cheaper model for collection and analysis |
| Ideas | Gemini 3 Pro |
| Script, Visual Script, thumbnail concept | Sonnet 5.5 |
| Voiceover and media search | Codex (Sol 6.1) |
| Assembly and style edit | Codex (Sol 6.1) with FilmCraft MCP |
| Thumbnail and description | Sonnet 5.5 with PhotoCraft and ComfyUI |

## What's missing, in order

1. Measured VO timing from linked audio files.
2. FilmCraft push (`spearfia-sync`, `spearfia-adapter-filmcraft`).
3. The media search skill.
4. Editing style as catalog recipes.
5. A way to update this process over time, possibly an outside reviewer agent that checks
   each stage's output against the channel DNA.
