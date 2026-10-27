# Stage 4: Visual Script and thumbnail concept

**Run with:** Claude or Codex with the Spear Fia MCP. Run after the script is approved in the desktop.

````text
You plan the visuals for the current Spear Fia project (MCP server "spearfia").

1. Read project_context (the script must be approved), channel_read, and document_read for
   "script" and "visuals". Follow style.md § Visuals and § Editing.
2. Create cues with visuals_edit covering the narration: one cue per beat or 1-3
   sentences. For each, set a title, visual_type, method (usually "LIVE footage" for clips,
   "Generated artwork" for graphics), and details:
   - "Subject": a broad pool at the character or game level ("The Witcher 3 cinematics
     or gameplay of Geralt"). No shot lists, no specific scenes.
     Every cue in a pool uses the identical Subject string; media search groups by it.
   - "match": subject (default) or mood. Use exact only for show moments, where the
     sentence describes a concrete event the viewer should see (about 5-10% of cues);
     only those get a specific VISUAL line.
   - "Search": raw-dump phrases first ("<game> all cinematics", "<game> all cutscenes").
3. Text, numbers and comparisons: method "Generated artwork" with the exact on-screen text.
4. Write one thumbnail concept and three title options in packaging.md style, as a
   Production note at the top of the Visual Script.
5. Draft the edit script, edit/edit.md in the project folder (write the file directly).
   Channel rules in execution.md already cover what repeats (section transitions, a card
   for every Generated artwork cue, captions, mix); only add what's specific to this video,
   one line per decision from this vocabulary and nothing else:
     - section "<heading prefix>" | transition dip-to-black 0.5 | sfx whoosh -8
     - "<exact script words>"@<line> | zoom punch-in | marker "Run ends"
     - "<exact script words>" | card "TEXT ON SCREEN" 3 | sfx impact
   zoom takes a recipe name from execution.md; sfx takes a file name in the project's SFX
   folder. Aim for the show moments and verdicts, 1-3 per section, not every line.

Report coverage (visuals_export cue-sheet), cues needing exact matches, and anything
you couldn't plan.
````
