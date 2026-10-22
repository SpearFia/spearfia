//! Effects and looks: every `:look[...]` cue, the lines it covers and when it runs.
//!
//! Where a look is written decides what it covers:
//!
//! - On a line that takes time: that line, or `lines=N` lines starting there.
//! - On a `##` heading: the whole section, up to the next heading of the same level or above.
//! - On a line of its own (or a note): from there to the end of the section.

use crate::edit::{self, next_id};
use crate::format::line_range;
use crate::timing::{Span, Timeline};
use crate::{Block, Directive, Script, Segment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The line it is written on.
    Line,
    /// `lines=N`: this many lines that take time, starting with the one it is written on.
    Lines(usize),
    /// Written on a heading: the whole section.
    Section,
    /// Written on a line of its own: from there to the end of the section.
    FromHere,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub cue: Directive,
    /// The block the cue is written in.
    pub block: usize,
    pub scope: Scope,
    /// Lines that take time and that this look covers, in order.
    pub covers: Vec<usize>,
    /// When it runs. Empty when it covers nothing.
    pub span: Span,
}

impl Look {
    pub fn label(&self) -> String {
        self.cue.label.clone().unwrap_or_else(|| "look".to_owned())
    }

    /// The cue's attributes that are not about scope (`lines=`).
    pub fn params(&self) -> impl Iterator<Item = &(String, String)> {
        self.cue.attrs.iter().filter(|(k, _)| k != "lines")
    }
}

pub fn line_of(b: &Block) -> usize {
    match b {
        Block::Heading { line, .. } | Block::Line { line, .. } | Block::Divider { line, .. } => {
            *line
        }
    }
}

/// Blocks that take time, in order.
pub fn timed(script: &Script, tl: &Timeline) -> Vec<usize> {
    script
        .blocks
        .iter()
        .enumerate()
        .filter(|(i, b)| {
            matches!(b, Block::Line { .. }) && tl.blocks.get(*i).is_some_and(|s| !s.is_empty())
        })
        .map(|(i, _)| i)
        .collect()
}

/// Index of the first block after `from` that is a heading of `level` or above (or the end).
fn section_end(script: &Script, from: usize, level: u8) -> usize {
    script
        .blocks
        .get(from + 1..)
        .unwrap_or_default()
        .iter()
        .position(|b| matches!(b, Block::Heading { level: l, .. } if *l <= level))
        .map_or(script.blocks.len(), |k| from + 1 + k)
}

/// All looks, in the order they are written.
pub fn looks(script: &Script, tl: &Timeline) -> Vec<Look> {
    let timed = timed(script, tl);
    let mut out = Vec::new();
    for (b, block) in script.blocks.iter().enumerate() {
        let cues: Vec<&Directive> = match block {
            Block::Heading { cues, .. } => cues.iter().collect(),
            Block::Line { segments, .. } => segments
                .iter()
                .filter_map(|s| match s {
                    Segment::Cue(d) => Some(d),
                    Segment::Text(_) => None,
                })
                .collect(),
            Block::Divider { .. } => Vec::new(),
        };
        for d in cues.into_iter().filter(|d| d.name == "look") {
            let is_timed = timed.contains(&b);
            let (scope, covers): (Scope, Vec<usize>) = match block {
                Block::Heading { level, .. } => {
                    let end = section_end(script, b, *level);
                    (
                        Scope::Section,
                        timed
                            .iter()
                            .copied()
                            .filter(|&t| t > b && t < end)
                            .collect(),
                    )
                }
                _ if is_timed => {
                    let n = d
                        .attr("lines")
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(1)
                        .max(1);
                    let start = timed.iter().position(|&t| t == b).unwrap_or(0);
                    let covers: Vec<usize> = timed[start..].iter().take(n).copied().collect();
                    (if n > 1 { Scope::Lines(n) } else { Scope::Line }, covers)
                }
                _ => {
                    let end = section_end(script, b, 2);
                    (
                        Scope::FromHere,
                        timed
                            .iter()
                            .copied()
                            .filter(|&t| t > b && t < end)
                            .collect(),
                    )
                }
            };
            let span = match (
                covers.first().and_then(|&a| tl.blocks.get(a)),
                covers.last().and_then(|&z| tl.blocks.get(z)),
            ) {
                (Some(a), Some(z)) => Span {
                    start: a.start,
                    end: z.end,
                },
                _ => Span::default(),
            };
            out.push(Look {
                cue: d.clone(),
                block: b,
                scope,
                covers,
                span,
            });
        }
    }
    out
}

/// Looks that cover this block.
pub fn over(looks: &[Look], block: usize) -> impl Iterator<Item = &Look> {
    looks.iter().filter(move |l| l.covers.contains(&block))
}

/// Add `:look[label]` to a block (a line or a heading).
pub fn add(src: &str, script: &Script, block: usize, label: &str) -> Option<String> {
    let id = next_id(script, "l");
    let label = edit::clean_label(label);
    edit::add_cue(src, script, block, &format!(":look[{label}]{{#{id}}}"))
}

/// Remove the look. A line that held nothing but this cue goes too. Returns the new source
/// and the line removed, if one was.
fn remove_inner(src: &str, script: &Script, look: &Look) -> Option<(String, Option<usize>)> {
    if let Some(Block::Line { segments, line, .. }) = script.blocks.get(look.block) {
        let alone = segments.iter().all(|s| match s {
            Segment::Text(t) => t.trim().is_empty(),
            Segment::Cue(d) => d.span == look.cue.span,
        });
        if alone {
            let r = line_range(src, *line)?;
            let end = if src[r.end..].starts_with("\r\n") {
                r.end + 2
            } else if src[r.end..].starts_with('\n') {
                r.end + 1
            } else {
                r.end
            };
            return Some((format!("{}{}", &src[..r.start], &src[end..]), Some(*line)));
        }
    }
    Some((edit::remove_cue(src, &look.cue)?, None))
}

pub fn remove(src: &str, script: &Script, look: &Look) -> Option<String> {
    remove_inner(src, script, look).map(|(s, _)| s)
}

/// Rename the look (keeps its attributes and id).
pub fn set_label(src: &str, look: &Look, label: &str) -> Option<String> {
    let mut d = look.cue.clone();
    d.label = Some(edit::clean_label(label));
    edit::replace_cue(src, &look.cue, &d)
}

/// Set (or remove, with `None`) one attribute on the look.
pub fn set_attr(src: &str, look: &Look, key: &str, value: Option<&str>) -> Option<String> {
    let mut d = look.cue.clone();
    match (d.attrs.iter().position(|(k, _)| k == key), value) {
        (Some(i), Some(v)) => d.attrs[i].1 = v.to_owned(),
        (Some(i), None) => {
            d.attrs.remove(i);
        }
        (None, Some(v)) => d.attrs.push((key.to_owned(), v.to_owned())),
        (None, None) => return Some(src.to_owned()),
    }
    edit::replace_cue(src, &look.cue, &d)
}

/// The heading of the section a block is in (the nearest `##` or deeper heading above it).
pub fn section_heading(script: &Script, block: usize) -> Option<usize> {
    let end = block.saturating_add(1).min(script.blocks.len());
    script.blocks[..end]
        .iter()
        .rposition(|b| matches!(b, Block::Heading { level, .. } if *level >= 2))
}

/// Change what the look covers: move the cue to the line or heading that gives that scope.
/// `FromHere` can't be chosen; it is only read.
pub fn set_scope(src: &str, script: &Script, look: &Look, scope: Scope) -> Option<String> {
    let first = look.covers.first().copied()?;
    let (target, lines) = match scope {
        Scope::Line => (first, None),
        Scope::Lines(n) => (first, (n > 1).then_some(n)),
        Scope::Section => (section_heading(script, first)?, None),
        Scope::FromHere => return None,
    };
    let mut d = look.cue.clone();
    d.attrs.retain(|(k, _)| k != "lines");
    if let Some(n) = lines {
        d.attrs.insert(0, ("lines".into(), n.to_string()));
    }
    if target == look.block {
        return edit::replace_cue(src, &look.cue, &d);
    }
    let target_line = line_of(script.blocks.get(target)?);
    let (removed, gone) = remove_inner(src, script, look)?;
    let target_line = match gone {
        Some(l) if l < target_line => target_line - 1,
        _ => target_line,
    };
    let parsed = crate::parse(&removed);
    let block = parsed
        .blocks
        .iter()
        .position(|b| line_of(b) == target_line)?;
    edit::add_cue(&removed, &parsed, block, &d.to_source())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, timing::timeline};

    const SRC: &str = "## Cold Open :look[grain]{#l1}\n[ON SCREEN: black] :look[title-card]{#l2}\nyou'd think it would help\n:look[facecam-pip]{position=bottom-left #l3}\nand it almost made me quit\n> a note\nreally\n## Chapter One\n:look[zoom]{lines=2 #l4} I'm a circuit racer\nthat was my thing\nand then\n";

    fn all() -> (Script, Timeline, Vec<Look>) {
        let s = parse(SRC);
        let tl = timeline(&s);
        let l = looks(&s, &tl);
        (s, tl, l)
    }

    #[test]
    fn scopes_and_coverage() {
        let (s, tl, l) = all();
        assert_eq!(l.len(), 4);
        let t = timed(&s, &tl);
        assert_eq!(l[0].scope, Scope::Section);
        assert_eq!(l[0].covers.len(), 4, "the four timed lines of Cold Open");
        assert_eq!(l[1].scope, Scope::Line);
        assert_eq!(l[1].covers, vec![t[0]]);
        assert_eq!(l[2].scope, Scope::FromHere);
        assert_eq!(l[2].covers, vec![t[2], t[3]]);
        assert_eq!(l[3].scope, Scope::Lines(2));
        assert_eq!(l[3].covers, vec![t[4], t[5]]);
        assert!(l[3].span.start < l[3].span.end);
        assert_eq!(over(&l, t[2]).count(), 2);
        assert_eq!(l[2].params().count(), 1);
    }

    #[test]
    fn edits_looks() {
        let (s, _, l) = all();
        let out = add(SRC, &s, 2, "Focus Loupe").unwrap();
        assert!(out.contains(":look[Focus Loupe]{#l5} you'd think"));
        let out = remove(SRC, &s, &l[2]).unwrap();
        assert!(out.contains("help\nand it almost"), "own line removed");
        let out = remove(SRC, &s, &l[1]).unwrap();
        assert!(out.contains("[ON SCREEN: black]\n"));
        let out = set_attr(SRC, &l[2], "position", Some("top-right")).unwrap();
        assert!(out.contains(":look[facecam-pip]{position=top-right #l3}"));
        let out = set_label(SRC, &l[1], "chapter-card").unwrap();
        assert!(out.contains(":look[chapter-card]{#l2}"));
        // Line → section: moves onto the heading.
        let out = set_scope(SRC, &s, &l[3], Scope::Section).unwrap();
        assert!(out.contains("## Chapter One :look[zoom]{#l4}\nI'm a circuit racer"));
        // Own line → that line: removes the line and puts the cue on the next timed line.
        let out = set_scope(SRC, &s, &l[2], Scope::Lines(2)).unwrap();
        assert!(
            out.contains(
                "help\n:look[facecam-pip]{lines=2 position=bottom-left #l3} and it almost"
            )
        );
        let p = parse(&out);
        let l2 = looks(&p, &timeline(&p));
        assert_eq!(l2[2].scope, Scope::Lines(2));
        // Section → line.
        let out = set_scope(SRC, &s, &l[0], Scope::Line).unwrap();
        assert!(out.starts_with(
            "## Cold Open\n[ON SCREEN: black] :look[title-card]{#l2} :look[grain]{#l1}"
        ));
    }

    #[test]
    fn empty_and_out_of_range_inputs() {
        let empty = parse("");
        assert_eq!(section_heading(&empty, 0), None);
        assert_eq!(section_heading(&parse("## A\nx\n"), 99), Some(0));
    }
}
