// SPDX-License-Identifier: MIT OR Apache-2.0
use super::{Check, Stage, invalid};
use crate::Result;
use spearfia_core::{
    links::{self, Links},
    project::{MediaRole, Project},
    visuals::Plan,
};
use spearfia_script::{Block, format::line_words};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path},
    time::UNIX_EPOCH,
};

pub(crate) fn relative_output(path: &str) -> Result<()> {
    let normalized = path.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > 4096
        || normalized.contains(':')
        || normalized.contains('\0')
        || Path::new(&normalized)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
        || lower.starts_with("sync/")
        || lower.starts_with('.')
        || lower == super::CONFIG
        || ["project.json", "channel.md", "links.json", "vision.yaml"].contains(&lower.as_str())
    {
        return Err(invalid(
            "Output checks must name ordinary project-relative artifacts, not configuration, approval records or run state.",
        ));
    }
    Ok(())
}
fn stamp(path: &Path) -> Result<String> {
    let meta = fs::metadata(path)
        .map_err(|e| invalid(format!("Missing output {}: {e}", path.display())))?;
    if !meta.is_file() || meta.len() == 0 {
        return Err(invalid(format!(
            "Output {} is empty or not a file.",
            path.display()
        )));
    }
    let text = path.extension().is_some_and(|x| {
        ["md", "json", "yaml", "txt"]
            .iter()
            .any(|s| x.eq_ignore_ascii_case(s))
    });
    if text && meta.len() <= 2 * 1024 * 1024 {
        let source = fs::read_to_string(path).map_err(|e| invalid(e.to_string()))?;
        if source.trim().is_empty() {
            return Err(invalid(format!(
                "Output {} contains no text.",
                path.display()
            )));
        }
        return Ok(crate::version(&source));
    }
    let modified = meta
        .modified()
        .map_err(|e| invalid(e.to_string()))?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| invalid(e.to_string()))?
        .as_nanos();
    Ok(format!("file:{}:{modified}", meta.len()))
}
pub(crate) fn current_stamp(project: &Project, path: &str) -> Result<String> {
    if let Some(id) = path.strip_prefix("@link:") {
        let links = Links::load(project)?;
        let link = links
            .links
            .iter()
            .find(|l| l.id == id)
            .ok_or_else(|| invalid(format!("Verified link {id} was removed.")))?;
        return Ok(crate::version(
            &serde_json::to_string(link).map_err(|e| invalid(e.to_string()))?,
        ));
    }
    stamp(&project.resolve(path))
}
fn remember(project: &Project, path: &str, artifacts: &mut BTreeMap<String, String>) -> Result<()> {
    artifacts.insert(path.into(), current_stamp(project, path)?);
    Ok(())
}
/// Verify saved artifacts independently of the agent's success claim.
pub fn verify(project: &Project, stage: &Stage) -> Result<BTreeMap<String, String>> {
    let source = project.read_script()?;
    let script = spearfia_script::parse(&source);
    let lines: Vec<_> = script
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Line { line, segments, .. } => Some((*line, line_words(segments))),
            _ => None,
        })
        .collect();
    let mut artifacts = BTreeMap::new();
    for check in &stage.checks {
        match check {
            Check::File { path } => {
                relative_output(path)?;
                let root = fs::canonicalize(&project.root).map_err(|e| invalid(e.to_string()))?;
                let resolved = fs::canonicalize(project.resolve(path))
                    .map_err(|e| invalid(format!("Missing {path}: {e}")))?;
                if !resolved.starts_with(root) {
                    return Err(invalid("An output artifact points outside the project."));
                }
                remember(project, path, &mut artifacts)?;
            }
            Check::Script => {
                if !script.blocks.iter().any(|b| matches!(b, Block::Line {kind, segments, ..} if kind.is_spoken() && !line_words(segments).is_empty())) {
                    return Err(invalid("The saved script has no spoken narration."));
                }
                remember(project, &project.file.script, &mut artifacts)?;
            }
            Check::Visuals => {
                let plan = Plan::parse(&project.read_visuals()?)?;
                if plan.cues.is_empty()
                    || plan
                        .cues
                        .iter()
                        .any(|c| c.meta.anchor.resolve(&source).range.is_none())
                {
                    return Err(invalid(
                        "Visual Script needs saved cues with resolved narration attachments.",
                    ));
                }
                remember(project, spearfia_core::visuals::FILE, &mut artifacts)?;
                remember(project, &project.file.script, &mut artifacts)?;
            }
            Check::Links {
                role,
                minimum,
                coverage,
            } => {
                let saved = Links::load(project)?;
                if !saved.unreadable.is_empty() {
                    return Err(invalid(
                        "Repair unreadable links before continuing production.",
                    ));
                }
                let selected: Vec<_> = saved.links.iter().filter(|l| l.role == *role).collect();
                if selected.len() < *minimum {
                    return Err(invalid(format!(
                        "Need at least {minimum} {} links.",
                        role.label()
                    )));
                }
                for link in &selected {
                    if link.kind != links::LinkKind::for_role(*role) {
                        return Err(invalid(format!(
                            "Link {} has a media kind inconsistent with its role.",
                            link.id
                        )));
                    }
                    if links::locate(&link.quote, link.line, &lines).is_none() {
                        return Err(invalid(format!("Link {} lost its narration.", link.id)));
                    }
                    remember(project, &link.media.path, &mut artifacts)?;
                    remember(project, &format!("@link:{}", link.id), &mut artifacts)?;
                    if *role == MediaRole::Voiceover {
                        let duration =
                            crate::timing::audio_seconds(&project.resolve(&link.media.path))
                                .filter(|s| s.is_finite() && *s > 0.0)
                                .ok_or_else(|| {
                                    invalid(format!(
                                        "Voiceover {} cannot be measured as playable audio.",
                                        link.media.path
                                    ))
                                })?;
                        let start = point(link.in_point.as_deref())?.unwrap_or(0.0);
                        let end = point(link.out_point.as_deref())?.unwrap_or(duration);
                        if end <= start || start >= duration || end > duration + 0.05 {
                            return Err(invalid(format!(
                                "Voiceover {} has invalid source in/out points.",
                                link.id
                            )));
                        }
                    }
                }
                if *coverage {
                    if *role != MediaRole::Voiceover {
                        return Err(invalid(
                            "Full spoken-line coverage currently requires voiceover-role links.",
                        ));
                    }
                    let timing = crate::timing::vo_seconds(
                        project,
                        &script,
                        &saved,
                        &mut crate::timing::Probe::default(),
                    );
                    let missing: Vec<_> = script
                        .blocks
                        .iter()
                        .filter_map(|b| match b {
                            Block::Line {
                                kind,
                                segments,
                                line,
                            } if *kind == spearfia_script::LineKind::Spoken
                                && !line_words(segments).is_empty()
                                && !timing.get(line).is_some_and(|s| s.is_finite() && *s > 0.0) =>
                            {
                                Some(line + 1)
                            }
                            _ => None,
                        })
                        .collect();
                    if !missing.is_empty() {
                        return Err(invalid(format!(
                            "Recorded narration does not cover script lines {missing:?}."
                        )));
                    }
                }
            }
            Check::Filmcraft => {
                remember(project, &project.file.script, &mut artifacts)?;
                remember(project, spearfia_core::links::LINKS_FILE, &mut artifacts)?;
                let folder = project.root.join("sync");
                let mut reviews: Vec<_> = fs::read_dir(&folder)
                    .map_err(|e| invalid(e.to_string()))?
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension().is_some_and(|s| s == "fcproj")
                            && p.file_stem()
                                .is_some_and(|s| s.to_string_lossy().starts_with("review-plan-"))
                    })
                    .collect();
                reviews.sort();
                let review = reviews
                    .last()
                    .ok_or_else(|| invalid("No FilmCraft review project was saved."))?;
                let stem = review.file_stem().unwrap().to_string_lossy();
                let id = stem.strip_prefix("review-").unwrap();
                for path in [
                    format!("sync/{stem}.fcproj"),
                    format!("sync/plans/{id}.json"),
                    format!("sync/snapshots/{id}.json"),
                ] {
                    remember(project, &path, &mut artifacts)?;
                    if path.ends_with(".json") {
                        let text = spearfia_core::channels::read_limited(
                            &project.resolve(&path),
                            8 * 1024 * 1024,
                        )?;
                        let value: serde_json::Value = serde_json::from_str(&text)
                            .map_err(|e| invalid(format!("Invalid review artifact: {e}")))?;
                        if path.contains("/plans/")
                            && (value["plan_id"] != id
                                || !value["items"]
                                    .as_array()
                                    .is_some_and(|items| !items.is_empty()))
                        {
                            return Err(invalid(
                                "FilmCraft review plan must match its ID and contain placed items.",
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(artifacts)
}

fn point(value: Option<&str>) -> Result<Option<f32>> {
    let Some(value) = value else { return Ok(None) };
    let parts: Vec<_> = value.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(invalid("Invalid audio source point."));
    }
    let mut total = 0.0;
    for part in parts {
        let n: f32 = part
            .parse()
            .map_err(|_| invalid("Invalid audio source point."))?;
        if !n.is_finite() || n < 0.0 {
            return Err(invalid(
                "Audio source points must be finite and nonnegative.",
            ));
        }
        total = total * 60.0 + n;
    }
    if !total.is_finite() {
        return Err(invalid("Audio source point is too large."));
    }
    Ok(Some(total))
}
