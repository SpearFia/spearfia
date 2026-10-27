# Stage prompts

One ready-to-paste prompt per pipeline stage (see [pipeline.md](../pipeline.md)). Each
agent starts cold: the prompt tells it where the project is, what to read, what to do and
when to stop. The Spear Fia project is the handoff between stages, so no prompt repeats
earlier work.

| # | Stage | Prompt | Agent |
| --- | --- | --- | --- |
| 1 | Channel DNA | [01-channel-dna.md](01-channel-dna.md) | Claude (Opus) |
| 2 | Ideas | [02-ideas.md](02-ideas.md) | Gemini (no MCP; uses the CLI) |
| 3 | Script | [03-script.md](03-script.md) | Claude or Codex |
| 4 | Visual Script and thumbnail concept | [04-visual-script.md](04-visual-script.md) | Claude or Codex |
| 5 | Voiceover linking | [05-voiceover.md](05-voiceover.md) | Codex |
| 6 | Media search | [06-media-search.md](06-media-search.md) | Codex, Tera xhigh |
| 7 | Assembly | [07-assembly.md](07-assembly.md) | Codex |
| 8 | Thumbnail and description | [08-thumbnail.md](08-thumbnail.md) | Claude or Codex |

Your gates: pick the idea after stage 2, approve the saved script in the desktop after
stage 3, review the FilmCraft timeline after stage 7.

## Setup (once)

- **Skills**: `channel-dna`, `media-search` and `learn-channel-scripts` are junctions
  from the skills repo into `~/.claude/skills` and `~/.codex/skills`. Both Codex accounts
  share `~/.codex/skills` and `~/.codex/config.toml`.
- **Spear Fia MCP** in `~/.codex/config.toml` (both accounts):

  ```toml
  [mcp_servers.spearfia]
  command = 'C:\dev\spearfia\spearfia\target\release\spearfia.exe'
  args = ["mcp", "--project", 'D:\SpearFia\current']
  ```

  For Claude Code, add the same server with
  `claude mcp add spearfia -- C:\dev\spearfia\spearfia\target\release\spearfia.exe mcp --project D:\SpearFia\current`.
- Rebuild the binary after pulling: `cargo build --release -p spearfia-cli`.

## Switch the current video

Every MCP config points at `D:\SpearFia\current`, a junction to the video you're working
on. Create the project in the desktop app (New Project, channel selected), then point the
junction at it, and restart the agents so their MCP servers reopen:

```powershell
$episode = 'D:\SpearFia\projects\cwi-001-harry-potter-vs-thanos'
if (Test-Path 'D:\SpearFia\current') { (Get-Item 'D:\SpearFia\current').Delete() }
New-Item -ItemType Junction -Path 'D:\SpearFia\current' -Target $episode
```

`Delete()` removes only the junction, never the folder it points to.

## Using a prompt

Paste the block from the stage file into the agent, filling the `<…>` parts. Each prompt
ends with a report; read it before starting the next stage. If an agent says it's blocked
(no approval, missing tool, unfilled cues), fix that first rather than pushing on.
