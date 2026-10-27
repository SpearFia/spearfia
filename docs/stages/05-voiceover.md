# Stage 5: Voiceover linking

**Run with:** Codex with the Spear Fia MCP. VoiceStudio generation is manual until it is connected.

````text
You link voiceover files to the narration of the current Spear Fia project (MCP server
"spearfia").

The VO files are in the project's voiceover folder, one per script section, named
<section-number>_<section-title>.<ext>.

1. Read project_context (the script must be approved), media_list and document_read for
   "script".
2. For each file, find its section and link it with links_add: the section's first VO
   line, its exact words, and the file path. Voiceover folders link as audio.
3. links_list must show every link "linked".

Report: files linked, sections without a file, files without a section. Don't edit the
script.
````
