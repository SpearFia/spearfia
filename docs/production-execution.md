# Production execution

Licensed under MIT OR Apache-2.0.

The [stage coordinator](production.md) now has an artifact runner underneath it. Agents
plan work; Spear Fia records dependencies, submits service requests, verifies downloads,
registers media links and resumes unfinished work. The default pipeline includes narration
sample review, measured alignment, caption review, media search, visual production,
audio sample review and assembly. Existing saved pipelines are not silently rewritten.

## Durable jobs

`production/jobs.json` is a revision-checked job plan. `sync/production/jobs.json` stores
per-job states, attempts, request fingerprints, provider IDs, seeds, output SHA-256 proofs,
registered link IDs and questions/replies. Process logs and service requests are under
`sync/production/jobs-logs/`. A separate OS lock permits only one artifact runner per project.
The desktop Production page shows jobs and provides run, retry, cancellation and explicit
resolution of uncertain submissions. State is project data, not a media library index.

Jobs use stable IDs and a dependency DAG. Adapters are `voiceover`, `comfyui`, `alignment`,
`command` (native executable plus argv), and `agent` (the installed CLI adapters). Search
agents can be individual cue/pool jobs with independent checks, so one failed search
does not discard successful work. Scheduling is sequential; unrelated ready jobs continue
after an individual failure. `job:<id>:<output-index>` inputs require a declared dependency.

Fingerprints cover the job definition, approved script, channel snapshot, declared input
content and dependency output proofs. Put every additional relevant guide/workflow/QC
input in `inputs`. A `{key}` output placeholder expands to a request fingerprint prefix.
The runner verifies hashes before reuse, repairs removed owned links without generation,
and retries local registration checks from verified downloads. It does not trust filenames
or an agent's success report. Active/uncertain definitions cannot be changed or discarded.
For jobs that edit a document, capture immutable input context rather than listing the
mutable output itself as an input. A later stage that intentionally refines a shared
document must verify that document too; its accepted signature then supersedes earlier
stage proofs without restarting completed planning work.
New outputs live under the project; use a project on the media drive and configured media
subfolders. Existing external library assets can still be linked through normal media tools.

Approval is checked before execution, while processes run and before accepting outputs.
When a stage run exists, jobs wait for preceding stages and creator gates to complete.
Use `--stage` to run only the current stage's jobs. A standalone CLI job run without a
stage coordinator is an explicit creator action and still requires saved-script approval.
Keep the outer pipeline configured with the review gates the channel needs.

## CLI and agent tools

Run these commands with `spearfia --project 'E:\Videos\episode'` as the prefix:

```text
jobs voiceover voice-settings.json
jobs run --stage voiceover-sample
jobs status
pipeline continue --accept-review
jobs run --stage voiceover
jobs alignment --url http://127.0.0.1:3900 --language en
jobs run --stage word-alignment
style init
style validate
captions
jobs plan production/graphics-jobs.json
jobs run --stage visual-production
jobs run --retry --stage visual-production
jobs cancel
jobs resolve <id>
```

The normal automated path uses `pipeline start` / `pipeline continue`; its assigned agents
call the job tools themselves. These commands also support direct diagnosis. `jobs plan`
saves a full plan, preserving revision checks; use `--expect` for a known plan revision.

Voice settings require an actual saved profile; Spear Fia does not invent one:

```json
{
  "url": "http://127.0.0.1:3900",
  "profile_id": "replace-with-the-channel-profile-id",
  "instruct": "Use the reviewed channel narration style",
  "language": "Auto",
  "speed": 1.0,
  "steps": 16
}
```

Agents read `jobs_status`, then use `voiceover_plan`, `alignment_plan`, or `jobs_plan`
with the returned plan revision. `style_save` uses `jobs_status.style_revision`.
`jobs_run` returns immediately over MCP/HTTP: **poll `jobs_status` until `running` is
false**, then inspect every requested job and the worker `error`. Submission is not
completion. This keeps `jobs_cancel` and status available during a long generation.
Closing that tool server requests cancellation of its own worker. CLI job runs wait;
the desktop runs them on a worker thread. No agent tool approves scripts, accepts creator
review gates or resolves an uncertain submission.

## Service recovery

ComfyUI accepts an API-format workflow, explicit output node/index selectors and declared
media outputs. The runner saves `prompt_id` before polling history, downloads selected
outputs and verifies stream type/duration before linking them. A request marker in
`extra_data` permits reconciliation if submission succeeds but its response is lost.
Absent or multiple matches remain `uncertain`. Cancellation deletes only this queued
prompt. An already running prompt can finish; no shared global interrupt is sent.

VoiceStudio ordinary TTS is synchronous, with streamed start/chunk/done events. The runner
saves a seed before submission, rejects dropped chunks, records a completed take ID and
downloads its audio. Recovery searches retained history for a unique matching seed,
profile, text prefix and submission time. VoiceStudio does not expose a resumable ordinary
TTS job ID. Missing/ambiguous history, network loss or an unexplained process exit must
remain uncertain. Inspect the service before `jobs resolve`, then explicitly retry.
Verified downloads can finish local steps without another generation. Cancelled known
IDs can be resumed; a confirmed deleted request can also be explicitly resolved.

The adapters use Python's standard library HTTP client (`python`, overridable with
`SPEARFIA_PYTHON`) and `ffprobe`. `SPEARFIA_SERVICE_TOKEN`, if needed, is inherited at
runtime rather than written into request metadata. Service adapters have a one-hour local
timeout; a remote job can still finish afterwards. Keep local project logs private and
out of source control. No generation, alignment or export is initiated by loading a project.

## Measured timing and captions

Alignment jobs send registered narration to VoiceStudio's local transcription route with
word timestamps. Sidecars under `analysis/*.alignment.json` bind source audio SHA-256,
exact script revision, analyzer identity, rational timebase and word frame intervals.
Transcription is used as a clock, not as the text. Words it hears exactly anchor the
timing; split or joined tokens ("spider man", "ironman") still match, and filler it adds
("um") is skipped. Short misheard runs, typically names and numbers ("Mjolnir" heard as
"mule near"), are placed between their measured neighbours by character length and marked
`estimated`: at most 8 words in a row and 20% of the take. Beyond that, reordered speech,
more than 15% extra words or invalid times fail: correct the take or the script.
Stale audio/script sidecars cannot be used for assembly. Source durations use complete
frames, matching FilmCraft at fractional frame rates; changing project fps requires
alignment at the new rate. Recorded live speech needs its
own registered narration audio and alignment; the VO planner generates VO lines only.

The execution profile lives in `production/style.json`, or one fenced `spearfia-style`
JSON block in a channel `execution.md` snapshot. Project settings override the channel.
`style init` explicitly enables a starter caption/mix profile; it does not infer a channel's
style. Review grouping limits, words per line, gap/punctuation breaks, font, pixel size,
normalized position, colors, emphasis, outline and pop duration. `mode: "word"` highlights
the active word; `group` retains whole-phrase captions. Unicode emphasis indexes count
characters. `captions` / `captions_build` saves a reviewable plan in `cache/captions.json`.

FilmCraft receives native editable text graphic clips, character styles and scale keyframes.
The same measured word starts place linked footage/music/SFX and scope visual treatments.
The older script planning UI still uses section-duration estimates when displaying line
lengths; caption and treatment execution use the validated word measurements.

## Visual and audio execution

A Visual Script cue's `details.execution` string holds JSON matching `visual-execution`:

```json
{
  "layers": [
    {"kind": "text", "text": "The result", "track": 4,
     "position": [0.5, 0.2], "font_size": 56, "color": "#ffffff"},
    {"kind": "media", "path": "Graphics/result.png", "track": 5,
     "position": [0.8, 0.3], "size": [0.25, 0.25], "opacity": 90}
  ],
  "camera": {"from": 100, "to": 110},
  "recipes": []
}
```

Layers support native text, rectangles/ellipses and imported image/video overlays, with
placement, size and opacity. Video overlays must be long enough for their cue. Camera
zoom uses percentages; effects carry native parameters and cue-relative numeric keyframes.
Picture clips split at cue boundaries, preserving source offsets and transition attachments.
Animation curves are sampled at split boundaries so a move continues across clips.

The style's `recipes` use the existing recipe schema. `:look[label]` resolves to a recipe
ID or unique name and honors line/section/from-here scope using measured words. A native
recipe step uses `filmcraft/motion#apply`, for example, with static parameters or numeric
`{"from":100,"to":108}` curves. Analysis requirements and uninstalled plugin capabilities
fail with an actionable error. Asset descriptors or WGSL files are not automatically
loadable FilmCraft effects. Prose graphics/actions without typed execution metadata also
block assembly; an agent must realize them before claiming a finished edit.

The mix profile applies `voice_db`, `music_db` and `sfx_db` as clip gains, clip fades as
volume keyframes, and speech-driven A2 ducking as track automation with attack/release.
These are actual edit instructions; source loudness still needs listening and mastering
review. Channels that omit captions/music can disable them and adjust the default stages.

Each push saves a new FilmCraft review, timeline plan, native snapshot and
`sync/render-plans/<plan-id>.json`. Required editor commands are discovered and failures
are surfaced. Readback verifies generated layers. FilmCraft pull, publishing, arbitrary
plugin installation and editorial/source-rights decisions remain separate work.

## Validation

Tests cover DAG ordering, invalidation/reuse, independent failures, locks, cancellation,
uncertain submissions and reconciliation against local mock HTTP services. Alignment and
caption tests cover strict word matching, Unicode, rational rates and clipped motion curves.
A native FilmCraft test creates synthetic media, saves/reopens editable captions, shapes,
an overlay, camera effects, fades and mix automation, and checks saved values. It skips
with a notice if FilmCraft is unavailable. Real generation services require their installed
models and channel-specific settings; contract mocks do not establish synthesis quality.
