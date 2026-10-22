//! Sound effects: every `:sfx[...]` cue, where it sits (on a word, at a cut, with a section)
//! and when it plays, plus the words of the script on the timeline so a cue can be dropped
//! on one.
//!
//! A cue plays where it is written: before a spoken word, at that word; at the start of a
//! line, at the cut into it; on a `##` heading, as the section starts; on an on-screen line,
//! as the line starts.

use crate::edit::{self, next_id};
use crate::format::{line_range, word_ranges};
use crate::timing::Timeline;
use crate::{Block, Directive, LineKind, Script, Segment};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub enum Place {
    /// Before word `index` of a spoken line (`index` > 0).
    Word { index: usize, word: String },
    /// Before the first word: on the cut into the line.
    Cut,
    /// After the last word.
    LineEnd,
    /// On an on-screen line or a note: as it starts.
    OnScreen,
    /// On a heading: as the section starts.
    Section,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sfx {
    pub cue: Directive,
    pub block: usize,
    pub at: f32,
    pub place: Place,
}

impl Sfx {
    pub fn label(&self) -> String {
        self.cue.label.clone().unwrap_or_else(|| "sfx".to_owned())
    }
}

/// One spoken word on the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub block: usize,
    pub index: usize,
    pub text: String,
    pub start: f32,
    pub end: f32,
}

/// Where to put a cue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Before word `index` of the line on source line `line`; `index` = word count puts it
    /// at the end.
    Word { line: usize, index: usize },
    /// At the start of a block (the cut into a line, a heading, an on-screen line).
    Start { line: usize },
}

fn line_of(b: &Block) -> usize {
    match b {
        Block::Heading { line, .. } | Block::Line { line, .. } | Block::Divider { line, .. } => {
            *line
        }
    }
}

/// Every sound effect, in script order.
pub fn sfx(script: &Script, tl: &Timeline) -> Vec<Sfx> {
    let mut out = Vec::new();
    for m in &tl.stingers {
        let Some(block) = script.blocks.get(m.block) else {
            continue;
        };
        let place = match block {
            Block::Heading { .. } | Block::Divider { .. } => Place::Section,
            Block::Line { kind, segments, .. } => {
                if !kind.is_spoken() {
                    Place::OnScreen
                } else {
                    let mut before = 0usize;
                    let mut next_word = None;
                    let mut found = false;
                    for s in segments {
                        match s {
                            Segment::Cue(d) if d.span == m.cue.span => found = true,
                            Segment::Cue(_) => {}
                            Segment::Text(t) => {
                                for w in crate::words_in(t) {
                                    if found {
                                        if next_word.is_none() {
                                            next_word = Some(clean(w));
                                        }
                                    } else {
                                        before += 1;
                                    }
                                }
                            }
                        }
                    }
                    let total = crate::word_count(segments);
                    match next_word {
                        _ if before == 0 => Place::Cut,
                        _ if before >= total => Place::LineEnd,
                        Some(w) => Place::Word {
                            index: before,
                            word: w,
                        },
                        None => Place::LineEnd,
                    }
                }
            }
        };
        out.push(Sfx {
            cue: m.cue.clone(),
            block: m.block,
            at: m.at,
            place,
        });
    }
    out
}

fn clean(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
        .to_owned()
}

/// The spoken words of the script with their estimated times.
pub fn words(script: &Script, tl: &Timeline) -> Vec<Word> {
    let mut out = Vec::new();
    for (b, block) in script.blocks.iter().enumerate() {
        let Block::Line { kind, segments, .. } = block else {
            continue;
        };
        if !kind.is_spoken() {
            continue;
        }
        // A timeline from another script: skip lines it doesn't cover.
        let Some(&span) = tl.blocks.get(b) else {
            continue;
        };
        let all: Vec<&str> = segments
            .iter()
            .filter_map(|s| match s {
                Segment::Text(t) => Some(t.as_str()),
                Segment::Cue(_) => None,
            })
            .flat_map(crate::words_in)
            .collect();
        let n = all.len().max(1) as f32;
        for (i, w) in all.iter().enumerate() {
            out.push(Word {
                block: b,
                index: i,
                text: (*w).to_owned(),
                start: span.start + span.len() * i as f32 / n,
                end: span.start + span.len() * (i + 1) as f32 / n,
            });
        }
    }
    out
}

/// Byte range of each word token, including inline marks around it (`**bold**`), so a cue
/// inserted at its start goes outside the marks.
fn tokens(src: &str, script: &Script, block: usize) -> Vec<Range<usize>> {
    let opens: Vec<Range<usize>> = match script.blocks.get(block) {
        Some(Block::Line { line, .. }) => line_range(src, *line)
            .map(|lr| {
                crate::attrs::spans(&src[lr.clone()], lr.start)
                    .into_iter()
                    .map(|s| s.open)
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    word_ranges(src, script, block)
        .into_iter()
        .map(|r| {
            let mut s = r.start;
            loop {
                if s > 0 && matches!(src.as_bytes()[s - 1], b'*' | b'+' | b'~' | b'`') {
                    s -= 1;
                } else if let Some(o) = opens.iter().find(|o| o.end == s) {
                    // The first word of a `[styled words]{...}` span: go before the `[`.
                    s = o.start;
                } else {
                    break;
                }
            }
            s..r.end
        })
        .collect()
}

/// Insert `cue` (source text) at a target.
fn insert(src: &str, script: &Script, target: Target, cue: &str) -> Option<String> {
    match target {
        Target::Start { line } => {
            let b = script.blocks.iter().position(|b| line_of(b) == line)?;
            edit::add_cue(src, script, b, cue)
        }
        Target::Word { line, index } => {
            let b = script.blocks.iter().position(|b| line_of(b) == line)?;
            match &script.blocks[b] {
                Block::Line { kind, .. } if kind.is_spoken() => {}
                _ => return edit::add_cue(src, script, b, cue),
            }
            let toks = tokens(src, script, b);
            if index == 0 || toks.is_empty() {
                return edit::add_cue(src, script, b, cue);
            }
            if let Some(t) = toks.get(index) {
                return Some(format!("{}{cue} {}", &src[..t.start], &src[t.start..]));
            }
            // After the last word, before a paragraph override.
            let lr = line_range(src, line)?;
            let text = &src[lr.clone()];
            let body = text.trim_end_matches(['\n', '\r']);
            let end = match crate::attrs::para_attrs(body) {
                Some((r, _)) => lr.start + body[..r.start].trim_end().len(),
                None => lr.start + body.trim_end().len(),
            };
            Some(format!("{} {cue}{}", &src[..end], &src[end..]))
        }
    }
}

/// Add `:sfx[label]` at a target.
pub fn add(src: &str, script: &Script, target: Target, label: &str) -> Option<String> {
    let id = next_id(script, "s");
    let cue = format!(":sfx[{}]{{#{id}}}", edit::clean_label(label));
    insert(src, script, target, &cue)
}

pub fn remove(src: &str, s: &Sfx) -> Option<String> {
    edit::remove_cue(src, &s.cue)
}

pub fn set_label(src: &str, s: &Sfx, label: &str) -> Option<String> {
    let mut d = s.cue.clone();
    d.label = Some(edit::clean_label(label));
    edit::replace_cue(src, &s.cue, &d)
}

/// Move the cue (keeping its label, attributes and id) to a target.
pub fn move_to(src: &str, s: &Sfx, target: Target) -> Option<String> {
    let removed = edit::remove_cue(src, &s.cue)?;
    let parsed = crate::parse(&removed);
    insert(&removed, &parsed, target, &s.cue.to_source())
}

/// Where a cue sits now, as a target (for nudging it a word left or right).
/// `None` if `s` is not from this script (its block is out of range).
pub fn target_of(script: &Script, s: &Sfx) -> Option<Target> {
    let block = script.blocks.get(s.block)?;
    let line = line_of(block);
    Some(match &s.place {
        Place::Word { index, .. } => Target::Word {
            line,
            index: *index,
        },
        Place::LineEnd => Target::Word {
            line,
            index: match block {
                Block::Line { segments, .. } => crate::word_count(segments),
                _ => 0,
            },
        },
        _ => Target::Start { line },
    })
}

/// The next spot one word earlier (`-1`) or later (`+1`), crossing into the line before or
/// after when it runs off the end.
pub fn nudge(script: &Script, s: &Sfx, dir: i32) -> Option<Target> {
    let spoken = |b: &Block| matches!(b, Block::Line { kind, .. } if kind.is_spoken());
    let count = |b: &Block| match b {
        Block::Line { segments, .. } => crate::word_count(segments),
        _ => 0,
    };
    let b = s.block;
    let block = script.blocks.get(b)?;
    let (line, index) = match target_of(script, s)? {
        Target::Word { line, index } => (line, index),
        Target::Start { .. } if spoken(block) => (line_of(block), 0),
        Target::Start { .. } => return None,
    };
    let n = count(block);
    if dir < 0 {
        if index > 0 {
            return Some(Target::Word {
                line,
                index: index - 1,
            });
        }
        let prev = script.blocks[..b].iter().rposition(spoken)?;
        let pn = count(&script.blocks[prev]);
        Some(Target::Word {
            line: line_of(&script.blocks[prev]),
            index: pn.saturating_sub(1),
        })
    } else {
        if index < n {
            return Some(Target::Word {
                line,
                index: index + 1,
            });
        }
        let next = b + 1 + script.blocks[b + 1..].iter().position(spoken)?;
        Some(Target::Word {
            line: line_of(&script.blocks[next]),
            index: 0,
        })
    }
}

/// Is the line spoken (has word-level timing)?
pub fn is_spoken(script: &Script, block: usize) -> bool {
    matches!(script.blocks.get(block), Some(Block::Line { kind, .. }) if kind.is_spoken() && *kind != LineKind::Note)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, timing::timeline};

    const SRC: &str = "## Cold Open :sfx[riser]{#s1}\n[ON SCREEN: black] :sfx[boom]{#s2}\n:sfx[whoosh]{#s3} you'd think it **would** help\nand it almost :sfx[scratch]{#s4} made me quit :sfx[thud]{#s5}\n";

    fn all() -> (Script, Timeline, Vec<Sfx>) {
        let s = parse(SRC);
        let tl = timeline(&s);
        let x = sfx(&s, &tl);
        (s, tl, x)
    }

    #[test]
    fn places_and_times() {
        let (s, tl, x) = all();
        assert_eq!(x.len(), 5);
        assert_eq!(x[0].place, Place::Section);
        assert_eq!(x[1].place, Place::OnScreen);
        assert_eq!(x[2].place, Place::Cut);
        assert_eq!(
            x[3].place,
            Place::Word {
                index: 3,
                word: "made".into()
            }
        );
        assert_eq!(x[4].place, Place::LineEnd);
        let w = words(&s, &tl);
        assert_eq!(w.len(), 11);
        let made = w.iter().find(|w| w.text == "made").unwrap();
        assert!((made.start - x[3].at).abs() < 1e-4, "cue lands on its word");
        assert!((x[4].at - tl.blocks[x[4].block].end).abs() < 1e-4);
    }

    /// The example script in `spearfia-spec` (`examples/script/script.md`), which shows each
    /// placement in `docs/script-directives.md`.
    #[test]
    fn spec_example_places() {
        // Pinned byte-for-byte; see `tests/fixtures/README.md`.
        let src = include_str!("../tests/fixtures/spec-example-script.md");
        assert!(src.ends_with(":look[facecam-pip]{position=bottom-left #l1}\n"));
        let s = parse(src);
        let tl = timeline(&s);
        let x = sfx(&s, &tl);
        let places: Vec<&Place> = x.iter().map(|s| &s.place).collect();
        assert_eq!(
            places,
            vec![
                &Place::LineEnd,
                &Place::OnScreen,
                &Place::Cut,
                &Place::Word {
                    index: 4,
                    word: "eight".into()
                },
                &Place::Section,
            ]
        );
        // The line end and the cut into the next line are the same moment.
        assert!((x[0].at - x[1].at).abs() < 1e-4);
    }

    #[test]
    fn own_line_plays_on_the_cut() {
        let s = parse("one two three\n:sfx[whoosh]{#s1}\nfour five\n");
        let tl = timeline(&s);
        let x = sfx(&s, &tl);
        assert!((x[0].at - tl.blocks[0].end).abs() < 1e-4);
        assert!((x[0].at - tl.blocks[2].start).abs() < 1e-4);
    }

    #[test]
    fn adds_and_moves() {
        let (s, _, x) = all();
        // Before "would", outside the bold marks.
        let out = add(SRC, &s, Target::Word { line: 2, index: 3 }, "ding").unwrap();
        assert!(out.contains("it :sfx[ding]{#s6} **would** help"));
        // At the end of a line.
        let out = add(SRC, &s, Target::Word { line: 2, index: 5 }, "ding").unwrap();
        assert!(out.contains("**would** help :sfx[ding]{#s6}\n"));
        // At the start of a line (the cut into it).
        let out = add(SRC, &s, Target::Start { line: 3 }, "whip").unwrap();
        assert!(out.contains("\n:sfx[whip]{#s6} and it almost"));
        // Move the scratch two words on.
        let out = move_to(SRC, &x[3], Target::Word { line: 3, index: 5 }).unwrap();
        assert!(out.contains("and it almost made me :sfx[scratch]{#s4} quit :sfx[thud]{#s5}"));
        let p = parse(&out);
        let y = sfx(&p, &timeline(&p));
        assert_eq!(
            y[3].place,
            Place::Word {
                index: 5,
                word: "quit".into()
            }
        );
        // Nudge left from the start of a line goes to the last word of the line before.
        assert_eq!(
            nudge(&s, &x[3], -1),
            Some(Target::Word { line: 3, index: 2 })
        );
        assert_eq!(nudge(&s, &x[2], -1), None, "the line before is on-screen");
        assert_eq!(nudge(&s, &x[4], 1), None, "last line");
        // Before a styled span, outside its brackets.
        let src2 = "I spent [eight hours]{color=red} learning\n";
        let p2 = parse(src2);
        let out = add(src2, &p2, Target::Word { line: 0, index: 2 }, "ding").unwrap();
        assert_eq!(
            out,
            "I spent :sfx[ding]{#s1} [eight hours]{color=red} learning\n"
        );
        let out = set_label(SRC, &x[1], "impact").unwrap();
        assert!(out.contains(":sfx[impact]{#s2}"));
        let out = remove(SRC, &x[3]).unwrap();
        assert!(out.contains("and it almost made me quit"));
    }

    #[test]
    fn mismatched_timeline_does_not_panic() {
        let s = parse("one two");
        let empty = timeline(&parse(""));
        assert!(words(&s, &empty).is_empty());
        let other = timeline(&parse("a :sfx[x]{#s1}\nb :sfx[y]{#s2}\n"));
        assert!(sfx(&parse(""), &other).is_empty());
    }

    #[test]
    fn styled_punctuation_is_not_a_word() {
        let plain = parse("I word, next :sfx[ding]{#s1}\n");
        let styled = parse("I [word]{color=red}, next :sfx[ding]{#s1}\n");
        let (tp, ts) = (timeline(&plain), timeline(&styled));
        assert_eq!(tp.stingers[0].at, ts.stingers[0].at);
        assert!((ts.stingers[0].at - 1.2).abs() < 1e-4);
        assert_eq!(words(&styled, &ts).len(), 3);
        let marks = parse("say ** ** :sfx[a]{#s1}\n");
        assert_eq!(words(&marks, &timeline(&marks)).len(), 1);
    }
}
