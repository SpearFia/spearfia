//! Every cut in the script: the boundary between two consecutive lines that take time
//! (VO, live and on-screen lines), with the transition used there and where it came from.
//!
//! A transition comes from a divider line (`:transition[whip-pan]{duration=0.3 #t1}` on its
//! own line between the two), or from a `:transition` cue at the end of the outgoing line.
//! Without either, the cut is a plain cut.

use crate::edit::{self, next_id};
use crate::format::{insert_line_after, line_range};
use crate::timing::Timeline;
use crate::{Block, Directive, Script, Segment};

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// A divider line (block index).
    Divider(usize),
    /// A `:transition` cue at the end of the outgoing line.
    LineCue,
    /// No transition written: a plain cut.
    Default,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cut {
    pub out_block: usize,
    pub in_block: usize,
    /// When the cut happens (end of the outgoing line), seconds.
    pub at: f32,
    /// The cut crosses a `##` section heading.
    pub section_change: bool,
    pub out_section: Option<String>,
    pub in_section: Option<String>,
    pub cue: Option<Directive>,
    pub source: Source,
}

impl Cut {
    /// A section change with no transition written: the ones worth deciding.
    pub fn unset(&self) -> bool {
        self.section_change && self.source == Source::Default
    }

    /// The transition's name, `cut` when none is written.
    pub fn label(&self) -> String {
        self.cue
            .as_ref()
            .and_then(|c| c.label.clone())
            .unwrap_or_else(|| "cut".to_owned())
    }

    /// `duration=` on the cue, in seconds.
    pub fn duration(&self) -> Option<f32> {
        self.cue.as_ref()?.attr("duration")?.parse().ok()
    }
}

fn line_of(b: &Block) -> usize {
    match b {
        Block::Heading { line, .. } | Block::Line { line, .. } | Block::Divider { line, .. } => {
            *line
        }
    }
}

/// All cuts, in order.
pub fn cuts(script: &Script, tl: &Timeline) -> Vec<Cut> {
    // Lines that take time.
    let timed: Vec<usize> = script
        .blocks
        .iter()
        .enumerate()
        .filter(|(i, b)| {
            matches!(b, Block::Line { .. }) && tl.blocks.get(*i).is_some_and(|s| !s.is_empty())
        })
        .map(|(i, _)| i)
        .collect();
    let section_title = |block: usize| tl.section_of(block).map(|s| s.title.clone());
    timed
        .windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let between = &script.blocks[a + 1..b];
            let divider = between
                .iter()
                .enumerate()
                .rev()
                .find_map(|(k, blk)| match blk {
                    Block::Divider { cue, .. } => Some((a + 1 + k, cue.clone())),
                    _ => None,
                });
            let line_cue = match &script.blocks[a] {
                Block::Line { segments, .. } => segments.iter().rev().find_map(|s| match s {
                    Segment::Cue(d) if d.name == "transition" => Some(d.clone()),
                    _ => None,
                }),
                _ => None,
            };
            let (cue, source) = match (divider, line_cue) {
                (Some((k, d)), _) => (Some(d), Source::Divider(k)),
                (None, Some(d)) => (Some(d), Source::LineCue),
                (None, None) => (None, Source::Default),
            };
            Cut {
                out_block: a,
                in_block: b,
                at: tl.blocks[a].end,
                section_change: between
                    .iter()
                    .any(|blk| matches!(blk, Block::Heading { level, .. } if *level >= 2)),
                out_section: section_title(a),
                in_section: section_title(b),
                cue,
                source,
            }
        })
        .collect()
}

/// Use `label` at this cut: change the existing transition, or add a divider line after
/// the outgoing line.
pub fn set(src: &str, script: &Script, cut: &Cut, label: &str) -> Option<String> {
    let label = edit::clean_label(label);
    match &cut.cue {
        Some(d) => {
            let mut nd = d.clone();
            nd.label = Some(label);
            edit::replace_cue(src, d, &nd)
        }
        None => {
            let id = next_id(script, "t");
            // After the last line before the incoming one, so a heading stays below it.
            let out_line = line_of(script.blocks.get(cut.out_block)?);
            let (out, _) =
                insert_line_after(src, out_line, &format!(":transition[{label}]{{#{id}}}"));
            Some(out)
        }
    }
}

/// Back to a plain cut: remove the divider line (or the cue on the outgoing line).
pub fn clear(src: &str, script: &Script, cut: &Cut) -> Option<String> {
    match (&cut.source, &cut.cue) {
        (Source::Divider(k), _) => {
            let line = line_of(script.blocks.get(*k)?);
            let r = line_range(src, line)?;
            // Take the line ending too.
            let end = if src[r.end..].starts_with("\r\n") {
                r.end + 2
            } else if src[r.end..].starts_with('\n') {
                r.end + 1
            } else {
                r.end
            };
            Some(format!("{}{}", &src[..r.start], &src[end..]))
        }
        (Source::LineCue, Some(d)) => edit::remove_cue(src, d),
        _ => Some(src.to_owned()),
    }
}

/// Set (or remove) `duration=` on the cut's transition.
pub fn set_duration(src: &str, cut: &Cut, seconds: Option<f32>) -> Option<String> {
    let d = cut.cue.as_ref()?;
    let mut nd = d.clone();
    nd.attrs.retain(|(k, _)| k != "duration");
    if let Some(s) = seconds {
        let v = format!("{s:.2}");
        let v = v.trim_end_matches('0').trim_end_matches('.');
        nd.attrs.insert(0, ("duration".into(), v.to_owned()));
    }
    edit::replace_cue(src, d, &nd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, timing::timeline};

    const SRC: &str = "## Cold Open\n[ON SCREEN: black]\nyou'd think it would help\n:transition[hard cut]{#t1}\n[ON SCREEN: words slam]\nand it almost made me quit :transition[smash cut]{#t2}\n## Chapter One\n> a note, not timed\nI'm a circuit racer\n";

    fn all() -> (Script, Vec<Cut>) {
        let s = parse(SRC);
        let tl = timeline(&s);
        let c = cuts(&s, &tl);
        (s, c)
    }

    #[test]
    fn finds_cuts_and_sources() {
        let (_, c) = all();
        assert_eq!(c.len(), 4);
        assert_eq!(c[0].source, Source::Default);
        assert_eq!(c[0].label(), "cut");
        assert!(matches!(c[1].source, Source::Divider(_)));
        assert_eq!(c[1].label(), "hard cut");
        assert_eq!(c[2].label(), "cut");
        assert_eq!(c[3].source, Source::LineCue);
        assert_eq!(c[3].label(), "smash cut");
        assert!(c[3].section_change);
        assert_eq!(c[3].in_section.as_deref(), Some("Chapter One"));
        assert!(!c[3].unset(), "has a cue");
        assert!(c.iter().all(|x| x.at > 0.0));
    }

    #[test]
    fn edits_transitions() {
        let (s, c) = all();
        let out = set(SRC, &s, &c[0], "whip-pan").unwrap();
        assert!(out.contains("[ON SCREEN: black]\n:transition[whip-pan]{#t3}\nyou'd"));
        let out = set(SRC, &s, &c[1], "zoom-punch").unwrap();
        assert!(out.contains(":transition[zoom-punch]{#t1}"));
        let out = clear(SRC, &s, &c[1]).unwrap();
        assert!(!out.contains("hard cut"));
        assert!(out.contains("help\n[ON SCREEN: words slam]"));
        let out = clear(SRC, &s, &c[3]).unwrap();
        assert!(out.contains("almost made me quit\n## Chapter One"));
        let out = set_duration(SRC, &c[1], Some(0.4)).unwrap();
        assert!(out.contains(":transition[hard cut]{duration=0.4 #t1}"));
        let s2 = parse(&out);
        let c2 = cuts(&s2, &timeline(&s2));
        assert_eq!(c2[1].duration(), Some(0.4));
        let back = set_duration(&out, &c2[1], None).unwrap();
        assert_eq!(back, SRC);
    }
}
