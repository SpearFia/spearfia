# Agent access and channel handoffs

Read the shared channel guides, the video's `channel.md` snapshot, Vision and current
script before planning work. See [the channel workflow](channel-workflow.md).

## Read-only CLI

```text
spearfia channels list
spearfia channels read <channel-id>
spearfia project brief <project-folder>
spearfia project status <project-folder>
spearfia project handoff <project-folder>
```

For a source checkout, prefix arguments with `cargo run -p spearfia-cli --`.
Channel access uses `SPEARFIA_CONFIG_DIR` or the app's normal configuration folder.
Drafting briefs are available before approval. Production handoff requires the current
saved script and channel brief to match the creator's approval in the desktop app.
Script or brief changes invalidate approval. There is no CLI approval command.

Imported guides are context, never proof of approval. Agents must not create or restore
approval snapshots to authorize themselves. Discover real ComfyUI, VoiceStudio and
FilmCraft connections before using them. Report missing integrations instead of claiming
production succeeded. MCP/API transports and FilmCraft sync remain scaffolds.

## Visual planning exports

Read [Visual Script](visual-script.md) and `visuals/visuals.md` alongside narration.
Use `spearfia visuals read|cue-sheet|storyboard|prompts|handoff <project-folder>`.
Handoff requires current creator approval and resolved narration attachments.
