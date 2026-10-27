//! A rough timeline of the script before any footage exists: when each line plays, which
//! music cue covers which part, where the gaps are, and where stingers and transitions
//! land. Spoken lines are timed at [`WORDS_PER_MINUTE`](crate::WORDS_PER_MINUTE); on-screen
//! directions get [`ON_SCREEN_SECONDS`]. Once clips are linked and analysed, their real
//! durations replace these estimates.
//!
//! Music cues (`:music[track]{...}`) start where they are written: on a heading, at the
//! start of the section; on a line, at that word. A cue plays until the next music cue,
//! a `:music[stop]`, or the end of the script.

use crate::{Block, Directive, LineKind, Script, Segment, estimate_seconds};
use std::collections::HashMap;

/// Estimated length of an `[ON SCREEN: ...]` line with no VO under it.
pub const ON_SCREEN_SECONDS: f32 = 2.0;

/// Gaps shorter than this between music cues are ignored.
pub const MIN_GAP_SECONDS: f32 = 0.5;

/// Labels that end the current music without starting a new track.
const STOP_LABELS: [&str; 3] = ["stop", "off", "none"];

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Span {
    pub start: f32,
    pub end: f32,
}

impl Span {
    pub fn len(self) -> f32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.len() <= 0.0
    }

    pub fn overlap(self, other: Span) -> f32 {
        (self.end.min(other.end) - self.start.max(other.start)).max(0.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SectionTime {
    pub title: String,
    pub block: usize,
    pub span: Span,
}

/// One music cue and the stretch of script it covers.
#[derive(Debug, Clone, PartialEq)]
pub struct MusicSpan {
    pub cue: Directive,
    /// Block the cue is written on.
    pub block: usize,
    pub span: Span,
}

impl MusicSpan {
    /// The track the cue names, if any (`:music{mood=tense}` names none yet).
    pub fn track(&self) -> Option<&str> {
        self.cue.label.as_deref()
    }
}

/// A point cue (`:sfx`, `:transition`) at a moment in time.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub cue: Directive,
    pub block: usize,
    pub at: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timeline {
    /// One entry per script block; headings and dividers have zero length.
    pub blocks: Vec<Span>,
    pub total: f32,
    pub sections: Vec<SectionTime>,
    pub music: Vec<MusicSpan>,
    /// Stretches with no music, at least [`MIN_GAP_SECONDS`] long.
    pub gaps: Vec<Span>,
    pub stingers: Vec<Marker>,
    pub transitions: Vec<Marker>,
}

impl Timeline {
    /// Seconds of `span` covered by music.
    pub fn music_over(&self, span: Span) -> f32 {
        self.music.iter().map(|m| m.span.overlap(span)).sum()
    }

    /// Music cues that overlap `span`, in order.
    pub fn music_in(&self, span: Span) -> impl Iterator<Item = &MusicSpan> {
        self.music
            .iter()
            .filter(move |m| m.span.overlap(span) > 0.0)
    }

    /// The section a block belongs to.
    pub fn section_of(&self, block: usize) -> Option<&SectionTime> {
        self.sections.iter().rev().find(|s| s.block <= block)
    }
}

pub fn is_stop(cue: &Directive) -> bool {
    cue.label
        .as_deref()
        .is_some_and(|l| STOP_LABELS.contains(&l.trim().to_ascii_lowercase().as_str()))
}

/// Lay the script out over time.
pub fn timeline(script: &Script) -> Timeline {
    timeline_measured(script, &HashMap::new())
}

/// [`timeline`], with spoken lines timed from `measured` (script line number → seconds of
/// recorded voiceover) where present, and estimated elsewhere.
pub fn timeline_measured(script: &Script, measured: &HashMap<usize, f32>) -> Timeline {
    let mut t = Timeline::default();
    let mut now = 0.0f32;
    // Music cue starts (and stops) in order: (time, block, cue).
    let mut starts: Vec<(f32, usize, Directive)> = Vec::new();

    for (i, b) in script.blocks.iter().enumerate() {
        match b {
            Block::Heading {
                level, text, cues, ..
            } => {
                t.blocks.push(Span {
                    start: now,
                    end: now,
                });
                if *level >= 2 {
                    t.sections.push(SectionTime {
                        title: text.clone(),
                        block: i,
                        span: Span {
                            start: now,
                            end: now,
                        },
                    });
                }
                for c in cues {
                    match c.name.as_str() {
                        "music" => starts.push((now, i, c.clone())),
                        "sfx" => t.stingers.push(Marker {
                            cue: c.clone(),
                            block: i,
                            at: now,
                        }),
                        _ => {}
                    }
                }
            }
            Block::Divider { cue, .. } => {
                t.blocks.push(Span {
                    start: now,
                    end: now,
                });
                t.transitions.push(Marker {
                    cue: cue.clone(),
                    block: i,
                    at: now,
                });
            }
            Block::Line {
                kind,
                segments,
                line,
            } => {
                let words: usize = crate::word_count(segments);
                let len = match kind {
                    LineKind::Spoken | LineKind::Live => measured
                        .get(line)
                        .copied()
                        .unwrap_or_else(|| estimate_seconds(words)),
                    LineKind::OnScreen => ON_SCREEN_SECONDS,
                    LineKind::Note => 0.0,
                };
                // Cues land at their word: spoken words before them / all spoken words.
                let mut before = 0usize;
                for s in segments {
                    match s {
                        Segment::Text(text) => before += crate::words_in(text).count(),
                        Segment::Cue(c) => {
                            let at = match kind {
                                k if k.is_spoken() && words > 0 => {
                                    now + len * before as f32 / words as f32
                                }
                                _ => now,
                            };
                            let marker = Marker {
                                cue: c.clone(),
                                block: i,
                                at,
                            };
                            match c.name.as_str() {
                                "music" => starts.push((at, i, c.clone())),
                                "sfx" => t.stingers.push(marker),
                                "transition" => t.transitions.push(marker),
                                _ => {}
                            }
                        }
                    }
                }
                t.blocks.push(Span {
                    start: now,
                    end: now + len,
                });
                now += len;
            }
        }
    }
    t.total = now;

    for k in 0..t.sections.len() {
        let end = t.sections.get(k + 1).map_or(now, |s| s.span.start);
        t.sections[k].span.end = end;
    }

    for (k, (at, block, cue)) in starts.iter().enumerate() {
        if is_stop(cue) {
            continue;
        }
        let end = starts.get(k + 1).map_or(now, |(next, ..)| *next);
        t.music.push(MusicSpan {
            cue: cue.clone(),
            block: *block,
            span: Span { start: *at, end },
        });
    }

    let mut cursor = 0.0f32;
    for m in &t.music {
        if m.span.start - cursor >= MIN_GAP_SECONDS {
            t.gaps.push(Span {
                start: cursor,
                end: m.span.start,
            });
        }
        cursor = cursor.max(m.span.end);
    }
    if now - cursor >= MIN_GAP_SECONDS {
        t.gaps.push(Span {
            start: cursor,
            end: now,
        });
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    const SRC: &str = "## Cold Open :music[hype-drop]{#m1}\n\
[ON SCREEN: black]\n\
one two three four five six seven eight nine ten :sfx[boom]{#s1}\n\
:transition[whip-pan]{#t1}\n\
## Chapter :music[stop]{#m2}\n\
eleven twelve thirteen fourteen fifteen :music[grip]{#m3} sixteen seventeen eighteen nineteen twenty\n";

    #[test]
    fn times_lines_and_sections() {
        let t = timeline(&parse(SRC));
        // 2s on-screen + 10 words (4s) + 10 words (4s).
        assert_eq!(t.total, 10.0);
        assert_eq!(t.sections.len(), 2);
        assert_eq!(
            t.sections[0].span,
            Span {
                start: 0.0,
                end: 6.0
            }
        );
        assert_eq!(
            t.sections[1].span,
            Span {
                start: 6.0,
                end: 10.0
            }
        );
        assert_eq!(
            t.blocks[2],
            Span {
                start: 2.0,
                end: 6.0
            }
        );
    }

    #[test]
    fn music_runs_until_the_next_cue_or_stop() {
        let t = timeline(&parse(SRC));
        assert_eq!(t.music.len(), 2, "stop does not make a span");
        assert_eq!(t.music[0].track(), Some("hype-drop"));
        assert_eq!(
            t.music[0].span,
            Span {
                start: 0.0,
                end: 6.0
            }
        );
        // Starts after five of ten words on a 4s line.
        assert_eq!(
            t.music[1].span,
            Span {
                start: 8.0,
                end: 10.0
            }
        );
        assert_eq!(
            t.gaps,
            vec![Span {
                start: 6.0,
                end: 8.0
            }]
        );
        assert_eq!(t.music_over(t.sections[1].span), 2.0);
    }

    #[test]
    fn markers_land_on_their_word() {
        let t = timeline(&parse(SRC));
        assert_eq!(t.stingers[0].at, 6.0, "sfx after the last word");
        assert_eq!(t.transitions[0].at, 6.0);
        assert_eq!(t.section_of(5).unwrap().title, "Chapter");
    }

    #[test]
    fn no_music_is_one_gap() {
        let t = timeline(&parse("## A\nhello there friend\n"));
        assert!(t.music.is_empty());
        assert_eq!(t.gaps.len(), 1);
    }
}
