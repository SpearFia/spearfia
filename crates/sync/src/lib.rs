//! `spearfia-sync`: Two-way editor sync: compiles timeline plans, keeps the cue ID map, and three-way merges editor changes against the last-synced snapshot.
//!
//! So far: [`compile`], the first one-way push (ADR 0005). Links and script order become a
//! `timeline-plan`: voiceover links set the timing, picture links fill the same stretch.
//! Pull and merge are not built yet.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spearfia_core::links::{self, Link, LinkKind, Links};
use spearfia_core::project::{MediaRole, Timebase};
use spearfia_script::{Block, Directive, Script, Segment, format::line_words};
use std::collections::BTreeMap;

/// The crate name, used in logs and analyzer/version stamps.
pub const NAME: &str = env!("CARGO_PKG_NAME");

pub const SCHEMA_URL: &str = "https://spearfia.dev/schemas/timeline-plan/v1.json";

/// A `timeline-plan` document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelinePlan {
    #[serde(rename = "$schema")]
    pub schema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_snapshot: Option<String>,
    pub timebase: Timebase,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Clip,
    Transition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub cue_id: String,
    pub kind: Kind,
    pub track: String,
    pub start_frame: u64,
    pub duration_frames: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<Intent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub path: String,
    pub in_frame: u64,
}

/// The slot the script asked for, set when the placed clip is shorter than it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intent {
    pub start_frame: u64,
    pub duration_frames: u64,
}

/// Something compile skipped or shortened. Nothing is guessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "issue", rename_all = "kebab-case")]
pub enum Issue {
    /// The link's words are no longer in the script.
    Lost { link: String },
    /// The link's file is not in `media` (not imported, or unreadable).
    Missing { link: String },
    /// `in` / `out` is not `HH:MM:SS`, `MM:SS` or seconds.
    BadPoint { link: String },
    /// A voiceover file with no known length can't set timing.
    Untimed { link: String },
    /// Nothing left to place: `in` at or past the end, or the next clip on its track starts
    /// at the same frame.
    Empty { link: String },
    /// A picture link before the first voiceover link.
    Unplaced { link: String },
    /// The media ran out `frames` before the end of its slot.
    Short { link: String, frames: u64 },
    /// A transition cue with no ID, an unknown label, or no picture clip after it.
    Unmapped {
        cue: Option<String>,
        label: Option<String>,
    },
}

impl Issue {
    /// One line for the creator.
    pub fn describe(&self) -> String {
        match self {
            Self::Lost { link } => format!("{link}: its words are no longer in the script"),
            Self::Missing { link } => {
                format!("{link}: the file is missing or FilmCraft can't read it")
            }
            Self::BadPoint { link } => format!("{link}: in/out is not HH:MM:SS or seconds"),
            Self::Untimed { link } => {
                format!("{link}: voiceover with no length (not an audio file?)")
            }
            Self::Empty { link } => format!("{link}: nothing left to place after in/out"),
            Self::Unplaced { link } => format!("{link}: picture before the first voiceover"),
            Self::Short { link, frames } => {
                format!("{link}: media ends {frames} frames before its slot")
            }
            Self::Unmapped { cue, label } => format!(
                "transition {}: \"{}\" has no FilmCraft match, no ID, or no picture clip after it",
                cue.as_deref().unwrap_or("(no ID)"),
                label.as_deref().unwrap_or("")
            ),
        }
    }
}

/// Compile `links` over `script` into a plan. `media` maps each link's media path to its
/// length in frames of `timebase`, or `None` for a still (any length). The plan has no
/// `plan_id`; the caller names it.
pub fn compile(
    script: &Script,
    links: &Links,
    media: &BTreeMap<String, Option<u64>>,
    timebase: Timebase,
) -> (TimelinePlan, Vec<Issue>) {
    let words = Words::new(script);
    let mut issues = Vec::new();

    let mut vo = Vec::new();
    let mut pictures = Vec::new();
    for l in &links.links {
        let id = || l.id.clone();
        let Some(a) = links::locate(&l.quote, l.line, &words.lines) else {
            issues.push(Issue::Lost { link: id() });
            continue;
        };
        let (Some(inp), Some(out)) = (point(&l.in_point, timebase), point(&l.out_point, timebase))
        else {
            issues.push(Issue::BadPoint { link: id() });
            continue;
        };
        let Some(&length) = media.get(&l.media.path) else {
            issues.push(Issue::Missing { link: id() });
            continue;
        };
        let at = Anchored {
            key: (a.line, a.first_word),
            link: l,
            inp: inp.unwrap_or(0),
            out,
            length,
        };
        // Music and SFX are placed like pictures, on their own audio tracks.
        match (l.kind, l.role) {
            (LinkKind::Audio, MediaRole::Music | MediaRole::Sfx) | (LinkKind::Picture, _) => {
                pictures.push(at)
            }
            (LinkKind::Audio, _) => vo.push(at),
        }
    }
    let order =
        |a: &Anchored, b: &Anchored| a.key.cmp(&b.key).then(id_num(a.link).cmp(&id_num(b.link)));
    vo.sort_by(order);
    pictures.sort_by(order);

    // Voiceover back to back on A1; each owns the words up to the next one.
    let mut items = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut now = 0;
    for v in &vo {
        let Some(len) = v.length else {
            issues.push(Issue::Untimed {
                link: v.link.id.clone(),
            });
            continue;
        };
        let len = v.available(len);
        if len == 0 {
            issues.push(Issue::Empty {
                link: v.link.id.clone(),
            });
            continue;
        }
        items.push(clip(v, "A1", now, len, None));
        spans.push(Span {
            key: v.key,
            pos: words.pos(v.key),
            start: now,
            len,
            words: 0,
        });
        now += len;
    }
    let ends: Vec<u64> = spans
        .iter()
        .skip(1)
        .map(|s| s.pos)
        .chain([words.total])
        .collect();
    for (s, end) in spans.iter_mut().zip(ends) {
        s.words = end.saturating_sub(s.pos);
    }

    // Pictures start where their words are spoken and run to the next picture on their
    // track or the end of their voiceover span, whichever comes first. Music and SFX run to
    // the next one on their track or the end of all voiceover (`now`).
    let mut slots = Vec::new();
    for p in &pictures {
        let audio = matches!(p.link.role, MediaRole::Music | MediaRole::Sfx);
        match time_at(&spans, &words, p.key) {
            Some((t, s)) => slots.push((
                track_for(p.link.role),
                t,
                if audio { now } else { s.start + s.len },
                p,
            )),
            None => issues.push(Issue::Unplaced {
                link: p.link.id.clone(),
            }),
        }
    }
    slots.sort_by_key(|&(track, t, _, p)| (track, t, p.key));
    let mut placed = Vec::new();
    for (i, &(track, start, span_end, p)) in slots.iter().enumerate() {
        let next = slots
            .get(i + 1)
            .filter(|n| n.0 == track)
            .map_or(u64::MAX, |n| n.1);
        let slot = span_end.min(next).saturating_sub(start);
        let dur = p.length.map_or(slot, |len| p.available(len).min(slot));
        if dur == 0 {
            issues.push(Issue::Empty {
                link: p.link.id.clone(),
            });
            continue;
        }
        // A sound effect is meant to be shorter than the time until the next one.
        let intent = (dur < slot && p.link.role != MediaRole::Sfx).then(|| {
            issues.push(Issue::Short {
                link: p.link.id.clone(),
                frames: slot - dur,
            });
            Intent {
                start_frame: start,
                duration_frames: slot,
            }
        });
        placed.push(clip(p, track, start, dur, intent));
    }
    placed.sort_by(|a, b| (a.start_frame, &a.track).cmp(&(b.start_frame, &b.track)));

    // Transitions sit on the in edge of the first picture clip at or after their cue.
    let mut transitions = Vec::new();
    for (key, cue) in words.transitions(script) {
        let unmapped = || Issue::Unmapped {
            cue: cue.id.clone(),
            label: cue.label.clone(),
        };
        let effect = match cue
            .label
            .as_deref()
            .map(|l| l.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("cut" | "hard cut" | "smash cut") => continue,
            Some("dissolve" | "cross dissolve") => "cross_dissolve",
            Some("dip to black") => "dip_to_black",
            Some("dip to white") => "dip_to_white",
            _ => {
                issues.push(unmapped());
                continue;
            }
        };
        let to = time_at(&spans, &words, key)
            .and_then(|(t, _)| placed.iter().find(|c| c.start_frame >= t));
        let (Some(id), Some(to)) = (&cue.id, to) else {
            issues.push(unmapped());
            continue;
        };
        let frames = cue
            .attr("duration")
            .and_then(|s| s.parse::<f64>().ok())
            .map_or(0, |s| seconds_to_frames(s, timebase));
        transitions.push(Item {
            cue_id: id.clone(),
            kind: Kind::Transition,
            track: to.track.clone(),
            start_frame: to.start_frame,
            duration_frames: frames,
            source: None,
            params: Some(json!({"effect": effect, "attach": to.cue_id, "edge": "in"})),
            intent: None,
        });
    }

    items.extend(placed);
    items.extend(transitions);
    let plan = TimelinePlan {
        schema: SCHEMA_URL.into(),
        plan_id: None,
        base_snapshot: None,
        timebase,
        items,
    };
    (plan, issues)
}

/// A link found in the script, with its source range in frames.
struct Anchored<'a> {
    key: (usize, usize),
    link: &'a Link,
    inp: u64,
    out: Option<u64>,
    length: Option<u64>,
}

impl Anchored<'_> {
    /// Frames between `in` and `out` (or the end) of media `len` frames long.
    fn available(&self, len: u64) -> u64 {
        self.out
            .map_or(len, |o| o.min(len))
            .saturating_sub(self.inp)
    }
}

/// A voiceover clip's stretch: from its anchor to the next one, `words` spoken words long.
struct Span {
    key: (usize, usize),
    pos: u64,
    start: u64,
    len: u64,
    words: u64,
}

/// The script's lines as [`links::locate`] sees them, with spoken-word counts for timing.
struct Words<'a> {
    lines: Vec<(usize, Vec<&'a str>)>,
    /// Per line: block index, spoken words before it, and whether it is spoken.
    meta: Vec<(usize, u64, bool)>,
    total: u64,
}

impl<'a> Words<'a> {
    fn new(script: &'a Script) -> Self {
        let (mut lines, mut meta, mut total) = (Vec::new(), Vec::new(), 0);
        for (b, block) in script.blocks.iter().enumerate() {
            if let Block::Line {
                kind,
                segments,
                line,
            } = block
            {
                let words = line_words(segments);
                meta.push((b, total, kind.is_spoken()));
                if kind.is_spoken() {
                    total += words.len() as u64;
                }
                lines.push((*line, words));
            }
        }
        Self { lines, meta, total }
    }

    /// Spoken words before `(line index, word)`. Unspoken lines (on-screen directions) take
    /// no time, so their words sit where the next spoken word starts.
    fn pos(&self, (line, word): (usize, usize)) -> u64 {
        self.meta
            .get(line)
            .map_or(self.total, |&(_, before, spoken)| {
                before + if spoken { word as u64 } else { 0 }
            })
    }

    /// Every `:transition` cue with the position it cuts at: inline, after the words before
    /// it; on a heading or its own line, at the start of the next line.
    fn transitions(&self, script: &'a Script) -> Vec<((usize, usize), &'a Directive)> {
        let is_transition = |c: &Directive| c.name == "transition";
        let mut out = Vec::new();
        for (b, block) in script.blocks.iter().enumerate() {
            let next_line = self
                .meta
                .iter()
                .position(|m| m.0 > b)
                .unwrap_or(self.lines.len());
            match block {
                Block::Heading { cues, .. } => out.extend(
                    cues.iter()
                        .filter(|c| is_transition(c))
                        .map(|c| ((next_line, 0), c)),
                ),
                Block::Divider { cue, .. } if is_transition(cue) => out.push(((next_line, 0), cue)),
                Block::Divider { .. } => {}
                Block::Line { segments, .. } => {
                    let this_line = next_line.saturating_sub(1);
                    for (k, s) in segments.iter().enumerate() {
                        if let Segment::Cue(c) = s
                            && is_transition(c)
                        {
                            out.push(((this_line, line_words(&segments[..k]).len()), c));
                        }
                    }
                }
            }
        }
        out
    }
}

/// Sequence frame for a script position: its voiceover span's start, plus the span's length
/// in proportion to the spoken words before the position.
// shortcut: linear word timing inside a span; use transcript word times once analysis has them.
fn time_at<'s>(spans: &'s [Span], words: &Words, key: (usize, usize)) -> Option<(u64, &'s Span)> {
    let s = spans.iter().rev().find(|s| s.key <= key)?;
    let into = words.pos(key).saturating_sub(s.pos).min(s.words);
    let offset = (s.len * into).checked_div(s.words).unwrap_or(0);
    Some((s.start + offset, s))
}

fn track_for(role: MediaRole) -> &'static str {
    match role {
        MediaRole::ARoll | MediaRole::Screen => "V1",
        MediaRole::Graphics => "V3",
        MediaRole::BRoll | MediaRole::Voiceover | MediaRole::Other => "V2",
        MediaRole::Music => "A2",
        MediaRole::Sfx => "A3",
    }
}

fn clip(a: &Anchored, track: &str, start: u64, len: u64, intent: Option<Intent>) -> Item {
    Item {
        cue_id: a.link.id.clone(),
        kind: Kind::Clip,
        track: track.into(),
        start_frame: start,
        duration_frames: len,
        source: Some(Source {
            path: a.link.media.path.clone(),
            in_frame: a.inp,
        }),
        params: None,
        intent,
    }
}

fn id_num(l: &Link) -> u64 {
    l.id.strip_prefix('k')
        .and_then(|n| n.parse().ok())
        .unwrap_or(u64::MAX)
}

/// No point → `Some(None)`; `HH:MM:SS`, `MM:SS` or seconds → `Some(Some(frames))`;
/// anything else → `None`.
fn point(p: &Option<String>, tb: Timebase) -> Option<Option<u64>> {
    let Some(p) = p else { return Some(None) };
    let (last, whole) = p
        .split(':')
        .collect::<Vec<_>>()
        .split_last()
        .map(|(l, w)| (*l, w.to_vec()))?;
    if whole.len() > 2 || !last.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return None;
    }
    let mut secs = 0.0;
    for w in &whole {
        if w.is_empty() || !w.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        secs = secs * 60.0 + w.parse::<f64>().ok()?;
    }
    let s: f64 = last.parse().ok()?;
    let total = if whole.is_empty() { s } else { secs * 60.0 + s };
    Some(Some(seconds_to_frames(total, tb)))
}

fn seconds_to_frames(s: f64, tb: Timebase) -> u64 {
    (s * f64::from(tb.num) / f64::from(tb.den)).round().max(0.0) as u64
}

#[cfg(test)]
mod tests;
