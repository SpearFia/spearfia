# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Agent editing

- Implement project-bound MCP stdio and authenticated local HTTP editing tools for narration and Visual Script, including typed cue operations, revision conflicts, reference binding and bounded undo/redo. Clean agent edits enter desktop undo; production approval remains creator-only.
- One undo history: MCP/HTTP Script and Visual Script edits and media links (desktop and agents) now go through `spearfia-ops`, under the project edit lock, and are recorded in `sync/history.jsonl` with transitions. `edit_undo`/`edit_redo` (and new `spearfia redo`) work across the desktop, CLI and agents and survive restarts. Revisions are content versions shared by every surface; `expected_revision` on undo/redo is now optional. History keeps the newest 100 changes (32 MiB).
- Agents can list project media and add, list and remove media links (`media_list`, `links_list`, `links_add`, `links_remove`). Link saves now start from the saved `links.json` under the project edit lock, so desktop and agent links no longer overwrite each other, and the desktop shows agent links within a second.

### Visual Script

- Add Visual Script after Script: shared Markdown editor, narration-linked visual cues, contextual production details, references, coverage/readiness/review, split/merge/shots and agent exports.

### Channels

- Reusable channel workspaces store editable/importable Markdown for identity, style,
  lessons, staple assets and production tools. Videos keep portable channel.md snapshots.
  Saved-script approval guards production handoffs and becomes stale when the script or
  channel brief changes. Read-only CLI commands expose channel context, drafting briefs
  and guarded handoffs to agents. Workflow/MCP documentation covers ComfyUI, VoiceStudio,
  optional editing stages and FilmCraft review, distinguishing implemented features from
  external integrations and remaining scaffolds.


### Media folders

- Copy files from Explorer into a highlighted media folder. Create folders, drag files
  and folders into the hierarchy, or move folders to the project root. Moves preserve
  script links and folder roles; imports keep originals and never overwrite existing files.

### Media linking

- Select across script lines and blank breaks, or click/drop onto a section heading.
  Link checked clips together or keep adding clips to the held selection. Overlapping
  links have a more transparent highlight. Hover linked words for a second to see clip
  names and reveal/highlight their files in the media sidebar. Multi-line linking saves
  all anchors together and keeps the existing links.json format.


### Vision catalog

- Catalog → Kind → Vision shows built-in and custom creative briefs. Copy an AI prompt
  to turn channel-analysis notes and your requirements into validated vision JSON, then
  paste or import it. Saved templates are available in New Project and catalog ranking;
  projects embed their rules, references, pacing and weights for portability.


### Added

- Initial repository scaffold.
- Project model (`spearfia-core`): create, open and save projects; `create_project` writes
  `project.json`, `vision.yaml`, `script/script.md`, `links.json` and the media, `analysis/`,
  `sync/`, `cache/` and `exports/` folders. Recent projects and catalog sources are kept in
  the user's config folder (`SPEARFIA_CONFIG_DIR` overrides it).
- Script parser (`spearfia-script`): front matter, `##` sections, spoken and `[ON SCREEN]`
  lines, inline directives with byte spans, transition dividers, runtime estimates.
- Catalog (`spearfia-catalog`): loads `*.asset.json` from local folders, search with vision
  weights, license status.
- Built-in vision presets (`spearfia-vision`) matching `starter-pack/presets`.
- FilmCraft detection (`spearfia-adapter-filmcraft`): `SPEARFIA_FILMCRAFT`, install folders,
  `PATH`.
- Timeline plan compiler (`spearfia-sync::compile`, ADR 0005): `links.json` and script order
  become a `timeline-plan`. Voiceover links set the timing on A1, picture links fill each
  voiceover's stretch by word position, `:transition` cues attach to the next picture clip.
  Lost, missing, short and unplaced links are reported, not guessed. Pinned `links` and
  updated `timeline-plan` schemas.
- FilmCraft push (`spearfia-adapter-filmcraft::push`, ADR 0005): imports the linked media into
  a headless `filmcraft-cli mcp` session, builds the compiled plan (clips named with their
  `{#cue}` IDs, transitions), and saves a new `sync/review-<plan>.fcproj` with the plan and
  a sequence snapshot. Nothing is written if any step fails; earlier reviews are never
  overwritten.
- Push entry points: the desktop **Sync** stage (approval and FilmCraft status, a Push
  button, the saved review with its issues, Open in FilmCraft), `spearfia push filmcraft
  <project>` and the MCP/API tool `timeline_push`. All require the creator's script
  approval (`Project::approved_script`). Checked against FilmCraft 0.13.0.
- Desktop app: Projects and Catalog tabs, New Project form, and the workspace with the
  workflow bar (Script, Media, Transitions, Effects, Music, Sound effects, Sync). The Script stage shows the
  script as a page with cue chips and transition dividers, plus a Markdown editing mode.
  The other stages are placeholders.
- Media stage: the project's media folders as a tree (roles shown, subfolders inherit them),
  "Link folder" for folders inside or outside the project, a clip grid, and a script view
  that looks exactly like the Script page but is read-only. Highlight words (the highlight
  stays), then drag a clip from the folders onto it; dropping on an unhighlighted line links
  the whole line. Picture links color the words in the role's color, voiceover links
  underline them, and each link gets a clickable tag with Unlink. Coverage per section.
- `links.json` (`spearfia-core::links`): each link keeps the quoted words and their line, and
  is found again after script edits (nearest line containing the quote); links whose words
  were deleted are listed as lost.
- The Script page and the Media script view share one renderer (`script_view.rs`), and the
  Script page shows links and coverage bars too.
- Music stage: the script laid out over time. Lanes for sections, script lines (VO and
  on-screen), music bars (the level dips under VO), stingers (`:sfx`) and transitions, plus
  a full-video overview with a movable view box and coverage per section. Music from the
  catalog is listed on the left, ranked by the project's vision; drag a track onto a line
  to start it there, onto a bar to swap the track, or onto a red gap to fill it. The
  inspector shows the selected cue's track, tempo, length and markers, suggests tracks
  that match its mood, and can remove it. Checks list gaps, cues with no track, tracks
  missing from the catalog, tracks shorter than what they cover, and licenses to check.
- Script timing (`spearfia-script::timing`): estimated start and end of every line (150 wpm
  for spoken lines, 2 s for on-screen lines), music spans, gaps, stingers and transitions.
- Script text edits (`spearfia-script::edit`): add, replace and remove cues while leaving the
  rest of `script.md` untouched; `Directive::to_source`, `next_id`.
- Script page rebuilt as a document editor laid out like Google Docs (the "1b" frame in
  Figma): type directly on the page. Markdown syntax is hidden except on the caret's line;
  cues show as colored chips, transition lines as centered dividers, and the margin shows
  VO/LIVE tags, coverage bars, bullets, clickable checkboxes and section times.
  - Header with the document title (edits the front-matter title), autosave status,
    File / Edit / View / Insert / Format / Cues / Tools / Help menus, a comments toggle and
    Editing / Suggesting / Viewing modes.
  - Toolbar: search menus, undo/redo, zoom, paragraph styles (Title, Heading 1–3, VO line,
    Live line, On-screen, Note), bold/italic/underline/strikethrough, link, comment,
    link-a-clip, checklist/bullets/numbering, indent, clear formatting, and buttons that
    insert SFX, Music, Look and Clip cues and transition dividers. Docs-style shortcuts.
  - Typing `/` opens an insert menu (cues, chapter card, divider, styles, lists, table,
    comment).
  - Document tabs: the `.md` files in `script/`, with "+" for a new tab; the open tab shows
    its summary (front-matter `summary:`) and a clickable heading outline with times.
  - Comments and suggested edits in the right margin, level with their text: reply,
    resolve, delete, and accept/reject suggestions. Stored in `comments.json`.
  - Find and replace, word count, script syntax help, Paged / Pageless / Markdown views
    with a ruler and page breaks, and a status bar (page, words, spoken runtime, Ln/Col).
  - "Link clip" on a selection opens the Media stage with those words highlighted.
- The Media stage's script view uses the same document engine (`doc.rs`), so it shows the
  script in exactly the same format; link file names sit beside the page.
- Script format: `LIVE:` lines (spoken to camera), and notes that are never spoken or timed
  (`> quotes`, `- bullets`, `1. numbered`, `- [ ] checklists`, `| tables |`). Inline
  `**bold**`, `*italic*`, `++underline++`, `~~strike~~` and links; link quotes ignore the
  formatting marks.
- `spearfia-script::format`: paragraph styles, inline marks, lists, indent, word ranges.
- `spearfia-core::comments` (`comments.json`) and `spearfia-core::tabs` (document tabs).
- IBM Plex Sans (SIL OFL 1.1) bundled for the document, with real bold and italic faces.
- Formatting in the Script editor, still stored in `script.md` (`spearfia-script::attrs`):
  - Paragraph styles in the front matter (`styles: { vo: { font, size, color, … } }`), edited
    in Format → Paragraph styles…, or by "Update style to match" / "Reset style".
  - Paragraph overrides at the end of a line: `{align=center spacing=1.5 size=20}`.
  - Inline spans: `[words]{color=red highlight=yellow size=18 font="Bebas Neue"}`.
  - Toolbar: font menu, font size (− 15 +), text color, highlight color, alignment and line
    spacing. With text selected they format the selection; with no selection, the paragraph.
    Shortcuts: Ctrl+Shift+. / , for size, Ctrl+Shift+L / E / R for alignment.
  - The editor hides the syntax like the other Markdown marks; the Media script view shows
    the same formatting.
- Fonts (`fonts.rs`): bundled IBM Plex Sans / Serif / Mono and Bebas Neue, any `.ttf`/`.otf`
  in the project's `fonts/` folder, and the fonts installed on the computer (scanned in the
  background the first time the font menu opens). A font that isn't on the computer falls
  back to IBM Plex Sans and is flagged in the font menu.
- Catalog: optional `audio` details on assets (`bpm`, `seconds`, `markers`), `Asset::short_name`,
  and `Catalog::find` to look up the asset a cue label names.
- Transitions stage: every cut between two timed lines, with the shot before and after it
  (linked clip, or "no clip"), the line text, both sections and the time of the cut, and
  where the transition came from (script divider, cue on the outgoing line, or nothing
  written: a plain cut). Filters for all cuts, section changes, cuts within a section, and
  unset section changes. Pick a transition from the vision-ranked library, from the
  suggestions on a row, by typing a name, or by dragging it onto a cut; set its duration,
  make it a plain cut again, or apply one transition to every unset section change. All
  edits are written to `script.md` as `:transition[...]` dividers.
- `spearfia-script::cuts`: lists the cuts with their transition and source, and sets,
  clears and times them.
- Effects stage: every line that takes time, grouped by section, with the looks that cover
  it. Bars on the left show what each look covers; chips on the right name the looks on the
  line (filled when written there, outlined when they come from the heading or an earlier
  line). Filters for all lines, lines with effects, on-screen lines with no look (with
  suggestions taken from the line's words and the vision), and looks the catalog doesn't
  have. Select a look to change its scope (this line, N lines, the whole section), edit its
  settings (menus and number fields from the catalog asset's capabilities, values bound to
  analysis such as face tracking shown as such, plus free `key=value` attributes) or remove
  it. Add one from the library (effects and templates, ranked by vision) with "Use", by
  typing a name, or by dragging it onto a line or a section heading. Every change is a text
  edit to `script.md`.
- `spearfia-script::looks`: lists the `:look` cues with the lines they cover and when they
  run, and adds, removes, renames, re-scopes and sets attributes on them.
- Sound effects stage: every `:sfx` cue on the timeline over the script. When zoomed in, the
  script lane shows each spoken word at its estimated time. Drag a sound from the library
  onto the timeline: it lands before the nearest word, on a cut (snaps within a few pixels
  of one), on a section heading, or as an on-screen line starts, with a guide line and a
  hint ("on \"Hours\"", "on the cut into …") while dragging. Drag a cue to move it, nudge
  it a word left or right, swap it for a sound with the same tags, rename or remove it.
  Cuts show their transition, and music shows its hit points (track markers such as
  `drop`). Checks: sounds missing from the catalog, two sounds within 0.3 s, a sound within
  0.5 s of a music hit, a long sound over VO, and licenses. A cue label that isn't an exact
  catalog name is matched by tag (`boom` → `boom-impact`).
- `spearfia-script::sfx`: lists the cues with where they sit (word, cut, line end,
  on-screen line, section) and when they play, lists the spoken words with their times, and
  adds, moves, nudges, renames and removes cues. A cue inserted before a styled word goes
  outside its `[...]{...}` span and inline marks.
- The Music and Sound effects stages share one timeline (`screens/timeline.rs`): view,
  zoom and pan, overview strip, ruler, script cards, lane labels and legend.
- A test pins the sound effect placements to the example in `spearfia-spec`
  (`examples/script/script.md`), which `docs/script-directives.md` now documents.
- Catalog names match whatever the separator: `telemetry overlay` finds `telemetry-overlay`.

- Command layer (`spearfia-ops`): a change is a plan (new text, diff, the version it was
  made from) that is applied only if the file still has that version, written atomically
  and recorded in `sync/history.jsonl`. Undo comes from that shared history and refuses
  to undo over a later edit. First commands: list cuts, set / clear / time a transition,
  fill every unset section change, and the unset-section-change check.
- The Transitions stage edits through the command layer and has an Undo button.
- CLI: `spearfia cuts`, `check`, `transition set|clear|duration|fill`, `undo`, `history`,
  with `--json`, `--dry-run` (prints the diff) and `--expect <version>`.

### Fixed

- "Use" buttons in the Transitions library did nothing: the drag area sat on top of them.
  Library cards now use `widgets::drag_card`, which puts the drag area underneath. The
  Music stage's track list uses it too.
- Script lines starting with a multi-byte character (`ééé hello`, `a😀 hello`) no longer
  crash the parser when checking for `LIVE:` / `VO:`.
- Adding a cue when the script already has `#s4294967295` no longer overflows; ids are
  never reused.
- Cues added to a heading or on-screen line go before a `{align=center}` paragraph
  override instead of after it, and trailing spaces are kept.
- Styled spans no longer change word counts or cue times: `I [word]{color=red}, next` has
  three words, like `I word, next`, and formatting-only tokens (`**`) are not words.
  `line_words` and `word_ranges` agree on this.
- Dragging a transition duration, a look's line count or a numeric effect setting now
  writes the dragged value on release instead of discarding it (`widgets::drag_value`).
- Five or more overlapping looks each get their own coverage bar.
- Sound drops snap to the nearest cut in reach, not the first.
- The first zoom click on a fitted Music or SFX timeline zooms from the fitted scale; the
  stored zoom is clamped.
- SFX tag fallback matches a tag equal to the whole label (`boom nonexistent` no longer
  finds `boom`), and the inspector says when a label matches more than one sound.
- Catalog name matching ignores case for all of Unicode (`écho` finds `Écho`), and an
  exact spelling wins over a looser match (`Catalog::find_all`, `Catalog::tagged`).
- `looks::section_heading`, `sfx::words`, `sfx::sfx`, `sfx::target_of` / `nudge`,
  `cuts::set` / `clear` and `looks::set_scope` handle empty or mismatched inputs instead of
  panicking.
- The SFX spec-example test reads a pinned copy of `spearfia-spec/examples/script/script.md`
  (`crates/script/tests/fixtures/`).
- Data loss (AI-readiness audit, milestone M0):
  - Project files are written atomically (temp file, flush, rename), so a crash mid-save
    leaves the old file instead of a truncated one.
  - `links.json` and `comments.json` are never overwritten when the file on disk can't be
    read. Before, a load failure became an empty list and the next save wiped the file.
  - `links.json` entries this version can't read are kept and written back, instead of
    being dropped on the next save.
  - Link and comment ids are never reused after a delete (`next_id` counter in the file).
  - A script or tab that failed to load is never saved, so typing into the empty page
    can't overwrite the real file.
  - `edit::replace_cue` and `remove_cue` refuse when the text at the cue's position is no
    longer that cue (the file changed since it was parsed), instead of editing the wrong
    bytes.
  - `Directive::to_source` cleans labels, keys and values (`"`, `{`, `}`, line breaks), so a
    written cue always parses back with its id.
  - Accepting a suggestion whose quoted text was edited away now says so and leaves it
    open, instead of marking it resolved without changing anything. Suggestions only apply
    to their own document (`comments::apply_suggestion`).

### Spec follow-up

- `project.json` now also writes `series`, `format`, `target_runtime_seconds`,
  `media_folders` and `filmcraft`. The schema allows them (`additionalProperties`), but they
  should be named in `spearfia-spec` before the next release.
- `links.json` needs a `links` schema in `spearfia-spec` (fields: `id`, `quote`, `line`,
  `media.path`, `media.hash`, `role`, `kind`, `in`, `out`), plus the top-level `next_id`.
- `script.md`: a line that is only a `:transition[...]` directive is a divider, and a line
  starting with `[` is an on-screen direction. Add both to `docs/script-directives.md`.
- `script.md` music cues: `:music[track]` plays until the next music cue, and
  `:music[stop]` (or `off`, `none`) ends the music. A cue on a heading starts with the
  section. Add these rules to `docs/script-directives.md`.
- `script.md`: document `LIVE:`, note lines and the inline marks in
  `docs/script-directives.md`.
- `script.md`: document the `styles:` front-matter block, `{...}` paragraph overrides and
  `[text]{...}` spans (keys: font, size, color, highlight, bold, italic, align, spacing).
- `comments.json` needs a `comments` schema (`id`, `doc`, `quote`, `line`, `author`, `body`,
  `created`, `suggestion`, `resolved`, `replies`), plus the top-level `next_id`.
- `script.md` escaping: until the spec defines it, `"`, `{` and `}` are dropped from cue
  attribute values when the app writes them.
- `*.asset.json`: name the `audio` object (`bpm`, `seconds`, `markers[{name, at}]`) in the
  `asset` schema.
- `script.md` looks: on a line a `:look` covers that line, or `lines=N` lines from there; on
  a heading, the whole section; on a line of its own, the rest of the section. `cap=` picks
  one of the asset's capabilities, and other attributes are that capability's inputs. Add
  these rules to `docs/script-directives.md`.
