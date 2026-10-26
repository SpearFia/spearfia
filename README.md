# Spear Fia

Script-first, AI-assisted content creation (shorts to long-form YouTube), paired with FilmCraft.

**Status:** pre-alpha. Nothing here is stable yet.

## The Spear Fia repos

| Repo | What it is |
| --- | --- |
| [spearfia](https://github.com/spearfia/spearfia) | The app monorepo: script-first, AI-assisted content creation, paired with FilmCraft |
| [spearfia-spec](https://github.com/spearfia/spearfia-spec) | Open formats: script, asset, project, vision, recipe, analysis, timeline plan |
| [plugins](https://github.com/spearfia/plugins) | Effect suite (DCTL, OFX, WGSL) with AI-readable capability descriptors |
| [skills](https://github.com/spearfia/skills) | Agent skills: plugin-builder, script writing, editing workflows |
| [starter-pack](https://github.com/spearfia/starter-pack) | Free, license-clean starter transitions, SFX, music and presets |
| [filmcraft](https://github.com/storytold/filmcraft) | The open-source Rust video editor Spear Fia works with |

## Channel workflows

Use **Channels** to edit/import reusable Markdown guides for each channel's style,
lessons, staple assets and production tools. Choose a Channel for a new video or attach
one through **Channel / handoff** in an existing project. Approve the saved script there
before copying the production handoff for an agent.

See [channel workflow](docs/channel-workflow.md) and [agent/CLI access](docs/agents.md).
This release provides knowledge and approval-based handoff; it does not automatically
run ComfyUI, VoiceStudio or FilmCraft sync.

## Visual Script

**Visual Script** follows Script in each project: linked narration, visual directions, production details, references, coverage/review and agent exports. See [the guide](docs/visual-script.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this work, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.
