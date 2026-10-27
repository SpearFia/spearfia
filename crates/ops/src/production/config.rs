// SPDX-License-Identifier: MIT OR Apache-2.0
use super::invalid;
use crate::{Doc, Origin, Result};
use serde::{Deserialize, Serialize};
use spearfia_ai::{Agent, Provider};
use spearfia_core::project::{MediaRole, Project};
use std::collections::HashSet;

pub const CONFIG: &str = "production/pipeline.json";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    #[serde(rename = "$schema", default = "schema")]
    pub schema: String,
    pub spec_version: u32,
    pub stages: Vec<Stage>,
}
fn schema() -> String {
    "https://spearfia.dev/schemas/production-pipeline/v1.json".into()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    pub id: String,
    pub title: String,
    pub agent: Agent,
    pub prompt: String,
    pub production: bool,
    pub review: bool,
    pub checks: Vec<Check>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Check {
    File {
        path: String,
    },
    Script,
    Visuals,
    Links {
        role: MediaRole,
        minimum: usize,
        coverage: bool,
    },
    Filmcraft,
}
impl Pipeline {
    pub fn parse(source: &str) -> Result<Self> {
        if source.len() > 1024 * 1024 {
            return Err(invalid("Pipeline configuration exceeds 1 MiB."));
        }
        let config: Self =
            serde_json::from_str(source).map_err(|e| invalid(format!("Invalid pipeline: {e}")))?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        if self.spec_version != 1 || self.stages.is_empty() || self.stages.len() > 32 {
            return Err(invalid("A v1 pipeline must have 1–32 stages."));
        }
        let mut ids = HashSet::new();
        let mut production = false;
        for s in &self.stages {
            if !s.id.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                || s.id.len() > 64
                || !s
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                || !ids.insert(&s.id)
                || s.title.trim().is_empty()
                || s.title.len() > 65536
                || s.prompt.trim().is_empty()
                || s.prompt.len() > 65536
                || s.checks.is_empty()
                || s.checks.len() > 32
            {
                return Err(invalid(
                    "Stages need unique lowercase IDs, a title, instructions and 1–32 output checks.",
                ));
            }
            if production && !s.production {
                return Err(invalid("Keep planning stages before production stages."));
            }
            production |= s.production;
            s.agent.validate()?;
            for check in &s.checks {
                match check {
                    Check::File { path } => {
                        super::verify::relative_output(path)?;
                    }
                    Check::Links { minimum, .. } if !(1..=5000).contains(minimum) => {
                        return Err(invalid("Link minimum must be 1–5000."));
                    }
                    Check::Links {
                        role,
                        coverage: true,
                        ..
                    } if *role != spearfia_core::project::MediaRole::Voiceover => {
                        return Err(invalid(
                            "Spoken-line coverage checks require the voiceover role.",
                        ));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
pub fn load_pipeline(project: &Project) -> Result<(Pipeline, String)> {
    let source = spearfia_core::channels::read_limited(&project.root.join(CONFIG), 1024 * 1024)?;
    Ok((Pipeline::parse(&source)?, crate::version(&source)))
}
pub fn save_pipeline(
    project: &Project,
    config: &Pipeline,
    expected: &str,
    origin: Origin,
) -> Result<String> {
    config.validate()?;
    let current = crate::read(project, CONFIG)?;
    let doc = Doc::from_text(CONFIG, current);
    doc.expect(Some(expected))?;
    let text = serde_json::to_string_pretty(config).map_err(|e| invalid(e.to_string()))? + "\n";
    crate::apply(
        project,
        &doc.plan(text, "Configure production agents and stages"),
        origin,
    )
}
fn file(path: &str) -> Check {
    Check::File { path: path.into() }
}
fn links(role: MediaRole, coverage: bool) -> Check {
    Check::Links {
        role,
        minimum: 1,
        coverage,
    }
}
pub fn defaults() -> Pipeline {
    use MediaRole::*;
    let definitions = [
        (
            "channel-dna",
            "Channel DNA",
            Provider::Claude,
            false,
            true,
            "Use the channel-dna skill and the attached channel guides. Ask for reference URLs if missing. Study the reference channel, update its shared guides through their supported operations, and write production/channel-dna.md listing the evidence and saved guide paths. Explain if the project snapshot should be refreshed before scripting.",
            vec![file("production/channel-dna.md")],
        ),
        (
            "ideas",
            "Ideas and selection",
            Provider::Claude,
            false,
            true,
            "Read channel idea/packaging rules. Write ranked original ideas with titles, thumbnail concepts and feasibility to production/ideas.md. Ask the creator to select an idea at the review gate.",
            vec![file("production/ideas.md")],
        ),
        (
            "script",
            "Script",
            Provider::Claude,
            false,
            true,
            "Use the creator's selected idea and the channel writing guide to draft the complete narration through Spear Fia document_edit. Save the script, then stop for creator review and saved-script approval in the desktop.",
            vec![Check::Script],
        ),
        (
            "visual-script",
            "Visual Script",
            Provider::Claude,
            false,
            false,
            "Read narration and channel visual rules. Use visuals_edit to create a visual plan covering narration, with resolved anchors, methods, references and concrete production instructions. Do not generate assets yet.",
            vec![Check::Visuals],
        ),
        (
            "voiceover-sample",
            "Voiceover sample",
            Provider::Codex,
            true,
            true,
            "Use the voiceover skill and configured VoiceStudio voice. Generate only the first section, verify the audio, link it to narration with links_add, and stop so the creator can listen.",
            vec![links(Voiceover, false)],
        ),
        (
            "voiceover",
            "Remaining voiceover",
            Provider::Codex,
            true,
            false,
            "Use the voiceover skill. Reuse the accepted sample. Generate, verify and link remaining sections. Every spoken line must be covered by recorded narration. Keep script unchanged.",
            vec![links(Voiceover, true)],
        ),
        (
            "media-search",
            "Media search",
            Provider::Codex,
            true,
            false,
            "Use the media-search skill and Visual Script. Reuse the shared library first. Search, inspect and link footage to narration with links_add. Record sources, filled cues and unresolved coverage in production/media-report.md. If required cues remain unfilled, return blocked or needs_input.",
            vec![links(BRoll, false), file("production/media-report.md")],
        ),
        (
            "audio-sample",
            "Music and SFX samples",
            Provider::Codex,
            true,
            true,
            "Use the audio-gen skill and channel sound rules. Reuse the library first. Produce and link one music bed and one SFX, then stop so the creator can listen. Write the proposed score in production/audio-plan.md.",
            vec![
                links(Music, false),
                links(Sfx, false),
                file("production/audio-plan.md"),
            ],
        ),
        (
            "audio",
            "Remaining music and SFX",
            Provider::Codex,
            true,
            false,
            "Use the audio-gen skill. Follow the accepted audio plan, reuse accepted samples, and generate/verify/link remaining music and SFX. Report unscored sections explicitly; keep script unchanged.",
            vec![links(Music, false), links(Sfx, false)],
        ),
        (
            "assembly",
            "FilmCraft review",
            Provider::Codex,
            true,
            true,
            "Inspect media links, call timeline_push once, and inspect all returned issues. Write the review path and every outstanding issue to production/assembly-report.md. Return needs_input if issues need creator decisions. Stop for FilmCraft review; do not publish.",
            vec![Check::Filmcraft, file("production/assembly-report.md")],
        ),
        (
            "packaging",
            "Thumbnail and description",
            Provider::Claude,
            true,
            true,
            "After the creator's FilmCraft review, use the channel packaging rules and available authoring tools. Create production/thumbnail.png and production/description.md with description and chapters. Ask about missing tools. Stop for creator review; do not publish.",
            vec![
                file("production/thumbnail.png"),
                file("production/description.md"),
            ],
        ),
    ];
    Pipeline {
        schema: schema(),
        spec_version: 1,
        stages: definitions
            .into_iter()
            .map(
                |(id, title, provider, production, review, prompt, checks)| Stage {
                    id: id.into(),
                    title: title.into(),
                    agent: Agent::new(provider),
                    prompt: prompt.into(),
                    production,
                    review,
                    checks,
                },
            )
            .collect(),
    }
}
