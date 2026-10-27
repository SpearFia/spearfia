# 6. Coordinate production through installed CLI agents

- Status: proposed
- Date: 2026-10-27
- License: MIT OR Apache-2.0

## Context

The pipeline currently has ready-to-paste prompts and shared project tools, but the
creator manually starts each agent and carries reports between stages. Existing skills
already operate the production services. We need saved assignments and recoverable runs
without duplicating those skills or implementing another project-editing path.

## Decision

Use the existing `spearfia-ai` crate for noninteractive Claude Code/Codex adapters.
Place the sequential coordinator in `spearfia-ops::production`, shared by a new desktop
Production page and `spearfia pipeline`. Attach the current project's MCP connection per
invocation, preserve other agent configuration, and launch native executables with argv
and piped stdin rather than evaluating shell command strings. Model/effort strings are
explicit per-stage settings; blank settings inherit the installed CLI's configuration.

Define pipeline, run and final-report formats in `spearfia-spec` first. Configuration
changes use the existing plan/apply/history system. Agents cannot undo configuration
changes. Runtime checkpoints and attempt logs are separate atomic coordination metadata.
A project-wide run lock prevents concurrent coordinators; edit locks cover short saves.

Require both a successful provider result and independently checked saved outputs.
Pause at creator reviews or agent questions. Production checks existing script/channel
approval before launching, during execution, and before accepting completed work.
Review acknowledgement never approves a script, and no agent tool exposes these controls.

On continuation, retain completed valid stages, invalidate changed artifacts and later
stages, and require explicit retry for failures and interrupted running checkpoints.
Retries launch fresh sessions with previous results and replies. Stop/timeouts terminate
the CLI tree; Windows also uses kill-on-close job objects for crash cleanup.

## Consequences

Creators can configure, run, inspect and resume the workflow from either surface. They
still need installed/authenticated CLIs, skills and available production services. Output
checks establish structural readiness, with editorial review remaining explicit. Jobs
submitted to independent services may outlive the CLI and need reconciliation on retry.
This is stage recovery rather than a transactional external job scheduler. Provider APIs,
captions, media analyzers, editing recipes and FilmCraft pull remain separate features.
