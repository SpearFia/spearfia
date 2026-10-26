//! Text operations behind the editor's toolbar: paragraph styles, bold/italic/underline,
//! lists, indent, clear formatting. They work on `script.md` as text and keep everything
//! else byte for byte. Ranges are byte ranges.
//!
//! Inline marks are Markdown: `**bold**`, `*italic*`, `++underline++`, `~~strike~~`,
//! `[text](url)`.

use crate::{LineKind, Script, Segment, is_note, kind_prefix};
use std::ops::Range;

/// Paragraph styles in the style menu, in menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Style {
    Title,
    Heading1,
    Heading2,
    Heading3,
    Vo,
    Live,
    OnScreen,
    Note,
    Visual(spearfia_core::visuals::Role),
}

impl Style {
    pub const ALL: [Self; 8] = [
        Self::Title,
        Self::Heading1,
        Self::Heading2,
        Self::Heading3,
        Self::Vo,
        Self::Live,
        Self::OnScreen,
        Self::Note,
    ];

    pub const VISUAL: [Self; 11] = [
        Self::Title,
        Self::Heading1,
        Self::Heading2,
        Self::Heading3,
        Self::Visual(spearfia_core::visuals::Role::Visual),
        Self::Visual(spearfia_core::visuals::Role::Action),
        Self::Visual(spearfia_core::visuals::Role::Text),
        Self::Visual(spearfia_core::visuals::Role::Graphic),
        Self::Visual(spearfia_core::visuals::Role::Prompt),
        Self::Visual(spearfia_core::visuals::Role::Continuity),
        Self::Visual(spearfia_core::visuals::Role::Note),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Heading1 => "Heading 1 · section",
            Self::Heading2 => "Heading 2",
            Self::Heading3 => "Heading 3",
            Self::Vo => "VO line",
            Self::Live => "Live line",
            Self::OnScreen => "On-screen",
            Self::Note => "Note",
            Self::Visual(role) => role.label(),
        }
    }

    /// Short label for the toolbar.
    pub fn short(self) -> &'static str {
        match self {
            Self::Heading1 => "Heading 1",
            s => s.label(),
        }
    }

    pub fn shortcut(self) -> &'static str {
        match self {
            Self::Title => "Ctrl+Alt+T",
            Self::Heading1 => "Ctrl+Alt+1",
            Self::Heading2 => "Ctrl+Alt+2",
            Self::Heading3 => "Ctrl+Alt+3",
            Self::Vo => "Ctrl+Alt+0",
            Self::Live => "Ctrl+Alt+L",
            Self::OnScreen => "Ctrl+Alt+O",
            Self::Note => "Ctrl+Alt+N",
            Self::Visual(_) => "",
        }
    }
}

/// List markers for the list buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    Bullet,
    Numbered,
    Check,
}

/// Byte range of zero-based line `n`, without its line ending.
pub fn line_range(src: &str, n: usize) -> Option<Range<usize>> {
    let mut start = 0;
    for (i, l) in src.split_inclusive('\n').enumerate() {
        let body = l.trim_end_matches(['\n', '\r']).len();
        if i == n {
            return Some(start..start + body);
        }
        start += l.len();
    }
    // A trailing empty line after the last `\n`.
    (n == src.split_inclusive('\n').count() && (src.is_empty() || src.ends_with('\n')))
        .then_some(src.len()..src.len())
}

/// Zero-based line number of the byte at `at`.
pub fn line_of(src: &str, at: usize) -> usize {
    src.as_bytes()[..at.min(src.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// Lines touched by a byte range.
pub fn lines_of(src: &str, r: &Range<usize>) -> Range<usize> {
    line_of(src, r.start)..line_of(src, r.end) + 1
}

/// The style of one line of source.
pub fn style_of(line: &str) -> Style {
    let t = line.trim_start();
    let hashes = t.bytes().take_while(|&b| b == b'#').count();
    if hashes > 0 && t[hashes..].starts_with(' ') {
        return match hashes {
            1 => Style::Title,
            2 => Style::Heading1,
            3 => Style::Heading2,
            _ => Style::Heading3,
        };
    }
    if let Some(role) = spearfia_core::visuals::Role::of(t) {
        return Style::Visual(role);
    }
    match kind_prefix(t) {
        Some((LineKind::Live, _)) => return Style::Live,
        Some(_) => return Style::Vo,
        None => {}
    }
    if t.starts_with('[') {
        Style::OnScreen
    } else if t.starts_with('>') || is_note(t) {
        Style::Note
    } else {
        Style::Vo
    }
}

/// The text of a line without its style markers (`## `, `LIVE: `, `> `, `[ON SCREEN: ...]`).
fn strip_style(line: &str) -> String {
    let t = line.trim();
    let hashes = t.bytes().take_while(|&b| b == b'#').count();
    if hashes > 0 && t[hashes..].starts_with(' ') {
        return t[hashes..].trim_start().to_owned();
    }
    if let Some(role) = spearfia_core::visuals::Role::of(t) {
        return t
            .strip_prefix(role.prefix())
            .unwrap_or(t)
            .trim_start()
            .to_owned();
    }
    if let Some((_, n)) = kind_prefix(t) {
        return t[n..].to_owned();
    }
    if let Some(rest) = t.strip_prefix('>') {
        return rest.trim_start().to_owned();
    }
    if let Some(inner) = t.strip_prefix('[')
        && let Some(close) = inner.find(']')
    {
        let body = &inner[..close];
        let body = body
            .strip_prefix("ON SCREEN:")
            .or_else(|| body.strip_prefix("ON-SCREEN:"))
            .unwrap_or(body)
            .trim();
        return format!("{body}{}", &inner[close + 1..]);
    }
    t.to_owned()
}

fn with_style(text: &str, style: Style) -> String {
    match style {
        Style::Title => format!("# {text}"),
        Style::Heading1 => format!("## {text}"),
        Style::Heading2 => format!("### {text}"),
        Style::Heading3 => format!("#### {text}"),
        Style::Vo => text.to_owned(),
        Style::Live => format!("LIVE: {text}"),
        Style::Note => format!("> {text}"),
        Style::Visual(role) => format!("{} {text}", role.prefix()),
        Style::OnScreen => {
            // Keep cues after the bracket: `[ON SCREEN: x] :sfx[...]`.
            let cut = text.find(" :").unwrap_or(text.len());
            format!("[ON SCREEN: {}]{}", text[..cut].trim(), &text[cut..])
        }
    }
}

/// Apply a paragraph style to every line in `lines` (line numbers).
pub fn set_style(src: &str, lines: Range<usize>, style: Style) -> String {
    let trailing = matches!(style, Style::Visual(_))
        && (src.is_empty() || src.ends_with('\n'))
        && lines.contains(&src.split_inclusive('\n').count());
    let mut out = map_lines(src, lines, |l| {
        if l.trim().is_empty() && !matches!(style, Style::Visual(_)) {
            return l.to_owned();
        }
        let lead = &l[..l.len() - l.trim_start().len()];
        format!("{lead}{}", with_style(&strip_style(l), style))
    });
    if trailing {
        out.push_str(&with_style("", style));
    }
    out
}

/// Toggle a list marker on every line in `lines`.
pub fn toggle_list(src: &str, lines: Range<usize>, list: List) -> String {
    let has = |t: &str| match list {
        List::Bullet => t.starts_with("- ") && !t.starts_with("- ["),
        List::Check => t.starts_with("- [ ] ") || t.starts_with("- [x] "),
        List::Numbered => {
            let d = t.bytes().take_while(u8::is_ascii_digit).count();
            d > 0 && t[d..].starts_with(". ")
        }
    };
    let all_on = src
        .split_inclusive('\n')
        .enumerate()
        .filter(|(i, l)| lines.contains(i) && !l.trim().is_empty())
        .all(|(_, l)| has(l.trim_start()));
    let mut n = 0;
    map_lines(src, lines, |l| {
        if l.trim().is_empty() {
            return l.to_owned();
        }
        let lead = &l[..l.len() - l.trim_start().len()];
        let body = strip_list(l.trim_start());
        if all_on {
            return format!("{lead}{body}");
        }
        n += 1;
        let marker = match list {
            List::Bullet => "- ".to_owned(),
            List::Check => "- [ ] ".to_owned(),
            List::Numbered => format!("{n}. "),
        };
        format!("{lead}{marker}{body}")
    })
}

fn strip_list(t: &str) -> &str {
    for p in ["- [ ] ", "- [x] ", "- ", "* ", "> "] {
        if let Some(r) = t.strip_prefix(p) {
            return r;
        }
    }
    let d = t.bytes().take_while(u8::is_ascii_digit).count();
    if d > 0 && t[d..].starts_with(". ") {
        return &t[d + 2..];
    }
    t
}

/// Tick or untick a `- [ ]` checklist item on line `n`.
pub fn toggle_check(src: &str, n: usize) -> String {
    map_lines(src, n..n + 1, |l| {
        if l.contains("- [ ] ") {
            l.replacen("- [ ] ", "- [x] ", 1)
        } else {
            l.replacen("- [x] ", "- [ ] ", 1)
        }
    })
}

/// Indent (`delta > 0`) or outdent every line by two spaces.
pub fn indent(src: &str, lines: Range<usize>, delta: i32) -> String {
    map_lines(src, lines, |l| {
        if delta > 0 {
            format!("  {l}")
        } else {
            l.strip_prefix("  ")
                .or_else(|| l.strip_prefix(' '))
                .unwrap_or(l)
                .to_owned()
        }
    })
}

fn map_lines(src: &str, lines: Range<usize>, mut f: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(src.len() + 16);
    for (i, raw) in src.split_inclusive('\n').enumerate() {
        let body = raw.trim_end_matches(['\n', '\r']);
        if lines.contains(&i) {
            out.push_str(&f(body));
        } else {
            out.push_str(body);
        }
        out.push_str(&raw[body.len()..]);
    }
    out
}

/// Expand an empty range to the word around it.
pub fn word_at(src: &str, at: usize) -> Range<usize> {
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}' || c == '-';
    let mut s = at.min(src.len());
    while s > 0 {
        let c = src[..s].chars().next_back().unwrap_or(' ');
        if !is_word(c) {
            break;
        }
        s -= c.len_utf8();
    }
    let mut e = at.min(src.len());
    while e < src.len() {
        let c = src[e..].chars().next().unwrap_or(' ');
        if !is_word(c) {
            break;
        }
        e += c.len_utf8();
    }
    s..e
}

/// Wrap the range in `marker` (`**`, `*`, `++`, `~~`), or unwrap it if it already is.
/// Returns the new source and the range of the same text afterwards.
pub fn toggle_wrap(src: &str, sel: Range<usize>, marker: &str) -> (String, Range<usize>) {
    let sel = if sel.is_empty() {
        word_at(src, sel.start)
    } else {
        sel
    };
    let m = marker.len();
    // The marker just outside the selection.
    let outside = sel.start >= m
        && src.get(sel.start - m..sel.start) == Some(marker)
        && src.get(sel.end..sel.end + m) == Some(marker)
        // `*` inside `**`: only count it if it isn't part of a longer run.
        && !(marker == "*" && src.get(sel.start.saturating_sub(2)..sel.start) == Some("**"));
    if outside {
        let out = format!(
            "{}{}{}",
            &src[..sel.start - m],
            &src[sel.clone()],
            &src[sel.end + m..]
        );
        return (out, sel.start - m..sel.end - m);
    }
    // The marker inside the selection.
    let inner = &src[sel.clone()];
    if inner.len() >= 2 * m && inner.starts_with(marker) && inner.ends_with(marker) {
        let out = format!(
            "{}{}{}",
            &src[..sel.start],
            &inner[m..inner.len() - m],
            &src[sel.end..]
        );
        return (out, sel.start..sel.end - 2 * m);
    }
    let out = format!(
        "{}{marker}{inner}{marker}{}",
        &src[..sel.start],
        &src[sel.end..]
    );
    (out, sel.start + m..sel.end + m)
}

/// Make the range a link: `[text](url)`.
pub fn make_link(src: &str, sel: Range<usize>, url: &str) -> (String, Range<usize>) {
    let sel = if sel.is_empty() {
        word_at(src, sel.start)
    } else {
        sel
    };
    let text = &src[sel.clone()];
    let out = format!("{}[{text}]({url}){}", &src[..sel.start], &src[sel.end..]);
    (out, sel.start + 1..sel.end + 1)
}

/// Remove inline marks (`**`, `*`, `++`, `~~`) inside the range.
pub fn clear_formatting(src: &str, sel: Range<usize>) -> (String, Range<usize>) {
    let inner = &src[sel.clone()];
    let mut cleaned = inner.to_owned();
    for m in ["**", "++", "~~", "*"] {
        cleaned = cleaned.replace(m, "");
    }
    let out = format!("{}{cleaned}{}", &src[..sel.start], &src[sel.end..]);
    let end = sel.start + cleaned.len();
    (out, sel.start..end)
}

/// Insert `text` at `at`.
pub fn insert(src: &str, at: usize, text: &str) -> String {
    format!("{}{text}{}", &src[..at], &src[at..])
}

/// Insert a new line after line `n`.
pub fn insert_line_after(src: &str, n: usize, text: &str) -> (String, usize) {
    let at = match line_range(src, n) {
        Some(r) => r.end,
        None => src.len(),
    };
    let nl = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let out = format!("{}{nl}{text}{}", &src[..at], &src[at..]);
    (out, at + nl.len() + text.len())
}

/// A word with inline marks trimmed off its ends (`**grip**` → `grip`), for matching
/// link quotes regardless of formatting.
pub fn clean_word(w: &str) -> &str {
    w.trim_matches(|c| matches!(c, '*' | '+' | '~' | '`'))
}

/// The words of a line, cleaned, skipping cues.
pub fn line_words(segments: &[Segment]) -> Vec<&str> {
    segments
        .iter()
        .filter_map(|s| match s {
            Segment::Text(t) => Some(crate::words_in(t)),
            Segment::Cue(_) => None,
        })
        .flatten()
        .map(clean_word)
        .collect()
}

/// The same words as [`line_words`], as byte ranges into `src`, for block `block`.
pub fn word_ranges(src: &str, script: &Script, block: usize) -> Vec<Range<usize>> {
    let Some(crate::Block::Line { segments, line, .. }) = script.blocks.get(block) else {
        return Vec::new();
    };
    let Some(lr) = line_range(src, *line) else {
        return Vec::new();
    };
    let text = &src[lr.clone()];
    let skip = kind_prefix(text).map_or(0, |(_, n)| n);
    // Text outside cues, formatting spans' markup and the paragraph override. A cue ends a
    // word; markup does not (`[word]{color=red},` is one word, as in `line_words`).
    let cues: Vec<Range<usize>> = segments
        .iter()
        .filter_map(|s| match s {
            Segment::Cue(d) => Some(d.span.clone()),
            Segment::Text(_) => None,
        })
        .collect();
    let markup: Vec<Range<usize>> = crate::attrs::spans(text, lr.start)
        .into_iter()
        .flat_map(|s| [s.open, s.close])
        .collect();
    let end = match crate::attrs::para_attrs(text) {
        Some((r, _)) => lr.start + r.start,
        None => lr.end,
    };
    let char_at = |i: usize| src[i..].chars().next().unwrap_or(' ');
    let mut out = Vec::new();
    let mut i = lr.start + skip;
    while i < end {
        if let Some(c) = cues.iter().chain(&markup).find(|c| c.start == i) {
            i = c.end;
            continue;
        }
        let ch = char_at(i);
        if ch.is_whitespace() {
            i += ch.len_utf8();
            continue;
        }
        let s = i;
        // Last byte of visible (non-markup) text in the word.
        let mut last = i;
        while i < end && !cues.iter().any(|c| c.start == i) {
            if let Some(m) = markup.iter().find(|m| m.start == i) {
                i = m.end;
                continue;
            }
            let ch = char_at(i);
            if ch.is_whitespace() {
                break;
            }
            i += ch.len_utf8();
            last = i;
        }
        // Trim inline marks so ranges cover the visible word; skip marks-only tokens.
        let w = &src[s..last];
        if !crate::is_word(w) {
            continue;
        }
        let lead = w.len() - w.trim_start_matches(crate::MARKS).len();
        let trail = w.len() - w.trim_end_matches(crate::MARKS).len();
        let (a, b) = (s + lead, (last - trail).max(s + lead));
        out.push(a..b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn styles_round_trip() {
        let src = "## Cold Open\n\"Hello there.\"\n";
        assert_eq!(style_of("## Cold Open"), Style::Heading1);
        let s = set_style(src, 1..2, Style::Live);
        assert_eq!(s, "## Cold Open\nLIVE: \"Hello there.\"\n");
        assert_eq!(style_of("LIVE: hi"), Style::Live);
        let s = set_style(&s, 1..2, Style::OnScreen);
        assert_eq!(s, "## Cold Open\n[ON SCREEN: \"Hello there.\"]\n");
        let s = set_style(&s, 1..2, Style::Vo);
        assert_eq!(s, "## Cold Open\n\"Hello there.\"\n");
        let s = set_style(&s, 0..1, Style::Heading2);
        assert_eq!(s, "### Cold Open\n\"Hello there.\"\n");
        assert_eq!(
            set_style("[ON SCREEN: x] :sfx[boom]{#s1}", 0..1, Style::Note),
            "> x :sfx[boom]{#s1}"
        );
        assert_eq!(
            set_style("x :sfx[boom]{#s1}", 0..1, Style::OnScreen),
            "[ON SCREEN: x] :sfx[boom]{#s1}"
        );
    }

    #[test]
    fn wraps_and_unwraps() {
        let src = "say hello there";
        let (s, r) = toggle_wrap(src, 4..9, "**");
        assert_eq!(s, "say **hello** there");
        assert_eq!(&s[r.clone()], "hello");
        let (s, r) = toggle_wrap(&s, r, "**");
        assert_eq!(s, src);
        assert_eq!(&s[r], "hello");
        // Empty selection takes the word.
        let (s, _) = toggle_wrap(src, 6..6, "*");
        assert_eq!(s, "say *hello* there");
        let (s, r) = toggle_wrap("a **b** c", 2..7, "**");
        assert_eq!((s.as_str(), r), ("a b c", 2..3));
        let (s, _) = clear_formatting("x **a** *b* ++c++ y", 0..19);
        assert_eq!(s, "x a b c y");
        let (s, r) = make_link("see docs", 4..8, "https://x.dev");
        assert_eq!(s, "see [docs](https://x.dev)");
        assert_eq!(&s[r], "docs");
    }

    #[test]
    fn lists_and_indent() {
        let src = "a\nb\n\nc\n";
        let s = toggle_list(src, 0..4, List::Bullet);
        assert_eq!(s, "- a\n- b\n\n- c\n");
        assert_eq!(toggle_list(&s, 0..4, List::Bullet), src);
        assert_eq!(toggle_list(src, 0..2, List::Numbered), "1. a\n2. b\n\nc\n");
        let c = toggle_list(src, 0..1, List::Check);
        assert_eq!(c, "- [ ] a\nb\n\nc\n");
        assert_eq!(toggle_check(&c, 0), "- [x] a\nb\n\nc\n");
        assert_eq!(indent("- a\n", 0..1, 1), "  - a\n");
        assert_eq!(indent("  - a\n", 0..1, -1), "- a\n");
    }

    #[test]
    fn lines_and_words() {
        let src = "## A\nLIVE: **Hello** there :sfx[boom]{#s1} friend\n> note line\n";
        assert_eq!(line_range(src, 1), Some(5..49));
        assert_eq!(line_of(src, 6), 1);
        let s = parse(src);
        assert!(matches!(
            s.blocks[1],
            crate::Block::Line {
                kind: LineKind::Live,
                ..
            }
        ));
        assert!(matches!(
            s.blocks[2],
            crate::Block::Line {
                kind: LineKind::Note,
                ..
            }
        ));
        let r = word_ranges(src, &s, 1);
        let words: Vec<&str> = r.iter().map(|r| &src[r.clone()]).collect();
        assert_eq!(words, ["Hello", "there", "friend"]);
        let src2 = "say [two words]{color=red} here {align=center}\n";
        let s2 = parse(src2);
        let w2: Vec<&str> = word_ranges(src2, &s2, 0)
            .iter()
            .map(|r| &src2[r.clone()])
            .collect();
        assert_eq!(w2, ["say", "two", "words", "here"]);
        if let crate::Block::Line { segments, .. } = &s2.blocks[0] {
            assert_eq!(line_words(segments), w2);
        }
        if let crate::Block::Line { segments, .. } = &s.blocks[1] {
            assert_eq!(line_words(segments), words);
        }
        assert_eq!(word_at("say hello", 6), 4..9);
    }

    #[test]
    fn word_ranges_join_across_markup() {
        let src = "I [word]{color=red}, next ** **\n";
        let s = parse(src);
        let r = word_ranges(src, &s, 0);
        assert_eq!(r.len(), 3);
        assert_eq!(&src[r[1].clone()], "word]{color=red},");
        if let crate::Block::Line { segments, .. } = &s.blocks[0] {
            assert_eq!(line_words(segments), ["I", "word,", "next"]);
        }
    }
}
