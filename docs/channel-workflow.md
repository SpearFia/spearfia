# Channel knowledge and video production

A **Channel** keeps the identity and accumulated knowledge of a YouTube channel across
videos. A **Vision** is a creative brief for a particular video: pacing, catalog weights,
writing rules and references. Use both: a channel can use different Visions for different
series or experiments without losing its lessons and recurring assets.

## Set up a channel

Open **Channels** on the home screen, enter a name, and choose **Create channel**. The app
creates ordinary Markdown under `<app-config>/channels/<channel-id>/`:

| Document | What to keep there |
| --- | --- |
| `channel.md` | Purpose, audience, channel promise and non-negotiables |
| `style.md` | Writing, storytelling, visuals, graphic language, pacing and sound |
| `lessons.md` | Dated review observations, evidence, experiments and changes to try |
| `staples.md` | Actual paths/catalog IDs for recurring music, SFX, graphics and motifs; usage and license/source |
| `tools.md` | ComfyUI workflows/models/input mappings, VoiceStudio engine/voice/settings, FilmCraft connection and output paths |
| `production.md` | The approval-first production sequence |

Edit and save guides inside the app or open the folder in your editor. **Import Markdown**
adds channel analyses and other notes without overwriting existing documents. Imported
names become lowercase IDs; rename a conflicting source before importing another version.
**Copy agent context** collects the saved guides for a drafting agent. A stale editor save
is rejected; copy the draft before reloading if another process edited the file.

Limits: 256 KiB per document, 32 documents and 1 MiB of combined document content per
channel, and 200 channels. `SPEARFIA_CONFIG_DIR` can relocate the app configuration and
channel library. Media stays in project or linked external folders, not in these guides.

Use **New video for channel** to open New Project with that channel selected.

## Choose a channel for a video

Select a Channel in **New Project → Basics**. The project gets a plain `channel.md` with
a snapshot of the channel's saved guides and the source folder path. Older projects can
attach a Channel through **Channel / handoff** in the project header.

Shared guide changes affect future snapshots. An existing video keeps its snapshot until
you explicitly choose **Replace / refresh from selected channel**. This keeps changes to
lessons or style from silently changing an in-progress video's production instructions.
The project remains readable without the original channel library.

## Draft, review, approve, hand off

1. Read the channel guides, the video's `channel.md`, its Vision and current script. If present, read `visuals/visuals.md` and use the [Visual Script exports](visual-script.md) to plan production; resolve stale narration attachments before generating.
2. Draft and revise the script. Use **Copy drafting brief** to pass context to an agent.
   This is planning only; it does not authorize media generation or timeline assembly.
3. Review the actual script in Spear Fia and let it save. Open **Channel / handoff** and
   choose **Approve saved script for production**. Unsaved editor text cannot be approved.
4. **Copy production handoff** now includes the approved script, channel snapshot, Vision,
   project paths and production instructions. An agent can also retrieve it through the CLI.
5. Editing the script or project channel brief invalidates that approval. Review and approve
   again. **Revoke approval** returns the project to drafting.

Approval compares exact saved content with `sync/approved-script.md` and
`sync/approved-channel.md`. These are app-maintained review records; agents must not create,
modify or restore them to grant themselves approval. This is a local workflow gate, not
authentication against a process that can modify arbitrary project files.

## Agent production sequence after approval

- Recheck approval before expensive jobs. Plan assets by script section/line and reuse the
  channel's recorded staples before generating replacements.
- Discover the available ComfyUI connection and workflows. Generate needed visuals,
  graphics and music with the channel's recorded model/settings, prompts and seeds.
- Discover the installed VoiceStudio app's API/MCP connection. Generate narration with
  the channel's configured engine and saved voice/profile. Record settings and output paths.
- Verify finished outputs, including content, duration, dimensions, audio quality and
  pronunciation. A queued job is not a finished asset. Keep failures and missing inputs explicit.
- Put outputs in project or linked media folders and link them to the correct words, lines
  or sections. Choose transitions, effects, music and SFX only where useful; omitting them
  is a valid editorial choice.
- Use a verified FilmCraft connection to build the review timeline. Report its actual
  project/sequence and outstanding issues to the creator. Stop before publishing.
- After review, add dated observations with episode references and evidence to shared
  `lessons.md`; keep experiments distinct from channel rules.

## What runs today

Channel creation/editing/import, project snapshots, approval checks, clipboard handoffs
and the read-only CLI commands in [agents.md](agents.md) are implemented. ComfyUI and
VoiceStudio jobs are performed through separately available agent/tool connections; this
feature does not launch them. Spear Fia's MCP/HTTP servers provide Script and Visual Script editing; FilmCraft timeline
sync remains a scaffold. A handoff must report a missing integration rather than claim production or a
FilmCraft push succeeded.
