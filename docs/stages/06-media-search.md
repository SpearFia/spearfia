# Stage 6: Media search

**Run with:** any model with vision at medium effort (Codex: `-c model_reasoning_effort="medium"`). The script talks to Spear Fia itself.

````text
Use the media-search skill on the current Spear Fia project (MCP server "spearfia").

Read docs/editing-handoff.md first. If the project has an applicable approved
script-first/VideoJev interpretation, preserve its specific/general distinction and
use that workflow instead of the legacy auto loop below. Validate existing approval
receipts and source hashes; never grant approval yourself. Reuse saved per-question
results and source/time provenance. Query-free general footage stays query-free.
Keep intended visuals separate from scored/confirmed observations and unresolved
requirements so the editing stage can judge the actual selected footage.

Otherwise run the auto loop from the skill until every LIVE cue is linked:
  python <skill>/scripts/pool.py auto --project <project folder>
  look at each contact sheet once and note bad clip numbers
  python <skill>/scripts/pool.py commit --project <project folder> --drop BATCH:N,N
The script links, reuses library clips and writes production/media-report.md; don't call
links_add/links_list yourself. Follow the channel's media rules in production.md.

Sources: pool.py searches the trusted channels in the library's channels.json first and
never uses a watermark channel. On each contact sheet, if every clip is clean add
`--trust BATCH` to the commit; if any frame shows a channel logo or watermark, use
`--watermark BATCH` instead (rejects the source and blocks the channel for good).

Report the final line of `pool.py status`, sources rejected, and anything left unfilled.
````
