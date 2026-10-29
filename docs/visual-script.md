# Visual Script

Open a project and choose **Visual Script**, immediately after Script. The optional plan
is stored as `visuals/visuals.md`. Existing projects need no migration. The narration script
remains authoritative; visual prose never adds spoken duration.

## Writing and linking

Select a narration passage in Script and choose **Plan visuals for selected narration**,
or select lines in the Visual Script narration pane (**Shift-click** extends across lines)
and choose **New cue from selection**. Each cue has an editable heading, stable internal ID
and an exact narration snapshot with its VO/LIVE label. Clicking the excerpt opens the
original passage in Script. LIVE passages offer keep-on-camera, overlay and cutaway choices.

Write naturally on the reused editor page. Paged/pageless/Markdown views, formatting,
find/replace, undo/redo, comments, suggested replacements and supporting document tabs
remain available. Visual paragraph types appear in the style menu and `/` insertion menu:
Visual direction, Action / reveal, On-screen text, Graphic specification, Production prompt,
Continuity / constraint, and Production note. The saved prefixes are `VISUAL:`, `ACTION:`,
`TEXT:`, `GRAPHIC:`, `PROMPT:`, `CONTINUITY:` and `PRODUCTION NOTE:`. All prose in this editor
is non-spoken, including text without a prefix. Timing labels derive from linked narration:
recorded VO lengths once VO files are linked, otherwise a 150 wpm estimate.

## Cue details and feedback

Select a cue in the left panel. Visual type (chart, character scene, overlay, hold, etc.)
and production method (reuse, LIVE footage, generated art/video, animation, Remotion,
manual) are separate. Chart details include data, units, population, sources, approximation,
axes and reveal order. Animation details cover rig/revision, characters, views, actions,
props, contacts, parameters and missing preparation. Generation details record actual
workflow/model/input mappings. Reference roles separate content, identity, style and
composition; references can include exact asset revisions, shots or frames.

Coverage counts narration words with a resolved visual treatment, including intentional
holds and LIVE footage. Asset readiness and review state are separate manual assessments.
A completed generation job does not imply accepted footage. Navigation finds the next
unresolved cue or unplanned narration. Missing local reference files are shown as issues.
Shared reference scope records intent and shows affected cues; it never overwrites an
external asset or template. Edit those in their own authoring tool.

## Revisions and safety

Exact unique narration passages survive movement. Repeated passages use saved surrounding
context; ambiguous, changed or deleted attachments remain visible with the original passage
and a candidate current passage. The candidate is only a line hint. Select the intended
narration and explicitly reattach it; this resets review to draft. Instructions/references
remain intact. Split at the document caret, merge adjacent cues, and add/reorder `### Shot`
blocks. Splits create a new ID; merges preserve the first ID and retain the other metadata.

Markdown is authoritative. Cue metadata is a single-line `<!-- visual-cue {JSON} -->`
record (hidden in document view, editable in Markdown). Its format is specified in the
[source specification](https://github.com/SpearFia/spearfia-spec/blob/90d4b384c16883d8a373c1f7516cd8f78b61c7ee/docs/visual-script.md) and its `visual-cue.schema.json`. Unknown
prose and metadata fields survive edits to other records. Invalid records produce an error
and remain editable. Limits: 1 MiB/document, 1000 cues, 128 references/cue, 64 KiB/anchor.
Clean documents reload external changes; stale saves are refused so another writer's
changes cannot be silently overwritten. Copy a conflicting draft before reloading.

## Exports and agent access

Copy or save a cue sheet, text storyboard, prompt pack or production handoff from the same
plan. The storyboard is text; image rendering is not included. Prompt packs include prompt,
continuity, exact on-screen text and graphic instructions plus details/references.

```text
spearfia visuals read <project-folder>
spearfia visuals cue-sheet <project-folder>
spearfia visuals storyboard <project-folder>
spearfia visuals prompts <project-folder>
spearfia visuals handoff <project-folder>
```

The handoff requires the current creator-approved saved narration/channel brief and resolved
visual attachments. It includes production context and never authorizes publishing. Other
exports are planning artifacts and include attachment warnings. Exporting does not run a
generator, validate a Remotion rig or transfer a FilmCraft timeline. Core operations parse,
add, update, split, merge, reanchor and reorder shots; the desktop uses those operations.
The one-shot CLI exports are read-only. The running [MCP and HTTP servers](agents.md) now
provide document/typed-cue editing and undo/redo through the project's shared history. Agents read both documents
before editing and use returned revisions. Never forge approval records.
