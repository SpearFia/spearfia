//! Transitions: list the cuts, set / clear / time a transition, fill every unset section
//! change, and the "unset section change" check. Wraps `spearfia_script::cuts`.
//!
//! Cuts are addressed by their number in [`list`] (0-based). Numbers shift when lines are
//! added, so writers pass the version they listed from; [`crate::apply`] refuses a plan
//! made from any other version.

use crate::{Doc, OpError, Plan, Result};
use serde::Serialize;
use spearfia_script::cuts::{self, Cut, Source};
use spearfia_script::timing::{self, Timeline};
use spearfia_script::{Block, Segment};

/// A cut, as the CLI and agents see it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CutInfo {
    pub cut: usize,
    /// Seconds into the estimated timeline.
    pub at: f32,
    pub transition: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
    /// `divider`, `line` (a cue at the end of the outgoing line) or `none` (plain cut).
    pub source: &'static str,
    pub section_change: bool,
    /// A section change with no transition written: the ones worth deciding.
    pub unset: bool,
    pub from_section: Option<String>,
    pub to_section: Option<String>,
    pub from_line: String,
    pub to_line: String,
}

/// A problem a stage reports, with the cut it's about.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    pub stage: &'static str,
    pub cut: usize,
    pub message: String,
}

fn timeline_cuts(doc: &Doc) -> (Timeline, Vec<Cut>) {
    let tl = timing::timeline(&doc.script);
    let all = cuts::cuts(&doc.script, &tl);
    (tl, all)
}

fn block_text(doc: &Doc, b: usize) -> String {
    match doc.script.blocks.get(b) {
        Some(Block::Line { segments, .. }) => segments
            .iter()
            .filter_map(|s| match s {
                Segment::Text(t) => Some(t.as_str()),
                Segment::Cue(_) => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
        Some(Block::Heading { text, .. }) => text.clone(),
        _ => String::new(),
    }
}

/// Every cut, in order.
pub fn list(doc: &Doc) -> Vec<CutInfo> {
    let (_, all) = timeline_cuts(doc);
    all.iter()
        .enumerate()
        .map(|(i, c)| CutInfo {
            cut: i,
            at: c.at,
            transition: c.label(),
            duration: c.duration(),
            source: match c.source {
                Source::Divider(_) => "divider",
                Source::LineCue => "line",
                Source::Default => "none",
            },
            section_change: c.section_change,
            unset: c.unset(),
            from_section: c.out_section.clone(),
            to_section: c.in_section.clone(),
            from_line: block_text(doc, c.out_block),
            to_line: block_text(doc, c.in_block),
        })
        .collect()
}

/// Section changes with no transition chosen yet.
pub fn checks(doc: &Doc) -> Vec<Check> {
    list(doc)
        .into_iter()
        .filter(|c| c.unset)
        .map(|c| Check {
            stage: "transitions",
            cut: c.cut,
            message: format!(
                "No transition into \"{}\" (plain cut at {:.1}s)",
                c.to_section.as_deref().unwrap_or("the next section"),
                c.at
            ),
        })
        .collect()
}

fn nth(doc: &Doc, n: usize) -> Result<Cut> {
    let (_, all) = timeline_cuts(doc);
    let count = all.len();
    all.into_iter().nth(n).ok_or_else(|| {
        OpError::NotFound(format!(
            "There is no cut {n}; this script has {count} (0–{}).",
            count.saturating_sub(1)
        ))
    })
}

fn failed(what: &str) -> OpError {
    OpError::Invalid(format!(
        "Couldn't {what}: the script didn't change as expected."
    ))
}

/// Use transition `label` at cut `n`.
pub fn set(doc: &Doc, n: usize, label: &str) -> Result<Plan> {
    let label = label.trim();
    if label.is_empty() {
        return Err(OpError::Invalid(
            "Give the transition a name, e.g. whip-pan.".into(),
        ));
    }
    let c = nth(doc, n)?;
    let out =
        cuts::set(&doc.text, &doc.script, &c, label).ok_or_else(|| failed("set the transition"))?;
    Ok(doc.plan(out, format!("Set cut {n} to {label}")))
}

/// Make cut `n` a plain cut again.
pub fn clear(doc: &Doc, n: usize) -> Result<Plan> {
    let c = nth(doc, n)?;
    let out =
        cuts::clear(&doc.text, &doc.script, &c).ok_or_else(|| failed("clear the transition"))?;
    Ok(doc.plan(out, format!("Make cut {n} a plain cut")))
}

/// Set (or with `None`, remove) the duration of the transition at cut `n`.
pub fn set_duration(doc: &Doc, n: usize, seconds: Option<f32>) -> Result<Plan> {
    if seconds.is_some_and(|s| !(s > 0.0 && s <= 10.0)) {
        return Err(OpError::Invalid(
            "Duration must be between 0 and 10 seconds.".into(),
        ));
    }
    let c = nth(doc, n)?;
    if c.cue.is_none() {
        return Err(OpError::Invalid(format!(
            "Cut {n} is a plain cut; set a transition before giving it a duration."
        )));
    }
    let out =
        cuts::set_duration(&doc.text, &c, seconds).ok_or_else(|| failed("set the duration"))?;
    let what = seconds.map_or("remove its duration".to_owned(), |s| format!("{s}s"));
    Ok(doc.plan(out, format!("Cut {n}: {what}")))
}

/// Use `label` at every unset section change, as one change (one undo).
pub fn fill_unset(doc: &Doc, label: &str) -> Result<Plan> {
    let label = label.trim();
    if label.is_empty() {
        return Err(OpError::Invalid(
            "Give the transition a name, e.g. whip-pan.".into(),
        ));
    }
    // One at a time from the end: setting a cut adds a divider line, which doesn't change
    // how many cuts there are, so earlier numbers stay valid.
    let mut text = doc.text.clone();
    let mut filled = 0;
    for n in (0..timeline_cuts(doc).1.len()).rev() {
        let d = Doc::from_text(&doc.rel, text.clone());
        let (_, all) = timeline_cuts(&d);
        if let Some(c) = all.get(n).filter(|c| c.unset()) {
            text = cuts::set(&d.text, &d.script, c, label)
                .ok_or_else(|| failed("fill the transitions"))?;
            filled += 1;
        }
    }
    Ok(doc.plan(
        text,
        format!(
            "Use {label} at {filled} unset section change{}",
            if filled == 1 { "" } else { "s" }
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::project;
    use crate::{Origin, apply, undo};

    const SRC: &str = "## Cold Open\n[ON SCREEN: black]\nyou'd think it would help\n## Chapter One\nI'm a circuit racer\n## Chapter Two\nthen it rained\n";

    #[test]
    fn lists_and_checks_unset_section_changes() {
        let doc = Doc::from_text("script/script.md", SRC.into());
        let cuts = list(&doc);
        assert_eq!(cuts.len(), 3);
        assert!(!cuts[0].section_change);
        assert!(cuts[1].unset && cuts[2].unset);
        assert_eq!(cuts[1].to_section.as_deref(), Some("Chapter One"));
        assert_eq!(cuts[1].to_line, "I'm a circuit racer");
        let checks = checks(&doc);
        assert_eq!(checks.iter().map(|c| c.cut).collect::<Vec<_>>(), [1, 2]);
        assert!(checks[0].message.contains("Chapter One"));
    }

    #[test]
    fn fill_is_one_change_and_one_undo() {
        let p = project("fill", SRC);
        let doc = Doc::main(&p).unwrap();
        let plan = fill_unset(&doc, "whip-pan").unwrap();
        assert_eq!(plan.summary, "Use whip-pan at 2 unset section changes");
        assert_eq!(
            plan.diff().matches("+:transition[whip-pan]").count(),
            2,
            "{}",
            plan.diff()
        );
        apply(&p, &plan, Origin::Cli).unwrap();
        let after = Doc::main(&p).unwrap();
        assert!(checks(&after).is_empty());
        assert_eq!(list(&after)[1].transition, "whip-pan");
        undo(&p).unwrap();
        assert_eq!(p.read_doc("script/script.md").unwrap(), SRC);
    }

    #[test]
    fn set_duration_and_clear() {
        let doc = Doc::from_text("script/script.md", SRC.into());
        let d1 = Doc::from_text(&doc.rel, set(&doc, 1, "zoom-punch").unwrap().text);
        let d2 = Doc::from_text(&doc.rel, set_duration(&d1, 1, Some(0.4)).unwrap().text);
        assert_eq!(list(&d2)[1].duration, Some(0.4));
        assert_eq!(clear(&d2, 1).unwrap().text, SRC);
    }

    #[test]
    fn bad_requests_explain_themselves() {
        let doc = Doc::from_text("script/script.md", SRC.into());
        assert!(
            set(&doc, 9, "x")
                .unwrap_err()
                .to_string()
                .contains("no cut 9; this script has 3")
        );
        assert!(set(&doc, 1, "  ").is_err());
        assert!(
            set_duration(&doc, 1, Some(0.4))
                .unwrap_err()
                .to_string()
                .contains("plain cut")
        );
        assert!(set_duration(&doc, 1, Some(-1.0)).is_err());
    }
}
