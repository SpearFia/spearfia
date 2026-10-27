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

pub mod edit;

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

/// Words heard in each voiceover file (`media/vo-words.json`): media path → `(word, start
/// seconds)` in file time. Empty means linear timing inside each voiceover span.
pub type Heard = BTreeMap<String, Vec<(String, f64)>>;

/// Compile `links` over `script` into a plan. `media` maps each link's media path to its
/// length in frames of `timebase`, or `None` for a still (any length). The plan has no
/// `plan_id`; the caller names it.
pub fn compile(
    script: &Script,
    links: &Links,
    media: &BTreeMap<String, Option<u64>>,
    timebase: Timebase,
) -> (TimelinePlan, Vec<Issue>) {
    compile_heard(script, links, media, timebase, &Heard::new())
}

/// [`compile`] with heard words: pictures, transitions and cues land on the frame their word
/// is spoken instead of a linear estimate.
pub fn compile_heard(
    script: &Script,
    links: &Links,
    media: &BTreeMap<String, Option<u64>>,
    timebase: Timebase,
    heard: &Heard,
) -> (TimelinePlan, Vec<Issue>) {
    let words = Words::new(script);
    let mut issues = Vec::new();
    let (vo, pictures) = anchor_links(links, media, timebase, &words, &mut issues);
    let (mut items, spans, now) = voice(&vo, &words, heard, timebase, &mut issues);

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
    for (i, &(track, at, span_end, p)) in slots.iter().enumerate() {
        // Links on the same words share their stretch in link order instead of covering
        // each other.
        let same = |j: &usize| slots[*j].0 == track && slots[*j].1 == at;
        let first = (0..=i).rev().take_while(same).last().unwrap_or(i);
        let last = (i..slots.len()).take_while(same).last().unwrap_or(i);
        let next = slots
            .get(last + 1)
            .filter(|n| n.0 == track)
            .map_or(u64::MAX, |n| n.1);
        let (k, n) = ((i - first) as u64, (last - first + 1) as u64);
        let stretch = span_end.min(next).saturating_sub(at);
        let start = at + stretch * k / n;
        let slot = at + stretch * (k + 1) / n - start;
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

/// Links found in the script, split into voiceover and everything placed against it, in
/// script order.
fn anchor_links<'l>(
    links: &'l Links,
    media: &BTreeMap<String, Option<u64>>,
    timebase: Timebase,
    words: &Words,
    issues: &mut Vec<Issue>,
) -> (Vec<Anchored<'l>>, Vec<Anchored<'l>>) {
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
    (vo, pictures)
}

/// Voiceover back to back on A1, and the span of script words each one owns.
fn voice(
    vo: &[Anchored],
    words: &Words,
    heard: &Heard,
    timebase: Timebase,
    issues: &mut Vec<Issue>,
) -> (Vec<Item>, Vec<Span>, u64) {
    let mut items = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut now = 0;
    for v in vo {
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
            path: v.link.media.path.clone(),
            inp: v.inp,
            times: Vec::new(),
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

    for s in &mut spans {
        if let Some(h) = heard.get(&s.path) {
            let span = &words.spoken[s.pos as usize..(s.pos + s.words) as usize];
            s.times = align(span, h, s.inp, s.len, timebase);
        }
    }
    (items, spans, now)
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
    /// The voiceover file and the frame it starts playing from.
    path: String,
    inp: u64,
    /// Frames into the span where each of its words starts, from heard words; empty: linear.
    times: Vec<u64>,
}

/// The script's lines as [`links::locate`] sees them, with spoken-word counts for timing.
struct Words<'a> {
    lines: Vec<(usize, Vec<&'a str>)>,
    /// Every spoken word in order; [`Words::pos`] indexes it.
    spoken: Vec<&'a str>,
    /// Per line: block index, spoken words before it, and whether it is spoken.
    meta: Vec<(usize, u64, bool)>,
    total: u64,
}

impl<'a> Words<'a> {
    fn new(script: &'a Script) -> Self {
        let (mut lines, mut meta, mut total, mut spoken) = (Vec::new(), Vec::new(), 0, Vec::new());
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
                    spoken.extend(words.iter().copied());
                }
                lines.push((*line, words));
            }
        }
        Self {
            lines,
            spoken,
            meta,
            total,
        }
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

/// Sequence frame for a script position: its voiceover span's start, plus when its word is
/// heard, or without heard words the span's length in proportion to the words before it.
fn time_at<'s>(spans: &'s [Span], words: &Words, key: (usize, usize)) -> Option<(u64, &'s Span)> {
    let s = spans.iter().rev().find(|s| s.key <= key)?;
    let into = words.pos(key).saturating_sub(s.pos).min(s.words);
    Some((s.start + s.at(into), s))
}

impl Span {
    /// Frames into the span where its word `k` starts (`k == words`: the span's end).
    fn at(&self, k: u64) -> u64 {
        self.times
            .get(k as usize)
            .copied()
            .unwrap_or_else(|| (self.len * k).checked_div(self.words).unwrap_or(0))
    }
}

/// Lowercase letters and digits only, so "Geralt's," and "geralts" match.
fn norm(w: &str) -> String {
    w.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Frames into a span (`len` frames, playing its file from frame `inp`) where each script word
/// starts: the longest common subsequence of script and heard words gives the matched words
/// their heard time; the rest are spread evenly between their matched neighbours.
fn align(script: &[&str], heard: &[(String, f64)], inp: u64, len: u64, tb: Timebase) -> Vec<u64> {
    let a: Vec<String> = script.iter().map(|w| norm(w)).collect();
    let b: Vec<String> = heard.iter().map(|(w, _)| norm(w)).collect();
    let (n, m) = (a.len(), b.len());
    let at = |i: usize, j: usize| i * (m + 1) + j;
    let mut lcs = vec![0u32; (n + 1) * (m + 1)];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[at(i, j)] = if !a[i].is_empty() && a[i] == b[j] {
                lcs[at(i + 1, j + 1)] + 1
            } else {
                lcs[at(i + 1, j)].max(lcs[at(i, j + 1)])
            };
        }
    }
    let mut known: Vec<Option<u64>> = vec![None; n];
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if !a[i].is_empty() && a[i] == b[j] {
            let f = seconds_to_frames(heard[j].1.max(0.0), tb)
                .saturating_sub(inp)
                .min(len);
            known[i] = Some(f);
            (i, j) = (i + 1, j + 1);
        } else if lcs[at(i + 1, j)] >= lcs[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    // Unmatched words share the time between the matched words around them.
    let mut out = vec![0; n];
    let mut prev = (0usize, 0u64); // (index + 1, frame) of the last placed known word; (0, 0): span start
    for i in 0..n {
        if let Some(f) = known[i] {
            out[i] = f.max(prev.1);
            prev = (i + 1, out[i]);
            continue;
        }
        let (q, fq) = (i + 1..n)
            .find_map(|q| known[q].map(|f| (q + 1, f)))
            .unwrap_or((n + 1, len));
        let fq = fq.max(prev.1);
        out[i] = prev.1 + (fq - prev.1) * (i + 1 - prev.0) as u64 / (q - prev.0) as u64;
    }
    out
}

/// When each part of the script is spoken, for placing edits and captions after a push.
pub struct Timing<'a> {
    words: Words<'a>,
    spans: Vec<Span>,
}

/// A spoken script word with its sequence frames.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedWord {
    pub text: String,
    pub start: u64,
    pub end: u64,
    /// The voiceover file it is heard in, and where that file's frame 0 sits in the sequence
    /// (`start - origin` is file time).
    pub path: String,
    pub origin: i64,
}

impl<'a> Timing<'a> {
    pub fn new(
        script: &'a Script,
        links: &Links,
        media: &BTreeMap<String, Option<u64>>,
        timebase: Timebase,
        heard: &Heard,
    ) -> Self {
        let words = Words::new(script);
        let mut issues = Vec::new();
        let (vo, _) = anchor_links(links, media, timebase, &words, &mut issues);
        let (_, spans, _) = voice(&vo, &words, heard, timebase, &mut issues);
        Self { words, spans }
    }

    /// The frame `quote` (near script line `line`) starts being spoken.
    /// Punctuation and case don't matter here, so an edit script can quote words loosely.
    pub fn frame_of(&self, quote: &str, line: usize) -> Option<u64> {
        let key = links::locate(quote, line, &self.words.lines)
            .map(|a| (a.line, a.first_word))
            .or_else(|| {
                let needle: Vec<String> = quote.split_whitespace().map(norm).collect();
                let mut best: Option<(usize, (usize, usize))> = None;
                for (i, (no, words)) in self.words.lines.iter().enumerate() {
                    let w: Vec<String> = words.iter().map(|w| norm(w)).collect();
                    if let Some(at) = w.windows(needle.len().max(1)).position(|x| x == needle.as_slice()) {
                        let d = no.abs_diff(line);
                        if best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, (i, at)));
                        }
                    }
                }
                best.map(|b| b.1)
            })?;
        time_at(&self.spans, &self.words, key).map(|(t, _)| t)
    }

    /// Each `##` heading's text and the frame its first spoken word starts.
    pub fn sections(&self, script: &Script) -> Vec<(String, u64)> {
        let mut out = Vec::new();
        for (b, block) in script.blocks.iter().enumerate() {
            if let Block::Heading { level: 2, text, .. } = block {
                let next = self
                    .words
                    .meta
                    .iter()
                    .position(|m| m.0 > b)
                    .unwrap_or(self.words.lines.len());
                // A heading before the first voiceover starts with it.
                let t = time_at(&self.spans, &self.words, (next, 0))
                    .map(|(t, _)| t)
                    .or(self.spans.first().map(|s| s.start));
                if let Some(t) = t {
                    out.push((text.clone(), t));
                }
            }
        }
        out
    }

    /// Every spoken word inside a voiceover span, in order.
    pub fn words(&self) -> Vec<TimedWord> {
        let mut out = Vec::new();
        for s in &self.spans {
            for k in 0..s.words {
                out.push(TimedWord {
                    text: self.words.spoken[(s.pos + k) as usize].to_string(),
                    start: s.start + s.at(k),
                    end: s.start + s.at(k + 1).max(s.at(k)),
                    path: s.path.clone(),
                    origin: s.start as i64 - s.inp as i64,
                });
            }
        }
        out
    }

    /// The end of the last voiceover clip.
    pub fn end(&self) -> u64 {
        self.spans.last().map_or(0, |s| s.start + s.len)
    }
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
