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
    Jobs {
        stage: String,
    },
    Alignment,
    Captions,
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
            "Use the voiceover skill. Read jobs_status and the saved channel voice settings. Use voiceover_plan with an explicit recorded VoiceStudio profile, then jobs_run with stage=voiceover-sample. Poll jobs_status until running=false, then inspect every state and registered output. Ask if the profile is missing. Stop for the creator to listen; never run later jobs across this review gate.",
            vec![
                links(Voiceover, false),
                Check::Jobs {
                    stage: "voiceover-sample".into(),
                },
            ],
        ),
        (
            "voiceover",
            "Remaining voiceover",
            Provider::Codex,
            true,
            false,
            "Use the voiceover skill. Reuse the verified accepted sample and jobs_run with stage=voiceover. Poll jobs_status until running=false and verify the selected stage completed. Resume recorded takes; never submit an uncertain generation again. Every spoken line must have narration or recorded live audio. If only one section exists, report that no remaining job is needed. Keep the approved script unchanged.",
            vec![links(Voiceover, true)],
        ),
        (
            "word-alignment",
            "Measured narration timing",
            Provider::Codex,
            true,
            false,
            "Read jobs_status, then alignment_plan for the registered narration using the local VoiceStudio URL. Run jobs_run stage=word-alignment and poll jobs_status until running=false. Strictly match measured words to the approved script; correct missing/reordered speech or request creator input. Never estimate word timestamps or change the approved narration.",
            vec![Check::Alignment],
        ),
        (
            "captions",
            "Animated captions",
            Provider::Codex,
            true,
            true,
            "Read the channel executable style and validate it. If no caption profile is recorded, propose grouping, font, position, emphasis and animation settings for creator review. Save production/style.json through style_save with jobs_status.style_revision. Use captions_build and inspect the grouping, line breaks and active-word ranges. Do not fabricate timing. Stop for creator review of the caption plan.",
            vec![Check::Captions],
        ),
        (
            "media-search",
            "Media search",
            Provider::Codex,
            true,
            false,
            "Use the media-search skill and Visual Script. Reuse the shared library first. Search, inspect and link footage to narration with links_add. Record sources, filled cues and unresolved coverage in production/media-report.md. If required cues remain unfilled, return blocked or needs_input.",
            vec![
                links(BRoll, false),
                Check::Visuals,
                file("production/media-report.md"),
            ],
        ),
        (
            "visual-production",
            "Graphics and visual treatments",
            Provider::Codex,
            true,
            false,
            "Realize the Visual Script after media search. Plan generated graphics/video through jobs_plan stage=visual-production and run/resume that stage with jobs_run. Add execution JSON in each cue details for text, shapes, media overlays, camera moves and native FilmCraft effects/recipes. Poll jobs_status until running=false and verify every planned output. Resolve every :look to a saved executable channel recipe. Unsupported capabilities require input rather than a claimed completion. Write production/visual-production.md with cue-to-output links and unresolved work.",
            vec![Check::Visuals, file("production/visual-production.md")],
        ),
        (
            "audio-sample",
            "Music and SFX samples",
            Provider::Codex,
            true,
            true,
            "Use the audio-gen skill and channel sound rules. Reuse verified library audio first. Plan any new ComfyUI API workflows through jobs_plan with stage=audio-sample, explicit output node/index, media role and narration anchors; run only that stage with jobs_run. Poll jobs_status until running=false; verify all selected jobs before claiming completion. Submit no filename-only cache hits. Produce one music and SFX sample, write production/audio-plan.md including levels, fades and ducking in the execution profile, and stop for creator listening.",
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
            "Use the audio-gen skill and accepted score. Plan new generation with jobs_plan stage=audio and execute/resume only that stage with jobs_run. Reuse verified samples and library assets. Poll jobs_status until running=false and verify all selected jobs. Save channel-specific dB levels, fades, attack/release and ducking to the executable style. Report unscored sections explicitly; keep the approved script unchanged.",
            vec![
                links(Music, false),
                links(Sfx, false),
                file("production/audio-plan.md"),
            ],
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
