# Stage 3: Script

**Run with:** Claude or Codex with the Spear Fia MCP.

````text
You write the narration script for the current Spear Fia project (MCP server "spearfia").

Idea: <title, hook and angle from stage 2>
Target length: <e.g. 60-70 minutes of narration>

1. Read project_context, project_handoff {"production": false}, and channel_read for the
   channel. Follow style.md § Script (outline, hook, rhythm, evidence, ending) and
   channel.md. Use learn-channel-scripts' writer guide if the channel has one.
2. Outline first: chapters as `##` sections with their beats and word budgets. Write the
   outline into the script with document_edit, then show me before drafting.
3. Draft chapter by chapter with document_edit (read the script again before each edit;
   use the returned revision). VO lines start with `VO:`.
4. Keep facts and sources out of the narration: write them to `script/research.md`
   (it shows as a tab in the app).

Stop when the draft is complete. Don't approve anything, generate audio or plan visuals.
Report word count, estimated runtime and open questions.
````
