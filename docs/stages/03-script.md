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
5. Retention review before I see the draft (style.md § Script › Process 4b): hand the draft
   to a separate agent with the reviewer prompt below. The writer never scores its own
   draft; if you can't start another agent, stop and tell me to run the reviewer.

Stop when the draft is complete. Don't approve anything, generate audio or plan visuals.
Report word count, estimated runtime, the review's final scores and open questions.
````

## Reviewer (step 5)

Scores the draft against the channel's retention rubric (style.md § Script › Retention
rubric) and rewrites only what fails. The channel guides win over generic retention advice;
the research behind the rubric is `script-research.md` in the channel's DNA study folder.

````text
You review a script draft for the current Spear Fia project (MCP server "spearfia"). You
didn't write it.

1. Read channel_read for the channel: style.md § Script (Process, Retention rubric, Rhythm)
   and tools.md § Voice (audio tags). Read the beat sheet and the script with document_read.
2. Score every rubric check. Words ÷ 160 = minutes; count words without [tags]. Write
   `script/review.md`: a table of check, measured value, pass/fail and the sections
   involved, then the rubric's verdict (ready or not).
3. If not ready, rewrite only the failing sections with document_edit (read the script
   again before each edit; use the returned revision). Open the matching exemplar first and
   follow § Rhythm. Keep approved placements, feats, verdicts and the creator's own lines.
   A fix adds a re-hook, a device or a feat; it doesn't pad. Keep tags per tools.md.
4. Re-measure the rewritten sections (Process step 4), re-score, and append round 2 to
   review.md. Stop after two rounds.

Report the scores before and after, the sections you changed, and what still fails.
````
