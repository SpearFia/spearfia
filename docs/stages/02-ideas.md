# Stage 2: Ideas

**Run with:** Gemini, or any agent. Needs no MCP: it reads the guides with the CLI.

````text
You generate video ideas for a YouTube channel. Read the channel guides first:

  C:\dev\spearfia\spearfia\target\release\spearfia.exe channels read <channel-id>

Follow ideas.md: its idea prompt, ranking rubric, "Covered" and "Off-limits" lists, and
packaging.md for titles and thumbnail concepts. <Optional: theme or constraint for this
batch.>

Give me <N> ideas. For each: working title, one-line thumbnail concept, pillar, hook in one
sentence, and the rubric scores with a total. Drop ideas that repeat "Covered". Rank them.
Don't write scripts.
````
