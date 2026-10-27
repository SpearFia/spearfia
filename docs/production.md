# Production coordinator

Licensed under MIT OR Apache-2.0.

The **Production** page runs the saved stages using installed Claude Code and Codex
CLIs. It uses their existing login, skills and tool connections. No API key is stored
by Spear Fia. This is a local, sequential workflow; running a stage can invoke paid
models or generation tools configured in those CLIs.

## Setup and use

1. Build both applications: `cargo build --workspace` (or `--release`). The `spearfia`
   CLI must sit beside `spearfia-desktop` so the coordinator can attach its project tools.
2. Install and log in to the agent CLIs. Install the channel-dna, voiceover, media-search
   and audio-gen skills as described in [stage setup](stages/README.md). Start VoiceStudio
   or ComfyUI before their stages. Other configured MCP servers and CLI settings remain
   available. The coordinator overrides only the `spearfia` connection for this project;
   switching the shared `current` junction is unnecessary for coordinated runs.
3. Open a project with an attached channel brief. In **Production**, choose the provider,
   exact model ID and effort for each stage. Blank model/effort fields use the CLI's saved
   defaults; model availability is checked by the CLI, without silently substituting one.
   For example, assign your Opus configuration to Channel DNA, Sonnet to writing, and
   your chosen Codex configuration to media and narration. Set actual IDs accepted by your
   installed CLI. You can remove optional stages, reorder stages, adjust instructions,
   output paths, review gates and timeouts. Keep planning stages before production.
4. **Save assignments**, then **Start new run**. The default stages include channel DNA,
   ideas, narration, Visual Script, VO sample/rest, media search, audio sample/rest,
   FilmCraft assembly and packaging. Channel DNA reports proposed snapshot refreshes;
   adopt changed guides through **Channel / handoff** before approving production.
5. At a review gate, inspect the outputs, enter a selection or feedback, and choose
   **Accept review and continue**. Script approval is a separate explicit creator action
   in the desktop. A review acknowledgement does not grant it. Missing approval pauses
   production before any CLI launches.
6. An agent question pauses the same stage. Supply a reply to continue. Failures and
   interrupted attempts require an explicit retry after inspecting existing work and logs.
   Each attempt starts a fresh CLI session with earlier reports, log paths and replies;
   it is instructed to reuse completed work and reconcile external generator jobs.

Claude Code uses `--print`, structured streaming output, `--permission-prompts none`,
`acceptEdits`, and an allowlist for Read/Glob/Grep/Skill/Bash and the project MCP tools.
Existing CLI deny rules still apply. Tool denials pause the stage instead of accepting
its completion. This requires a version supporting `--permission-prompts` (v2.1.259+).
Codex uses `exec --json`, `workspace-write`, approval policy `never`, a required
project-specific MCP server, and a structured final response. Both preserve other CLI
configuration. No sandbox-bypass flags are used. Native executable overrides are
available; Windows npm Codex installations are discovered without evaluating `.cmd`
or PowerShell shims. Child windows are hidden. See the official
[Codex noninteractive guide](https://developers.openai.com/codex/noninteractive) and
[Claude Code programmatic guide](https://code.claude.com/docs/en/headless).

## Saved state and checks

- `production/pipeline.json`: editable stage definitions, saved through the shared command
  layer with revision checks and undo/redo. Schemas originate in `spearfia-spec`.
  Agent undo/redo cannot cross a creator assignment change; the desktop/CLI can.
- `sync/production/run.json`: atomic checkpoint with stage status, attempt count, replies,
  reports, verified artifact signatures and log directories.
- `sync/production/logs/<stage>-<attempt>-<time>/`: prompt, stdout events, stderr, schema,
  structured answer and normalized report. Logs are local project data; do not commit
  them or share them without reviewing their contents.
- `sync/production/archive/`: previous checkpoints when explicitly starting a new run.

A successful CLI exit and a `completed` report are both required. Saved output checks
then verify nonempty files, actual spoken script text, resolved visual cues, existing
narration-linked media, measured VO coverage, or a new FilmCraft review with its matching
nonempty plan and valid snapshot. Adding later media links does not invalidate earlier
sample stages. Existing reviews cannot satisfy a new assembly attempt. Structural checks
do not certify pronunciation, visual relevance, thumbnail quality or complete picture
coverage; stage reports and creator reviews remain necessary. Adjust checks/instructions
to the channel's needs, including channels that intentionally omit music or SFX.

One coordinator holds the project's OS run lock. Document editing uses the separate
short-lived project edit lock. Production approval is rechecked before a stage, every
second while it runs, and before accepting its completed outputs. Changed configuration
requires a new run. Changed/missing verified artifacts invalidate the affected stage and
its successors; retry is explicit. Completed valid stages are skipped on continuation.

**Stop run** terminates the CLI process tree and saves a recoverable cancellation. Windows
job objects also terminate descendants if the coordinator exits unexpectedly. Other
platforms use process groups during normal cancellation. An interrupted checkpoint is
marked failed on continuation; inspect external generators before retrying. Jobs already
submitted to an independent service can continue after its CLI exits. Closing the project
requests a stop. A coordinator started in another CLI can be monitored/stopped from the
desktop. A dead coordinator's stored `running` state does not disable recovery controls.

## CLI

```powershell
spearfia --project 'E:\Videos\episode' pipeline init
spearfia --project 'E:\Videos\episode' pipeline show
spearfia --project 'E:\Videos\episode' pipeline start
spearfia --project 'E:\Videos\episode' pipeline status
spearfia --project 'E:\Videos\episode' pipeline continue --accept-review --reply 'Use idea 2'
spearfia --project 'E:\Videos\episode' pipeline continue --reply 'Use the saved voice'
spearfia --project 'E:\Videos\episode' pipeline continue --retry
spearfia --project 'E:\Videos\episode' pipeline stop
```

`init` refuses to overwrite an existing config. Configuration can also be edited as JSON;
the next continuation checks its content revision. Production controls and review
acknowledgements are intentionally absent from the agent MCP/HTTP tool list. Approval
remains a local creator workflow gate, not protection against arbitrary filesystem writes.
