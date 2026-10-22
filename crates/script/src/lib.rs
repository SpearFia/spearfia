//! `spearfia-script`: reads `script.md` (Markdown + directives, see
//! `spearfia-spec/docs/script-directives.md`) into blocks the UI and sync can use.
//!
//! Each non-empty line is one block:
//!
//! - `## Heading` starts a section; directives on the heading line apply to the section.
//! - A line that is only a `:transition[...]` directive is a divider between lines.
//! - A line starting with `[` (e.g. `[ON SCREEN: ...]`) is an on-screen direction.
//! - A line starting with `LIVE:` is spoken to camera; `VO:` (optional) marks voiceover.
//! - Notes are never spoken: `> quotes`, `- bullets`, `1. numbered`, `- [ ] checklists`
//!   and `| table |` rows.
//! - Any other line is spoken (VO).
//!
//! The round-trip-safe writer comes later; for now edits rewrite the whole file.

pub mod attrs;
pub mod cuts;
pub mod edit;
pub mod format;
pub mod looks;
pub mod sfx;
pub mod timing;

use std::ops::Range;

/// The crate name, used in logs.
pub const NAME: &str = env!("CARGO_PKG_NAME");

/// Speaking rate used for runtime estimates.
pub const WORDS_PER_MINUTE: f32 = 150.0;

/// `:name[label]{key=value #id}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub name: String,
    pub label: Option<String>,
    pub attrs: Vec<(String, String)>,
    pub id: Option<String>,
    /// Byte range of the whole directive in the source.
    pub span: Range<usize>,
}

impl Directive {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Short text for a chip: the label, then any attributes (`whip-pan 0.4s`,
    /// `mood=tense`).
    pub fn display(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(l) = &self.label {
            parts.push(l.clone());
        }
        for (k, v) in &self.attrs {
            parts.push(match k.as_str() {
                "duration" => format!("{v}s"),
                "in" | "out" => format!("{k} {v}"),
                _ if self.label.is_some() => v.clone(),
                _ => format!("{k}={v}"),
            });
        }
        if parts.is_empty() {
            self.name.clone()
        } else {
            parts.join(" ")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Cue(Directive),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Voiceover, read aloud.
    Spoken,
    /// `LIVE:` spoken to camera.
    Live,
    /// `[ON SCREEN: ...]` direction.
    OnScreen,
    /// Writer's notes: quotes, lists, checklists, tables. Not spoken or timed.
    Note,
}

impl LineKind {
    /// Read aloud (VO or live).
    pub fn is_spoken(self) -> bool {
        matches!(self, Self::Spoken | Self::Live)
    }
}

/// Bytes at the start of a line that mark its kind (`LIVE: `, `VO: `), if any.
pub fn kind_prefix(line: &str) -> Option<(LineKind, usize)> {
    let lead = line.len() - line.trim_start().len();
    let t = &line[lead..];
    for (p, k) in [("LIVE:", LineKind::Live), ("VO:", LineKind::Spoken)] {
        // Compare bytes: slicing `t` at `p.len()` could split a multi-byte character.
        if t.as_bytes()
            .get(..p.len())
            .is_some_and(|b| b.eq_ignore_ascii_case(p.as_bytes()))
        {
            let rest = &t[p.len()..];
            let pad = rest.len() - rest.trim_start().len();
            return Some((k, lead + p.len() + pad));
        }
    }
    None
}

/// Whether a (trimmed) line is a note: quote, list item, checklist or table row.
pub fn is_note(trimmed: &str) -> bool {
    let numbered = {
        let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
        digits > 0 && trimmed[digits..].starts_with(". ")
    };
    trimmed.starts_with("> ")
        || trimmed == ">"
        || trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with('|')
        || numbered
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        text: String,
        cues: Vec<Directive>,
        line: usize,
    },
    Line {
        kind: LineKind,
        segments: Vec<Segment>,
        line: usize,
    },
    Divider {
        cue: Directive,
        line: usize,
    },
}

/// The YAML front matter, kept raw, with the keys the UI shows pulled out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub raw: String,
    pub title: Option<String>,
    pub format: Option<String>,
    pub vision: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Script {
    pub front_matter: Option<FrontMatter>,
    pub blocks: Vec<Block>,
}

/// One `##` section, for the outline.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionSummary {
    pub title: String,
    pub block_index: usize,
    pub spoken_words: usize,
    pub est_seconds: f32,
    pub cues: usize,
}

impl Script {
    /// Sections (level-2 headings) with word counts and runtime estimates.
    pub fn outline(&self) -> Vec<SectionSummary> {
        let mut out: Vec<SectionSummary> = Vec::new();
        for (i, b) in self.blocks.iter().enumerate() {
            match b {
                Block::Heading {
                    level, text, cues, ..
                } if *level >= 2 => out.push(SectionSummary {
                    title: text.clone(),
                    block_index: i,
                    spoken_words: 0,
                    est_seconds: 0.0,
                    cues: cues.len(),
                }),
                Block::Line { kind, segments, .. } => {
                    if let Some(s) = out.last_mut() {
                        if kind.is_spoken() {
                            s.spoken_words += word_count(segments);
                        }
                        s.cues += segments
                            .iter()
                            .filter(|g| matches!(g, Segment::Cue(_)))
                            .count();
                    }
                }
                Block::Divider { .. } => {
                    if let Some(s) = out.last_mut() {
                        s.cues += 1;
                    }
                }
                Block::Heading { .. } => {}
            }
        }
        for s in &mut out {
            s.est_seconds = estimate_seconds(s.spoken_words);
        }
        out
    }

    /// Estimated runtime of all spoken lines, in seconds.
    pub fn est_seconds(&self) -> f32 {
        let words: usize = self
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Line { kind, segments, .. } if kind.is_spoken() => {
                    Some(word_count(segments))
                }
                _ => None,
            })
            .sum();
        estimate_seconds(words)
    }

    /// The level-1 heading, or the front-matter title.
    pub fn title(&self) -> Option<&str> {
        self.blocks
            .iter()
            .find_map(|b| match b {
                Block::Heading { level: 1, text, .. } => Some(text.as_str()),
                _ => None,
            })
            .or_else(|| self.front_matter.as_ref()?.title.as_deref())
    }
}

/// Inline marks that are formatting, not words (`**bold**`, `++underline++`, ...).
pub const MARKS: [char; 4] = ['*', '+', '~', '`'];

/// Whether a whitespace-separated token is a word, not formatting marks alone (`**`).
pub fn is_word(token: &str) -> bool {
    token.chars().any(|c| !MARKS.contains(&c))
}

/// The words of a text segment: whitespace-separated, skipping formatting-only tokens.
pub fn words_in(text: &str) -> impl Iterator<Item = &str> {
    text.split_whitespace().filter(|w| is_word(w))
}

pub fn word_count(segments: &[Segment]) -> usize {
    segments
        .iter()
        .map(|s| match s {
            Segment::Text(t) => words_in(t).count(),
            Segment::Cue(_) => 0,
        })
        .sum()
}

pub fn estimate_seconds(words: usize) -> f32 {
    words as f32 / WORDS_PER_MINUTE * 60.0
}

/// Parse a whole `script.md`.
pub fn parse(src: &str) -> Script {
    let mut script = Script::default();
    let mut offset = 0;
    let mut lines = src.split_inclusive('\n').enumerate().peekable();

    // Front matter: `---` on the first line, closed by another `---` line.
    if src.starts_with("---\n") || src.starts_with("---\r\n") {
        let mut raw = String::new();
        let mut consumed = 0;
        let mut closed = false;
        for (i, l) in src.split_inclusive('\n').enumerate() {
            consumed += l.len();
            if i == 0 {
                continue;
            }
            if l.trim_end() == "---" {
                closed = true;
                break;
            }
            raw.push_str(l);
        }
        if closed {
            let fm = front_matter(raw);
            script.front_matter = Some(fm);
            offset = consumed;
            let skip = src[..consumed].matches('\n').count();
            for _ in 0..skip {
                lines.next();
            }
        }
    }

    for (n, raw_line) in lines {
        let start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches(['\n', '\r']);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lead = line.len() - line.trim_start().len();

        // A `{align=center}` paragraph override is formatting, not text.
        let trimmed = match attrs::para_attrs(trimmed) {
            Some((r, _)) => &trimmed[..r.start],
            None => trimmed,
        };
        if let Some((level, rest_at)) = heading(trimmed) {
            let segs = segments(&trimmed[rest_at..], start + lead + rest_at);
            let text = segs
                .iter()
                .filter_map(|s| match s {
                    Segment::Text(t) => Some(t.as_str()),
                    Segment::Cue(_) => None,
                })
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let cues = segs
                .into_iter()
                .filter_map(|s| match s {
                    Segment::Cue(d) => Some(d),
                    Segment::Text(_) => None,
                })
                .collect();
            script.blocks.push(Block::Heading {
                level,
                text,
                cues,
                line: n,
            });
            continue;
        }

        // `LIVE:` / `VO:` prefixes are not words.
        let (prefixed, skip) = match kind_prefix(trimmed) {
            Some((k, n)) => (Some(k), n),
            None => (None, 0),
        };
        let segs = segments(&trimmed[skip..], start + lead + skip);
        let only_transition = {
            let mut cues = segs.iter().filter_map(|s| match s {
                Segment::Cue(d) => Some(d),
                Segment::Text(_) => None,
            });
            let first = cues.next();
            let texts_blank = segs.iter().all(|s| match s {
                Segment::Text(t) => t.trim().is_empty(),
                Segment::Cue(_) => true,
            });
            match (first, cues.next()) {
                (Some(d), None) if texts_blank && d.name == "transition" => Some(d.clone()),
                _ => None,
            }
        };
        if let Some(cue) = only_transition {
            script.blocks.push(Block::Divider { cue, line: n });
            continue;
        }
        let kind = if let Some(k) = prefixed {
            k
        } else if trimmed.starts_with('[')
            && !attrs::spans(trimmed, 0)
                .first()
                .is_some_and(|s| s.open.start == 0)
        {
            LineKind::OnScreen
        } else if is_note(trimmed) {
            LineKind::Note
        } else {
            LineKind::Spoken
        };
        script.blocks.push(Block::Line {
            kind,
            segments: segs,
            line: n,
        });
    }
    script
}

fn front_matter(raw: String) -> FrontMatter {
    let get = |key: &str| {
        raw.lines().find_map(|l| {
            let v = l.strip_prefix(key)?.strip_prefix(':')?.trim();
            (!v.is_empty()).then(|| v.trim_matches('"').to_owned())
        })
    };
    FrontMatter {
        title: get("title"),
        format: get("format"),
        vision: get("vision"),
        raw,
    }
}

/// `## Title` → (2, index of the title text).
fn heading(line: &str) -> Option<(u8, usize)> {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &line[hashes..];
    if !rest.starts_with(' ') {
        return None;
    }
    let pad = rest.len() - rest.trim_start().len();
    Some((hashes as u8, hashes + pad))
}

/// Split a line into text and directives. `base` is the line's byte offset in the file.
pub fn segments(line: &str, base: usize) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut text_start = 0;
    let mut i = 0;
    let bytes = line.as_bytes();
    // Inline formatting spans: `[` and `]{...}` are markup, not text. Text on both sides
    // of markup stays one segment, so `[word]{color=red},` is still one word.
    let markup: Vec<Range<usize>> = attrs::spans(line, 0)
        .into_iter()
        .flat_map(|s| [s.open, s.close])
        .collect();
    let mut text = String::new();
    while i < bytes.len() {
        if let Some(m) = markup.iter().find(|m| m.start == i) {
            text.push_str(&line[text_start..i]);
            i = m.end;
            text_start = i;
            continue;
        }
        let at_boundary = i == 0 || bytes[i - 1].is_ascii_whitespace() || bytes[i - 1] == b'(';
        if bytes[i] == b':'
            && at_boundary
            && let Some((d, end)) = directive(line, i, base)
        {
            text.push_str(&line[text_start..i]);
            if !text.is_empty() {
                out.push(Segment::Text(std::mem::take(&mut text)));
            }
            out.push(Segment::Cue(d));
            i = end;
            text_start = end;
            continue;
        }
        i += line[i..].chars().next().map_or(1, char::len_utf8);
    }
    text.push_str(&line[text_start..]);
    if !text.is_empty() {
        out.push(Segment::Text(text));
    }
    // Drop whitespace-only text between cues, but keep spacing inside sentences.
    out.retain(|s| !matches!(s, Segment::Text(t) if t.trim().is_empty()));
    for s in &mut out {
        if let Segment::Text(t) = s {
            *t = t.trim().to_owned();
        }
    }
    out
}

/// Parse a directive starting at the `:` at `start`. Returns it and the end index.
fn directive(line: &str, start: usize, base: usize) -> Option<(Directive, usize)> {
    let b = line.as_bytes();
    let mut i = start + 1;
    if !b.get(i)?.is_ascii_lowercase() {
        return None;
    }
    while i < b.len() && (b[i].is_ascii_lowercase() || b[i].is_ascii_digit() || b[i] == b'-') {
        i += 1;
    }
    let name = line[start + 1..i].to_owned();
    let mut label = None;
    let mut bracketed = false;
    if b.get(i) == Some(&b'[') {
        bracketed = true;
        let close = line[i + 1..].find(']')? + i + 1;
        label = Some(line[i + 1..close].to_owned());
        i = close + 1;
    }
    let mut attrs = Vec::new();
    let mut id = None;
    if b.get(i) == Some(&b'{') {
        bracketed = true;
        let close = line[i + 1..].find('}')? + i + 1;
        (attrs, id) = attributes(&line[i + 1..close]);
        i = close + 1;
    }
    if !bracketed {
        // `:word` with neither [label] nor {attrs} is just text.
        return None;
    }
    Some((
        Directive {
            name,
            label,
            attrs,
            id,
            span: base + start..base + i,
        },
        i,
    ))
}

type Attrs = (Vec<(String, String)>, Option<String>);

/// `duration=0.4 mood="slow build" #t1` → attributes and id.
fn attributes(s: &str) -> Attrs {
    let mut attrs = Vec::new();
    let mut id = None;
    let mut chars = s.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        // Read a token, honoring quotes after `=`.
        let mut end = s.len();
        let mut in_quote = false;
        for (j, ch) in chars.by_ref() {
            if ch == '"' {
                in_quote = !in_quote;
            } else if ch.is_whitespace() && !in_quote {
                end = j;
                break;
            }
        }
        let tok = &s[start..end];
        if let Some(rest) = tok.strip_prefix('#') {
            if !rest.is_empty() {
                id = Some(rest.to_owned());
            }
        } else if let Some((k, v)) = tok.split_once('=') {
            attrs.push((k.to_owned(), v.trim_matches('"').to_owned()));
        } else if !tok.starts_with('.') {
            attrs.push((tok.to_owned(), String::new()));
        }
    }
    (attrs, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---\ntitle: Sample Video\nformat: short\nvision: ../vision.yaml\n---\n## Hook\nNever drifted a car in my life. :sfx[record-scratch]{#s1}\n\n:transition[whip-pan]{duration=0.4 #t1}\n\n## The attempt :clip[footage/day1.mp4]{in=00:01:12 out=00:01:20 #c1}\n:music{mood=tense #m1}\n:look[facecam-pip]{position=bottom-left #l1}\n[ON SCREEN: Black. Two seconds of silence.]\n";

    #[test]
    fn parses_front_matter() {
        let s = parse(SAMPLE);
        let fm = s.front_matter.as_ref().unwrap();
        assert_eq!(fm.title.as_deref(), Some("Sample Video"));
        assert_eq!(fm.format.as_deref(), Some("short"));
        assert_eq!(s.title(), Some("Sample Video"));
    }

    #[test]
    fn parses_blocks() {
        let s = parse(SAMPLE);
        let kinds: Vec<&str> = s
            .blocks
            .iter()
            .map(|b| match b {
                Block::Heading { .. } => "h",
                Block::Divider { .. } => "div",
                Block::Line {
                    kind: LineKind::Spoken,
                    ..
                } => "spoken",
                Block::Line {
                    kind: LineKind::OnScreen,
                    ..
                } => "screen",
                Block::Line {
                    kind: LineKind::Live,
                    ..
                } => "live",
                Block::Line {
                    kind: LineKind::Note,
                    ..
                } => "note",
            })
            .collect();
        assert_eq!(
            kinds,
            ["h", "spoken", "div", "h", "spoken", "spoken", "screen"]
        );

        let Block::Heading { text, cues, .. } = &s.blocks[3] else {
            panic!()
        };
        assert_eq!(text, "The attempt");
        assert_eq!(cues[0].name, "clip");
        assert_eq!(cues[0].attr("in"), Some("00:01:12"));
        assert_eq!(cues[0].id.as_deref(), Some("c1"));
    }

    #[test]
    fn directive_spans_point_into_the_source() {
        let s = parse(SAMPLE);
        let Block::Divider { cue, .. } = &s.blocks[2] else {
            panic!()
        };
        assert_eq!(
            &SAMPLE[cue.span.clone()],
            ":transition[whip-pan]{duration=0.4 #t1}"
        );
        assert_eq!(cue.display(), "whip-pan 0.4s");
    }

    #[test]
    fn inline_cues_split_text() {
        let segs = segments("Never drifted. :sfx[record-scratch]{#s1} Again.", 0);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0], Segment::Text("Never drifted.".into()));
        assert!(
            matches!(&segs[1], Segment::Cue(d) if d.label.as_deref() == Some("record-scratch"))
        );
        assert_eq!(segs[2], Segment::Text("Again.".into()));
    }

    #[test]
    fn colons_in_prose_are_not_directives() {
        for line in [
            "Target runtime: ~10:00",
            "[ON SCREEN: Black.]",
            "Note :this is fine",
        ] {
            let segs = segments(line, 0);
            assert!(segs.iter().all(|s| matches!(s, Segment::Text(_))), "{line}");
        }
    }

    #[test]
    fn quoted_attributes() {
        let segs = segments(r#":music{mood="slow build" #m2}"#, 0);
        let Segment::Cue(d) = &segs[0] else { panic!() };
        assert_eq!(d.attr("mood"), Some("slow build"));
        assert_eq!(d.id.as_deref(), Some("m2"));
        assert_eq!(d.display(), "mood=slow build");
    }

    #[test]
    fn outline_counts_spoken_words_only() {
        let s = parse(
            "## A\none two three four five\n[ON SCREEN: ten words that should not count toward speech at all]\n## B\nsix\n",
        );
        let o = s.outline();
        assert_eq!(o.len(), 2);
        assert_eq!(o[0].spoken_words, 5);
        assert_eq!(o[1].spoken_words, 1);
        assert!((o[0].est_seconds - 2.0).abs() < 1e-4);
    }

    #[test]
    fn crlf_and_unclosed_front_matter() {
        let s = parse("## A\r\nhello there\r\n");
        assert_eq!(s.blocks.len(), 2);
        let s = parse("---\ntitle: x\n## not closed\n");
        assert!(s.front_matter.is_none());
    }

    #[test]
    fn unicode_line_starts_do_not_panic() {
        for src in ["ééé hello\n", "a😀 hello\n", "é\n", "VOé\n", "LIVE😀\n"] {
            let s = parse(src);
            assert_eq!(s.blocks.len(), 1, "{src}");
        }
        assert_eq!(kind_prefix("ééé hello"), None);
        assert_eq!(kind_prefix("live: hi"), Some((LineKind::Live, 6)));
    }

    #[test]
    fn formatting_marks_are_not_words() {
        assert_eq!(word_count(&segments("say ** ** now", 0)), 2);
        assert_eq!(
            word_count(&segments("I [word]{color=red}, next", 0)),
            word_count(&segments("I word, next", 0))
        );
    }
}
