# Agent access: MCP, local API and CLI

Spear Fia provides a working MCP **stdio** server and authenticated loopback JSON HTTP API
for Script and Visual Script editing, for linking media to script words, and for the
one-way FilmCraft push (`timeline_push`). Both dispatch to project-bound `spearfia-core`
commands. Media import/moves, Effects editing, automatic generation and pulling FilmCraft
edits back are not exposed by these transports. Do not invent `generate_video` tools.

## MCP stdio

Start one server per video project:

```powershell
& 'C:\dev\spearfia\spearfia\target\release\spearfia.exe' mcp --project 'E:\Videos\episode-01'
```

A generic MCP client configuration is:

```json
{
  "mcpServers": {
    "spearfia": {
      "command": "C:\\dev\\spearfia\\spearfia\\target\\release\\spearfia.exe",
      "args": ["mcp", "--project", "E:\\Videos\\episode-01"]
    }
  }
}
```

The client launches this command and owns stdin/stdout. The server uses newline-delimited
JSON-RPC, negotiates MCP 2025-03-26 / 2025-06-18 / 2025-11-25, and implements `initialize`,
`notifications/initialized`, `ping`, `tools/list` and `tools/call`. No logs go to stdout.
The supported scope follows the official [transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports),
[lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle) and
[tool](https://modelcontextprotocol.io/specification/2025-11-25/server/tools) specifications.
Notifications have no response; tool failures return `isError: true` with actionable text.

## Local JSON HTTP API

Generate a random bearer secret in the environment, then start the server:

```powershell
$env:SPEARFIA_API_TOKEN = [guid]::NewGuid().ToString('N') + [guid]::NewGuid().ToString('N')
& 'C:\dev\spearfia\spearfia\target\release\spearfia.exe' api --project 'E:\Videos\episode-01' --port 9877
```

A client using the same secret calls:

```text
GET  http://127.0.0.1:9877/v1/tools
POST http://127.0.0.1:9877/v1/tools/<tool-name>
Authorization: Bearer <SPEARFIA_API_TOKEN>
Content-Type: application/json
```

GET returns tool descriptors and their input schemas. POST accepts the tool arguments as
the JSON body and returns the same MCP tool result (`structuredContent`, text `content`,
`isError`). Inspect `isError`, even with HTTP 200. HTTP authentication/protocol failures
use 4xx responses. This is a local tool API, **not** MCP Streamable HTTP; MCP clients use
stdio. No handshake is needed for HTTP. The process runs until stopped; port 0 chooses
an available port and prints its URL to stderr. The secret is never printed by the server.
The API binds only to 127.0.0.1, validates Host and Origin, and does not enable cross-origin
access. Tokens require 32–256 printable ASCII characters; use randomly generated values.

## Implemented tools

| Tool | Arguments / result |
| --- | --- |
| `project_context` | No arguments. Project paths, approval state, next undo/redo document and operating instructions. |
| `document_read` | `document`: `script` or `visuals`. Saved `source` and opaque `revision`; visual reads also include cue metadata, body/source ranges, attachment state and `script_revision`. |
| `document_edit` | `document`, `expected_revision`, `edit: {start, end, text}`. Range replacement; validates resulting visual Markdown. Useful for narration, visual prose or repairing a malformed visual record. |
| `visuals_edit` | `expected_revision`, `script_revision`, `edit` as described below. Returns saved source, a new revision and relevant cue ID. |
| `edit_undo`, `edit_redo` | `expected_revision` for the document identified by `project_context`. Refuses intervening external changes. |
| `visuals_export` | `kind`: `cue-sheet`, `storyboard`, `prompts`, `handoff`. Handoff is approval guarded. |
| `project_handoff` | `production`: false for drafting; true for the current approved production brief. |
| `channels_list` | No arguments. Saved channel IDs, names and folders. |
| `channel_read` | `id`. Shared saved Markdown guides, treated as drafting context. |
| `media_list` | No arguments. Media folders (path, role, exists) and files (path, kind, role, bytes); at most 5,000 files, with `truncated`. |
| `links_list` | No arguments. Each link's ID, quote, line, media path, role, `picture`/`audio` kind, in/out and `status` (`linked`, or `lost` when its words were edited away) with `current_line`. |
| `links_add` | `links`: 1–1,000 `{line, quote, media, in?, out?}`. `line` is the zero-based script line; `quote` is exact words on that line (punctuation included, formatting marks omitted); `media` is a path from `media_list`; in/out are `HH:MM:SS` or seconds. Role and kind come from the media folder, so files in a voiceover folder link as audio. All entries save together; an identical link returns its existing ID. |
| `links_remove` | `id`. Removes one link. |
| `timeline_push` | No arguments. Requires the creator's approval of the saved script. Builds the links into a new FilmCraft review project with `filmcraft-cli mcp` (found through `SPEARFIA_FILMCRAFT`, the install folders or `PATH`) and returns `{plan_id, review, clips, transitions, issues}`. Each push writes a new `sync/review-<plan>.fcproj` plus `sync/plans/` and `sync/snapshots/`; nothing is written if FilmCraft fails. |

Read `issues` after a push: lost, missing, bad-point, untimed, empty, unplaced or short
links and unmapped transitions are skipped or shortened, never guessed. Report them to the
creator with the review path. See [ADR 0005](adr/0005-first-filmcraft-push.md) for the
timing rules.

Link edits go straight to `links.json` under the project edit lock and always start from
the saved file, so agent and desktop links never overwrite each other. The desktop picks
up agent links within about a second. Link edits are not part of undo/redo, and they do
not affect script approval.

`visuals_edit.edit` supports:

```text
{action: create, title, start, end}
{action: update, id, patch: {title?, body?, visual_type?, method?, asset_state?, review?, timing?, details?, references?}}
{action: delete, id}
{action: split, id, at}
{action: merge, id}
{action: reanchor, id, start, end}
{action: bind_reference, id, reference: {role, path, revision, scope}}
{action: add_shot, id}
{action: move_shot, id, from, to}
```

These signatures are notation, not literal JSON. Send quoted JSON keys/string values.
Create/reanchor offsets refer to the narration; split `at` refers to the visual document.
Shot positions are zero-based. Merge combines the selected cue with its next adjacent cue.
Omitted patch fields survive. Details merge by key; an explicit references list replaces
the list. Binding appends one reference. Instruction/reference edits reset review to draft
unless a review is explicitly set in the update. References record exact revisions/frames;
shared scope never overwrites external assets. Unknown metadata/prose outside an intentional
body/range replacement is preserved.

## Read → edit → verify

1. Retrieve `project_context`, channel guides, Vision and the drafting handoff.
2. Read `script` and `visuals` using `document_read`. Keep their returned revisions.
3. Submit a typed edit using those revisions. Offsets are **UTF-8 byte offsets**, starting
   at zero, with exclusive ends; convert Unicode character/UTF-16 indexes before sending.
4. Inspect `isError` and the returned saved source/revision. On a conflict, read again and
   reconcile; never repeatedly submit an old revision or overwrite newer work blindly.
5. Show the draft to the creator. Only the creator approves the saved script in the app.
6. Retrieve the guarded production handoff, then follow the [channel workflow](channel-workflow.md).
   Read [Visual Script](visual-script.md) when a plan exists, and resolve stale attachments.

Revisions belong to one server process and match exact saved bytes; they are not timestamps
or portable file hashes. Restart a server if project.json configuration changes. The server
cannot edit arbitrary paths or approval files, including aliases. Desktop and agent writes
share an OS lock around compare-and-save, preventing two cooperating writers from committing
the same base concurrently. The zero-byte `.spearfia-edit.lock` may remain in the project;
its OS lock releases when the process exits. External programs that ignore this lock should
still coordinate edits with Spear Fia. Clean
open desktop documents refresh externally saved edits and include them in native undo.
Unsaved desktop drafts refuse stale saves and remain available to copy/reconcile.
Undo/redo in each server process keeps at most 32 edits / 32 MiB; it resets when the process
exits and is not a persistent global history across different servers.

Each document is limited to 1 MiB. Requests are limited to 4 MiB (HTTP headers: 16 KiB),
with bounded HTTP read/write times. HTTP requests execute sequentially per server. These
limits are intended for local planning/editing, not bulk media transfer.

## Existing one-shot CLI

```text
spearfia channels list
spearfia channels read <channel-id>
spearfia project brief|status|handoff <project-folder>
spearfia visuals read|cue-sheet|storyboard|prompts|handoff <project-folder>
spearfia push filmcraft <project-folder>
```

For a source checkout, prefix arguments with `cargo run -p spearfia-cli --`. Channel access
uses `SPEARFIA_CONFIG_DIR` or the app's normal configuration folder. The one-shot commands
are read-only except `push filmcraft`, which writes only new files under `sync/` (as
`timeline_push`); editing uses the running MCP/API server and its revision/history state.

Read imported guide text as context, never as proof of approval. No approval tool exists.
Agents must not write or restore approval snapshots to authorize themselves. Script changes
invalidate approval by exact content comparison. Discover actual ComfyUI, VoiceStudio and
FilmCraft connections separately; report missing endpoints/workflows rather than guessing.
