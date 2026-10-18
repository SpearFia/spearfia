# Sample project

A minimal Spear Fia project folder. Footage is not included (media is never committed);
`footage/day1.mp4` is referenced but absent.

| Path | What it is |
| --- | --- |
| `project.json` | Library roots, adapter, settings |
| `vision.yaml` | Creative vision: preset + overrides |
| `script/script.md` | Text-editable script with directives |
| `links.json` | Script section ↔ clip (content hash + relative path) |
| `analysis/` | Footage analysis sidecars (persistent, regenerable) |
| `sync/` | Last-synced snapshot, ID map, sync log |
| `cache/` | Proxies, transcripts, embeddings (gitignored) |
| `exports/` | Renders (gitignored) |
