//! Markdown visual plans. Pure edits preserve bytes outside the changed cue/record.
use crate::{
    CoreError,
    error::Result,
    project::{Project, write_text},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    ops::Range,
};

pub const FILE: &str = "visuals/visuals.md";
pub const EMPTY: &str = "# Visual Script\n\n";
pub const MAX_BYTES: usize = 1024 * 1024;
const PREFIX: &str = "<!-- visual-cue ";
fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::Invalid(message.into())
}

/// Non-spoken paragraph roles shared by the document editor and exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Visual,
    Action,
    Text,
    Graphic,
    Prompt,
    Continuity,
    Note,
}
impl Role {
    pub const ALL: [Self; 7] = [
        Self::Visual,
        Self::Action,
        Self::Text,
        Self::Graphic,
        Self::Prompt,
        Self::Continuity,
        Self::Note,
    ];
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Visual => "VISUAL:",
            Self::Action => "ACTION:",
            Self::Text => "TEXT:",
            Self::Graphic => "GRAPHIC:",
            Self::Prompt => "PROMPT:",
            Self::Continuity => "CONTINUITY:",
            Self::Note => "PRODUCTION NOTE:",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Visual => "Visual direction",
            Self::Action => "Action / reveal",
            Self::Text => "On-screen text",
            Self::Graphic => "Graphic specification",
            Self::Prompt => "Production prompt",
            Self::Continuity => "Continuity / constraint",
            Self::Note => "Production note",
        }
    }
    pub fn of(line: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|r| line.trim_start().starts_with(r.prefix()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    pub quote: String,
    pub line: usize,
    pub lines: usize,
    pub kind: String,
    pub before: String,
    pub after: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Anchor {
    pub fn new(script: &str, range: Range<usize>) -> Result<Self> {
        let quote = script
            .get(range.clone())
            .ok_or_else(|| invalid("Select a valid narration passage."))?;
        if quote.trim().is_empty() || quote.len() > 65536 {
            return Err(invalid("Select 1–65536 bytes of narration."));
        }
        let before: String = script
            .get(..range.start)
            .unwrap_or("")
            .chars()
            .rev()
            .take(80)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let after = script
            .get(range.end..)
            .unwrap_or("")
            .chars()
            .take(80)
            .collect();
        let first_line = script
            .get(..range.start)
            .unwrap_or("")
            .bytes()
            .filter(|b| *b == b'\n')
            .count();
        let spoken: Vec<_> = script
            .lines()
            .skip(first_line)
            .take(quote.lines().count().max(1))
            .filter(|line| {
                let t = line.trim();
                !t.is_empty()
                    && !t.starts_with(['#', '>', '-', '*', '|', '[', ':'])
                    && Role::of(t).is_none()
                    && !t.starts_with("<!--")
            })
            .collect();
        if spoken.is_empty() {
            return Err(invalid(
                "Select spoken narration (VO or LIVE), rather than a heading or production note.",
            ));
        }
        let live = spoken
            .iter()
            .filter(|l| l.trim_start().to_ascii_uppercase().starts_with("LIVE:"))
            .count();
        Ok(Self {
            quote: quote.into(),
            line: script
                .get(..range.start)
                .unwrap_or("")
                .bytes()
                .filter(|b| *b == b'\n')
                .count(),
            lines: quote.lines().count().max(1),
            kind: if live == spoken.len() {
                "LIVE"
            } else if live > 0 {
                "VO / LIVE"
            } else {
                "VO"
            }
            .into(),
            before,
            after,
            extra: BTreeMap::new(),
        })
    }
    pub fn resolve(&self, script: &str) -> Resolution {
        let matches: Vec<_> = script
            .match_indices(&self.quote)
            .take(1001)
            .map(|(i, q)| i..i + q.len())
            .collect();
        let contextual: Vec<_> = matches
            .iter()
            .filter(|r| {
                script
                    .get(..r.start)
                    .is_some_and(|s| s.ends_with(&self.before))
                    && script
                        .get(r.end..)
                        .is_some_and(|s| s.starts_with(&self.after))
            })
            .collect();
        let range = if matches.len() == 1 {
            matches.first().cloned()
        } else if contextual.len() == 1 {
            contextual.first().map(|r| (*r).clone())
        } else {
            None
        };
        let range =
            range.filter(|r| Anchor::new(script, r.clone()).is_ok_and(|a| a.kind == self.kind));
        if range.is_some() {
            return Resolution {
                state: "Linked",
                range,
                candidate: String::new(),
            };
        }
        let candidate = script
            .lines()
            .skip(self.line.min(1_000_000))
            .take(self.lines.min(1_000_000))
            .collect::<Vec<_>>()
            .join("\n");
        Resolution {
            state: if matches.len() > 1 {
                "Ambiguous"
            } else if candidate.trim().is_empty() {
                "Missing"
            } else {
                "Changed"
            },
            range: None,
            candidate,
        }
    }
}
#[derive(Debug, Clone)]
pub struct Resolution {
    pub state: &'static str,
    pub range: Option<Range<usize>>,
    pub candidate: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub role: String,
    pub path: String,
    pub revision: String,
    pub scope: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    pub id: String,
    pub anchor: Anchor,
    pub visual_type: String,
    pub method: String,
    pub asset_state: String,
    pub review: String,
    pub timing: String,
    pub details: BTreeMap<String, String>,
    pub references: Vec<Reference>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Metadata {
    fn validate(&self) -> Result<()> {
        let id = self.id.as_bytes();
        if id.is_empty()
            || id.len() > 80
            || !id.first().is_some_and(u8::is_ascii_alphabetic)
            || !id
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(b))
        {
            return Err(invalid(
                "A visual cue ID must start with a letter and contain up to 80 letters, digits, underscores or hyphens.",
            ));
        }
        if self.anchor.quote.trim().is_empty()
            || self.anchor.quote.len() > 65536
            || self.anchor.line > 1_000_000
            || !(1..=1_000_000).contains(&self.anchor.lines)
            || self.anchor.before.chars().count() > 80
            || self.anchor.after.chars().count() > 80
            || self.references.len() > 128
        {
            return Err(invalid(format!(
                "Cue {} has an invalid or oversized anchor/reference list.",
                self.id
            )));
        }
        Ok(())
    }
    pub fn new(id: String, anchor: Anchor) -> Self {
        let live = anchor.kind == "LIVE";
        Self {
            id,
            anchor,
            visual_type: if live { "On camera" } else { "Scene" }.into(),
            method: if live { "LIVE footage" } else { "Reuse" }.into(),
            asset_state: "missing".into(),
            review: "draft".into(),
            timing: String::new(),
            details: BTreeMap::new(),
            references: vec![],
            extra: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct Cue {
    pub title: String,
    pub meta: Metadata,
    pub span: Range<usize>,
    pub record: Range<usize>,
    pub body: Range<usize>,
}
#[derive(Debug, Clone)]
pub struct Plan {
    pub source: String,
    pub cues: Vec<Cue>,
}
pub(crate) fn replace(src: &str, range: Range<usize>, text: &str) -> Result<String> {
    let left = src
        .get(..range.start)
        .ok_or_else(|| invalid("Invalid visual edit boundary."))?;
    let right = src
        .get(range.end..)
        .ok_or_else(|| invalid("Invalid visual edit boundary."))?;
    if range.start > range.end {
        return Err(invalid("Reversed visual edit boundary."));
    }
    let len = left
        .len()
        .saturating_add(text.len())
        .saturating_add(right.len());
    if len > MAX_BYTES {
        return Err(invalid("Visual Script is limited to 1 MiB."));
    }
    Ok(format!("{left}{text}{right}"))
}
fn record(meta: &Metadata) -> Result<String> {
    meta.validate()?;
    let text = serde_json::to_string(meta).map_err(|e| invalid(e.to_string()))?;
    Ok(format!("{PREFIX}{text} -->"))
}
impl Plan {
    pub fn parse(source: &str) -> Result<Self> {
        if source.len() > MAX_BYTES {
            return Err(invalid("Visual Script is limited to 1 MiB."));
        }
        let mut headers = Vec::new();
        let mut records = Vec::new();
        let mut offset: usize = 0;
        let mut fence: Option<(u8, usize)> = None;
        for line in source.split_inclusive('\n') {
            let text = line.trim_end_matches(['\r', '\n']);
            let trimmed = text.trim_start();
            let delimiter = trimmed.as_bytes().first().copied();
            if let Some(ch @ (b'`' | b'~')) = delimiter {
                let count = trimmed.bytes().take_while(|b| *b == ch).count();
                if count >= 3 {
                    if let Some((open, length)) = fence {
                        if open == ch
                            && count >= length
                            && trimmed.get(count..).is_some_and(|s| s.trim().is_empty())
                        {
                            fence = None;
                        }
                    } else {
                        fence = Some((ch, count));
                    }
                    offset = offset.saturating_add(line.len());
                    continue;
                }
            }
            if fence.is_none() {
                if let Some(title) = text.strip_prefix("## ") {
                    headers.push((offset, title.to_owned()));
                }
                if text.starts_with(PREFIX) {
                    let json = text
                        .strip_prefix(PREFIX)
                        .and_then(|s| s.strip_suffix(" -->"))
                        .ok_or_else(|| {
                            invalid("Incomplete visual cue record. Repair it in Markdown view.")
                        })?;
                    let meta: Metadata = serde_json::from_str(json).map_err(|e| {
                        invalid(format!(
                            "Invalid visual cue record: {e}. Repair it in Markdown view."
                        ))
                    })?;
                    meta.validate()?;
                    records.push((offset..offset + text.len(), offset + line.len(), meta));
                }
            }
            offset = offset.saturating_add(line.len());
        }
        if records.len() > 1000 {
            return Err(invalid("Visual Script is limited to 1000 cues."));
        }
        let mut ids = HashSet::new();
        let mut cues = Vec::new();
        for (record, body_start, meta) in records {
            if !ids.insert(meta.id.clone()) {
                return Err(invalid(format!("Duplicate visual cue ID: {}", meta.id)));
            }
            let Some((start, title)) = headers.iter().rev().find(|(at, _)| *at < record.start)
            else {
                return Err(invalid("A visual cue record needs a ## heading."));
            };
            if cues.iter().any(|c: &Cue| c.span.start == *start) {
                return Err(invalid("A heading can have only one visual cue record."));
            }
            let end = headers
                .iter()
                .find(|(at, _)| *at > record.start)
                .map_or(source.len(), |(at, _)| *at);
            cues.push(Cue {
                title: title.clone(),
                meta,
                span: *start..end,
                record,
                body: body_start..end,
            });
        }
        Ok(Self {
            source: source.into(),
            cues,
        })
    }
    pub fn cue(&self, id: &str) -> Result<&Cue> {
        self.cues
            .iter()
            .find(|c| c.meta.id == id)
            .ok_or_else(|| invalid("The visual cue no longer exists. Reload the plan."))
    }
    fn next_id(&self) -> String {
        // IDs use the clock plus collision checking so splitting/merging does not recycle IDs.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut id = format!("v{stamp:x}");
        while self.cues.iter().any(|c| c.meta.id == id) {
            id.push('x');
        }
        id
    }
    pub fn add(&self, title: &str, anchor: Anchor) -> Result<(String, String)> {
        if self.cues.len() >= 1000 {
            return Err(invalid("Visual Script is limited to 1000 cues."));
        }
        let id = self.next_id();
        let meta = Metadata::new(id.clone(), anchor);
        let title = title.replace(['\n', '\r'], " ");
        let new = format!(
            "\n## {}\n{}\n\nVISUAL: Describe what the viewer sees.\n\nACTION: Describe movement and narration triggers.\n\nCONTINUITY: Record what must stay consistent.\n\n",
            title.trim(),
            record(&meta)?
        );
        Ok((
            replace(&self.source, self.source.len()..self.source.len(), &new)?,
            id,
        ))
    }
    pub fn update(&self, meta: &Metadata) -> Result<String> {
        let cue = self.cue(&meta.id)?;
        replace(&self.source, cue.record.clone(), &record(meta)?)
    }
    pub fn reanchor(&self, id: &str, anchor: Anchor) -> Result<String> {
        let mut meta = self.cue(id)?.meta.clone();
        meta.anchor = anchor;
        meta.review = "draft".into();
        self.update(&meta)
    }
    pub fn split(&self, id: &str, at: usize) -> Result<(String, String)> {
        let cue = self.cue(id)?;
        if at <= cue.body.start || at >= cue.body.end || !self.source.is_char_boundary(at) {
            return Err(invalid(
                "Place the caret between visual paragraphs or shots to split this cue.",
            ));
        }
        let mut meta = cue.meta.clone();
        meta.id = self.next_id();
        meta.review = "draft".into();
        let insertion = format!("\n\n## {} (continued)\n{}\n\n", cue.title, record(&meta)?);
        let mut first = cue.meta.clone();
        first.review = "draft".into();
        let out = replace(&self.source, at..at, &insertion)?;
        let out = Plan::parse(&out)?.update(&first)?;
        Ok((out, meta.id))
    }
    pub fn merge_next(&self, id: &str, script: &str) -> Result<String> {
        let index = self
            .cues
            .iter()
            .position(|c| c.meta.id == id)
            .ok_or_else(|| invalid("Unknown cue."))?;
        let cue = self.cue(id)?;
        let next = self
            .cues
            .get(index.saturating_add(1))
            .ok_or_else(|| invalid("There is no following cue to merge."))?;
        if cue.span.end != next.span.start {
            return Err(invalid(
                "Only adjacent cues can be merged; intervening prose is preserved.",
            ));
        }
        let a = cue
            .meta
            .anchor
            .resolve(script)
            .range
            .ok_or_else(|| invalid("Resolve the first cue's narration before merging."))?;
        let b = next
            .meta
            .anchor
            .resolve(script)
            .range
            .ok_or_else(|| invalid("Resolve the next cue's narration before merging."))?;
        let mut meta = cue.meta.clone();
        meta.anchor = Anchor::new(script, a.start.min(b.start)..a.end.max(b.end))?;
        meta.review = "draft".into();
        meta.asset_state = "missing".into();
        meta.references.extend(next.meta.references.clone());
        let history = meta
            .extra
            .entry("merged_cues".into())
            .or_insert_with(|| Value::Array(vec![]));
        let Some(history) = history.as_array_mut() else {
            return Err(invalid("The merged_cues extension must be an array."));
        };
        history.push(serde_json::to_value(&next.meta).map_err(|e| invalid(e.to_string()))?);
        let body = format!(
            "## {}\n{}\n{}\n### Merged: {}\n{}",
            cue.title,
            record(&meta)?,
            self.source.get(cue.body.clone()).unwrap_or(""),
            next.title,
            self.source.get(next.body.clone()).unwrap_or("")
        );
        replace(&self.source, cue.span.start..next.span.end, &body)
    }
    pub fn shots(&self, id: &str) -> Result<Vec<Range<usize>>> {
        let cue = self.cue(id)?;
        let mut starts = Vec::new();
        let mut at = cue.body.start;
        for line in self
            .source
            .get(cue.body.clone())
            .unwrap_or("")
            .split_inclusive('\n')
        {
            if line.starts_with("### Shot") {
                starts.push(at);
            }
            at += line.len();
        }
        Ok(starts
            .iter()
            .enumerate()
            .map(|(i, start)| *start..starts.get(i + 1).copied().unwrap_or(cue.body.end))
            .collect())
    }
    pub fn add_shot(&self, id: &str) -> Result<String> {
        let cue = self.cue(id)?;
        let count = self.shots(id)?.len().saturating_add(1);
        replace(
            &self.source,
            cue.body.end..cue.body.end,
            &format!("\n### Shot {count}\nVISUAL: Describe this shot.\n\n"),
        )
    }
    pub fn move_shot(&self, id: &str, from: usize, to: usize) -> Result<String> {
        let ranges = self.shots(id)?;
        let (Some(first), Some(last)) = (ranges.first(), ranges.last()) else {
            return Err(invalid("Add ### Shot headings before reordering shots."));
        };
        if from >= ranges.len() || to >= ranges.len() {
            return Err(invalid("Choose an existing shot position."));
        }
        let mut parts: Vec<_> = ranges
            .iter()
            .filter_map(|r| self.source.get(r.clone()))
            .collect();
        let part = parts.remove(from);
        parts.insert(to, part);
        replace(&self.source, first.start..last.end, &parts.join(""))
    }
    pub fn export(&self, kind: &str, script: &str) -> Result<String> {
        if !["cue-sheet", "storyboard", "prompts", "handoff"].contains(&kind) {
            return Err(invalid("Choose cue-sheet, storyboard, prompts or handoff."));
        }
        let mut out = format!(
            "# Visual Script — {kind}\n\nDerived from {FILE}. Narration timings are estimates; visual prose has no spoken duration.\n\n"
        );
        for cue in &self.cues {
            let resolution = cue.meta.anchor.resolve(script);
            out.push_str(&format!("## {} [{}]\n\nLinked narration · {} · {}\n\n> {}\n\nType: {} · Method: {}\nAssets: {} · Review: {} · Timing: {}\n\n", cue.title, cue.meta.id, cue.meta.anchor.kind, resolution.state, cue.meta.anchor.quote.replace('\n', "\n> "), cue.meta.visual_type, cue.meta.method, cue.meta.asset_state, cue.meta.review, cue.meta.timing));
            if resolution.range.is_none() {
                out.push_str(&format!("ATTACHMENT ISSUE: {}. Candidate current passage (line hint only; confirm before reattaching):\n\n{}\n\n", resolution.state, resolution.candidate));
            }
            let body = self.source.get(cue.body.clone()).unwrap_or("");
            if kind == "prompts" {
                for line in body.lines().filter(|l| {
                    l.starts_with("PROMPT:")
                        || l.starts_with("CONTINUITY:")
                        || l.starts_with("TEXT:")
                        || l.starts_with("GRAPHIC:")
                }) {
                    out.push_str(line);
                    out.push('\n');
                }
            } else {
                out.push_str(body);
            }
            for reference in &cue.meta.references {
                out.push_str(&format!(
                    "\nReference [{}]: {} · revision/shot/frame: {} · scope: {}\n",
                    reference.role, reference.path, reference.revision, reference.scope
                ));
            }
            for (key, value) in &cue.meta.details {
                out.push_str(&format!("\n{key}: {value}\n"));
            }
        }
        if kind == "handoff" {
            out.push_str("\nThis visual plan grants no production approval. Recheck the saved script/channel handoff before generating or assembling. Discover actual tool capabilities; report missing inputs, assets and failed reviews. Do not publish.\n");
        }
        Ok(out)
    }
}
impl Project {
    pub fn read_visuals(&self) -> Result<String> {
        let path = self.root.join(FILE);
        match crate::channels::read_limited(&path, MAX_BYTES) {
            Err(CoreError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                Ok(EMPTY.into())
            }
            result => result,
        }
    }
    pub fn write_visuals(&self, expected: &str, text: &str) -> Result<()> {
        if text.len() > MAX_BYTES {
            return Err(invalid("Visual Script is limited to 1 MiB."));
        }
        if self.read_visuals()? != expected {
            return Err(invalid(
                "Visual Script changed on disk. Copy your draft, then reload before saving.",
            ));
        }
        self.write_doc_expected(FILE, expected, text)
    }
    pub fn save_visual_export(&self, path: &std::path::Path, text: &str) -> Result<()> {
        let protected = [
            self.root.join(FILE),
            self.script_path(),
            self.root.join("channel.md"),
            self.root.join("project.json"),
            self.root.join("comments.json"),
            self.root.join("sync/approved-script.md"),
            self.root.join("sync/approved-channel.md"),
            self.vision_path(),
        ];
        if protected
            .into_iter()
            .chain(
                self.document_tabs(FILE, "Visual Script")
                    .into_iter()
                    .chain(self.tabs())
                    .map(|t| self.resolve(&t.rel)),
            )
            .any(|p| {
                p == path
                    || p.canonicalize()
                        .ok()
                        .zip(path.canonicalize().ok())
                        .is_some_and(|(a, b)| a == b)
            })
        {
            return Err(invalid(
                "Choose an export filename that does not replace a project document.",
            ));
        }
        write_text(path, text)
    }
    pub fn export_visuals(&self, kind: &str) -> Result<String> {
        let plan = Plan::parse(&self.read_visuals()?)?;
        let script = self.read_script()?;
        let mut text = plan.export(kind, &script)?;
        if kind == "handoff" {
            if plan
                .cues
                .iter()
                .any(|c| c.meta.anchor.resolve(&script).range.is_none())
            {
                return Err(invalid(
                    "Resolve changed, missing or ambiguous visual attachments before production handoff.",
                ));
            }
            text.push_str(&self.agent_brief(true, None)?);
            if self.read_script()? != script || self.read_visuals()? != plan.source {
                return Err(invalid(
                    "The script or visual plan changed during export. Retry with the current documents.",
                ));
            }
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan() -> (String, Plan, String) {
        let script = "## Intro\nVO: Maya meets Jess.\nLIVE: Welcome back!\n".to_owned();
        let range = script.find("VO:").unwrap()..script.find("\nLIVE:").unwrap();
        let (source, id) = Plan::parse(EMPTY)
            .unwrap()
            .add("Garden", Anchor::new(&script, range).unwrap())
            .unwrap();
        (script, Plan::parse(&source).unwrap(), id)
    }
    #[test]
    fn anchors_survive_moves_but_do_not_silently_accept_revisions_or_duplicates() {
        let (script, p, _) = plan();
        let a = &p.cues[0].meta.anchor;
        assert!(a.resolve(&format!("New intro\n{script}")).range.is_some());
        assert_eq!(a.resolve(&script.replace("Jess", "James")).state, "Changed");
        let mut duplicate = a.clone();
        duplicate.before.clear();
        duplicate.after.clear();
        assert_eq!(
            duplicate
                .resolve("VO: Maya meets Jess.\nVO: Maya meets Jess.")
                .state,
            "Ambiguous"
        );
        assert_eq!(a.resolve("").state, "Missing");
    }
    #[test]
    fn partial_live_selection_inherits_its_original_line_kind() {
        let script = "LIVE: Welcome back!";
        let anchor = Anchor::new(script, 6..script.len()).unwrap();
        assert_eq!(anchor.kind, "LIVE");
        assert!(anchor.resolve(script).range.is_some());
        assert_eq!(anchor.resolve("VO: Welcome back!").state, "Changed");
        assert!(Anchor::new("## Heading", 0..10).is_err());
    }
    #[test]
    fn fenced_examples_do_not_become_live_cues() {
        let (_, plan, _) = plan();
        let example = format!("````markdown\n~~~\n{}\n```\n````\n", plan.source);
        assert!(Plan::parse(&example).unwrap().cues.is_empty());
    }
    #[test]
    fn metadata_changes_preserve_unknown_prose_and_extensions() {
        let (_, p, id) = plan();
        let mut m = p.cue(&id).unwrap().meta.clone();
        m.extra.insert("future".into(), Value::Bool(true));
        let source = format!("{}\nUnrecognized prose 🍀\n", p.update(&m).unwrap());
        let p = Plan::parse(&source).unwrap();
        let mut m = p.cue(&id).unwrap().meta.clone();
        m.method = "Remotion template".into();
        let out = p.update(&m).unwrap();
        assert!(out.ends_with("Unrecognized prose 🍀\n"));
        assert!(out.contains("\"future\":true"));
        assert!(
            p.export("prompts", "")
                .unwrap()
                .contains("ATTACHMENT ISSUE")
        );
    }
    #[test]
    fn split_merge_and_shot_order_preserve_content_and_ids() {
        let (script, p, id) = plan();
        let source = format!(
            "{}### Shot 1\nVISUAL: First.\n\n### Shot 2\nPROMPT: Second.\n\n",
            p.source
        );
        let p = Plan::parse(&source).unwrap();
        let moved = p.move_shot(&id, 1, 0).unwrap();
        assert!(moved.find("Shot 2").unwrap() < moved.find("Shot 1").unwrap());
        let at = source.find("### Shot 2").unwrap();
        let (split, new_id) = p.split(&id, at).unwrap();
        let p = Plan::parse(&split).unwrap();
        assert_ne!(id, new_id);
        assert_eq!(p.cues.len(), 2);
        let merged = p.merge_next(&id, &script).unwrap();
        let p = Plan::parse(&merged).unwrap();
        assert_eq!(p.cues.len(), 1);
        assert_eq!(p.cues[0].meta.id, id);
        assert!(p.source.contains("PROMPT: Second."));
        assert!(p.source.contains("merged_cues"));
    }
    #[test]
    fn hostile_input_returns_errors_without_panics() {
        let (_, p, id) = plan();
        for source in [
            format!("{}{}", p.source, p.source),
            "## Bad\n<!-- visual-cue {} -->\n".into(),
            "## Bad\n<!-- visual-cue broken".into(),
            "x".repeat(MAX_BYTES + 1),
        ] {
            assert!(Plan::parse(&source).is_err());
        }
        assert!(p.split(&id, usize::MAX).is_err());
        assert!(p.move_shot(&id, usize::MAX, 0).is_err());
        assert!(Anchor::new("é", 1..2).is_err());
        for n in 0..p.source.len() {
            let _ = Plan::parse(
                &p.source
                    .get(..n)
                    .unwrap_or("")
                    .replace("visual-cue", "visual-cue {"),
            );
        }
    }
    #[test]
    fn production_export_requires_approval_and_resolved_attachments() {
        let root = crate::testutil::temp_dir("visual-handoff");
        let project = Project {
            root: root.clone(),
            file: serde_json::from_str(r#"{"name":"Test","spec_version":1,"adapter":"filmcraft"}"#)
                .unwrap(),
        };
        let script = "VO: Maya meets Jess.";
        project.write_script(script).unwrap();
        write_text(&project.vision_path(), "{}").unwrap();
        project
            .set_channel_brief(None, "# Channel\nTest channel.")
            .unwrap();
        let (source, _) = Plan::parse(EMPTY)
            .unwrap()
            .add("Scene", Anchor::new(script, 0..script.len()).unwrap())
            .unwrap();
        project.write_visuals(EMPTY, &source).unwrap();
        assert!(project.export_visuals("handoff").is_err());
        project.approve_script(script).unwrap();
        assert!(
            project
                .export_visuals("handoff")
                .unwrap()
                .contains("Maya meets Jess")
        );
        project
            .write_script("VO: An entirely revised passage.")
            .unwrap();
        project
            .approve_script("VO: An entirely revised passage.")
            .unwrap();
        assert!(project.export_visuals("handoff").is_err());
        assert!(
            project
                .export_visuals("cue-sheet")
                .unwrap()
                .contains("ATTACHMENT ISSUE")
        );
        assert!(
            project
                .save_visual_export(&project.root.join(FILE), "wrong")
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn visual_save_rejects_external_changes() {
        let root = crate::testutil::temp_dir("visual-save");
        let project = Project {
            root: root.clone(),
            file: serde_json::from_str(r#"{"name":"Test","spec_version":1,"adapter":"filmcraft"}"#)
                .unwrap(),
        };
        assert_eq!(project.read_visuals().unwrap(), EMPTY);
        project.write_visuals(EMPTY, "# First\n").unwrap();
        assert!(project.write_visuals(EMPTY, "# Stale\n").is_err());
        assert_eq!(project.read_visuals().unwrap(), "# First\n");
        std::fs::remove_dir_all(root).unwrap();
    }
}
