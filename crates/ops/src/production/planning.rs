// SPDX-License-Identifier: MIT OR Apache-2.0
//! Build service jobs from the saved narration and its registered audio links.
use super::{
    invalid,
    jobs::{self, Anchor, Job, Output, Plan, Task},
};
use crate::Result;
use spearfia_core::{
    links::{self, LinkKind, Links},
    project::{MediaRole, Project},
};
use spearfia_script::{Block, LineKind, format::line_words};

fn existing(project: &Project) -> Result<Plan> {
    if project.root.join(jobs::PLAN).exists() {
        Ok(jobs::load_plan(project)?.0)
    } else {
        Ok(Plan {
            spec_version: 1,
            jobs: vec![],
        })
    }
}
fn output(path: String, kind: &str, anchor: Option<Anchor>) -> Output {
    Output {
        path,
        kind: kind.into(),
        role: anchor.as_ref().map(|_| MediaRole::Voiceover),
        anchor,
        node: None,
        index: 0,
    }
}
#[derive(serde::Deserialize)]
pub struct VoiceSettings {
    pub url: String,
    pub profile_id: String,
    #[serde(default)]
    pub instruct: String,
    #[serde(default = "language")]
    pub language: String,
    #[serde(default = "speed")]
    pub speed: f64,
    #[serde(default = "steps")]
    pub steps: u32,
}
fn language() -> String {
    "auto".into()
}
fn speed() -> f64 {
    1.0
}
fn steps() -> u32 {
    16
}
pub fn voiceover(project: &Project, settings: &VoiceSettings) -> Result<Plan> {
    let source = project.approved_script()?;
    let script = spearfia_script::parse(&source);
    let folder = project
        .file
        .media_folders
        .iter()
        .find(|f| f.role == MediaRole::Voiceover)
        .ok_or_else(|| invalid("Configure a project voiceover media folder first."))?;
    super::verify::relative_output(&format!("{}/test.wav", folder.path))?;
    let mut sections: Vec<Vec<(usize, String)>> = vec![vec![]];
    for block in script.blocks {
        match block {
            Block::Heading { level: 1 | 2, .. } => {
                if !sections.last().unwrap().is_empty() {
                    sections.push(vec![]);
                }
            }
            Block::Line {
                kind: LineKind::Spoken,
                line,
                segments,
            } => {
                let words = line_words(&segments);
                if !words.is_empty() {
                    sections.last_mut().unwrap().push((line, words.join(" ")));
                }
            }
            _ => {}
        }
    }
    let mut plan = existing(project)?;
    plan.jobs
        .retain(|j| !j.id.starts_with("narration-section-"));
    for (index, section) in sections.iter().filter(|s| !s.is_empty()).enumerate() {
        let id = format!("narration-section-{:04}", index + 1);
        let stage = if index == 0 {
            "voiceover-sample"
        } else {
            "voiceover"
        };
        let text = section
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        plan.jobs.push(Job {
            id,
            title: format!("Narration section {}", index + 1),
            stage: stage.into(),
            depends_on: if index == 0 {
                vec![]
            } else {
                vec!["narration-section-0001".into()]
            },
            inputs: vec![],
            checks: vec![],
            task: Task::Voiceover {
                url: settings.url.clone(),
                text,
                profile_id: settings.profile_id.clone(),
                instruct: settings.instruct.clone(),
                language: settings.language.clone(),
                speed: settings.speed,
                steps: settings.steps,
                outputs: vec![output(
                    format!("{}/{:04}-{{key}}.wav", folder.path, index + 1),
                    "audio",
                    Some(Anchor {
                        line: section[0].0,
                        quote: section[0]
                            .1
                            .split_whitespace()
                            .take(8)
                            .collect::<Vec<_>>()
                            .join(" "),
                    }),
                )],
            },
        });
    }
    if !plan
        .jobs
        .iter()
        .any(|j| j.id.starts_with("narration-section-"))
    {
        return Err(invalid(
            "There are no VO sections to generate. Live A-roll is recorded separately.",
        ));
    }
    plan.validate()?;
    Ok(plan)
}
pub fn alignment(project: &Project, url: &str, language: &str) -> Result<Plan> {
    let source = project.approved_script()?;
    let script = spearfia_script::parse(&source);
    let links = Links::load(project)?;
    let located: Vec<_> = script
        .blocks
        .iter()
        .filter_map(|b| {
            if let Block::Line { line, segments, .. } = b {
                Some((*line, line_words(segments)))
            } else {
                None
            }
        })
        .collect();
    let mut speech = Vec::new();
    let mut section = 0;
    for b in &script.blocks {
        match b {
            Block::Heading { level: 1 | 2, .. } => section += 1,
            Block::Line { line, kind, .. } if kind.is_spoken() => speech.push((*line, section)),
            _ => {}
        }
    }
    let mut audio = Vec::new();
    for link in &links.links {
        if link.kind == LinkKind::Audio && !matches!(link.role, MediaRole::Music | MediaRole::Sfx) {
            let at = links::locate(&link.quote, link.line, &located)
                .ok_or_else(|| invalid(format!("Audio link {} lost its anchor.", link.id)))?;
            audio.push((located[at.line].0, link));
        }
    }
    audio.sort_by_key(|(line, _)| *line);
    let mut plan = existing(project)?;
    plan.jobs.retain(|j| !j.id.starts_with("align-narration-"));
    for (index, (line, link)) in audio.iter().enumerate() {
        let section = speech
            .iter()
            .find(|(l, _)| l == line)
            .map(|(_, s)| *s)
            .ok_or_else(|| invalid("Narration must attach to a spoken line."))?;
        let stop = audio.get(index + 1).map_or(usize::MAX, |(line, _)| *line);
        let lines = speech
            .iter()
            .filter(|(l, s)| l >= line && *l < stop && *s == section)
            .map(|(l, _)| *l)
            .collect();
        plan.jobs.push(Job {
            id: format!("align-narration-{}", link.id),
            title: format!("Align {}", link.media.path),
            stage: "word-alignment".into(),
            depends_on: vec![],
            inputs: vec![],
            checks: vec![],
            task: Task::Alignment {
                url: url.into(),
                audio: link.media.path.clone(),
                lines,
                language: language.into(),
                outputs: vec![output(
                    format!("analysis/{}-{{key}}.alignment.json", link.id),
                    "json",
                    None,
                )],
            },
        });
    }
    if audio.is_empty() {
        return Err(invalid(
            "Register narration audio before planning word alignment.",
        ));
    }
    plan.validate()?;
    Ok(plan)
}
