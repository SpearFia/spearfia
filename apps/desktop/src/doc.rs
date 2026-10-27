//! The document engine shared by the Script page (editable) and the Media stage's script
//! view (read-only), so both show the script in exactly the same format.
//!
//! `script.md` is laid out as one styled text, like a word processor page:
//!
//! - headings, VO, `LIVE:`, on-screen and note lines each get their paragraph style;
//! - `**bold**`, `*italic*`, `++underline++`, `~~strike~~` and `[links](url)` are styled;
//! - cues (`:sfx[boom]{#s1}`) show as colored chips with only their label;
//! - a line that is only a transition is drawn as a centered divider.
//!
//! The Markdown syntax is still in the text (so the galley matches the file byte for byte,
//! which is what lets egui edit it). It is hidden, except on the line with the caret, where
//! it is shown so it can be edited. Margin decorations (VO/LIVE tags, coverage bars,
//! bullets, checkboxes, divider rules, section times) are painted after layout.

use crate::theme;
use egui::text::{ByteIndex, CCursor, LayoutJob, LayoutSection, TextFormat};
use egui::{Align, Color32, FontFamily, FontId, Galley, Pos2, Rect, Stroke, pos2, vec2};
use spearfia_script::{Block, LineKind, Script, attrs, kind_prefix, segments};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

/// Page width at 100% (US Letter at 90 px/in).
pub const PAGE_W: f32 = 765.0;
/// Page height at 100%, for page breaks.
pub const PAGE_H: f32 = 990.0;
pub const MARGIN_L: f32 = 96.0;
pub const MARGIN_R: f32 = 80.0;
pub const MARGIN_T: f32 = 72.0;
pub const MARGIN_B: f32 = 72.0;

/// Width of the text column at `zoom`.
pub fn text_width(zoom: f32) -> f32 {
    (PAGE_W - MARGIN_L - MARGIN_R) * zoom
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Quote,
    Visual(spearfia_core::visuals::Role),
    Bullet,
    Numbered,
    Check(bool),
    Table,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    FrontMatter,
    Blank,
    Title,
    H1,
    H2,
    H3,
    Spoken,
    Live,
    OnScreen,
    Note(NoteKind),
    Divider,
}

/// One line of the source.
#[derive(Debug, Clone)]
pub struct Line {
    /// Byte range without the line ending.
    pub range: Range<usize>,
    pub style: LineStyle,
    /// Bytes of style markers at the start (`## `, `LIVE: `, `- [ ] `), from `range.start`.
    pub prefix: usize,
    /// The script block on this line, if any.
    pub block: Option<usize>,
}

/// Classify every line of `src`. `script` must be `parse(src)`.
pub fn lines(src: &str, script: &Script) -> Vec<Line> {
    let block_of: HashMap<usize, usize> = script
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let line = match b {
                Block::Heading { line, .. }
                | Block::Line { line, .. }
                | Block::Divider { line, .. } => *line,
            };
            (line, i)
        })
        .collect();
    let fm_end = front_matter_lines(src);
    let mut out = Vec::new();
    let mut start = 0;
    for (n, raw) in src.split_inclusive('\n').enumerate() {
        let body = raw.trim_end_matches(['\n', '\r']);
        let range = start..start + body.len();
        start += raw.len();
        let block = block_of.get(&n).copied();
        let (style, prefix) = if n < fm_end {
            (LineStyle::FrontMatter, 0)
        } else {
            classify(body, block.and_then(|b| script.blocks.get(b)))
        };
        out.push(Line {
            range,
            style,
            prefix,
            block,
        });
    }
    // An empty last line after a final newline still needs a row for the caret.
    if src.is_empty() || src.ends_with('\n') {
        out.push(Line {
            range: src.len()..src.len(),
            style: LineStyle::Blank,
            prefix: 0,
            block: None,
        });
    }
    out
}

/// Number of lines taken by front matter (0 if none).
fn front_matter_lines(src: &str) -> usize {
    if !(src.starts_with("---\n") || src.starts_with("---\r\n")) {
        return 0;
    }
    for (i, l) in src.split_inclusive('\n').enumerate().skip(1) {
        if l.trim_end() == "---" {
            return i + 1;
        }
    }
    0
}

fn classify(body: &str, block: Option<&Block>) -> (LineStyle, usize) {
    let lead = body.len() - body.trim_start().len();
    let t = &body[lead..];
    if t.starts_with("<!-- visual-cue ") {
        return (LineStyle::FrontMatter, 0);
    }
    match block {
        None => (LineStyle::Blank, 0),
        Some(Block::Divider { .. }) => (LineStyle::Divider, 0),
        Some(Block::Heading { level, .. }) => {
            let hashes = t.bytes().take_while(|&b| b == b'#').count();
            let pad = t[hashes..].len() - t[hashes..].trim_start().len();
            let style = match level {
                1 => LineStyle::Title,
                2 => LineStyle::H1,
                3 => LineStyle::H2,
                _ => LineStyle::H3,
            };
            (style, lead + hashes + pad)
        }
        Some(Block::Line { kind, .. }) => match kind {
            LineKind::Spoken => (LineStyle::Spoken, kind_prefix(body).map_or(0, |(_, n)| n)),
            LineKind::Live => (LineStyle::Live, kind_prefix(body).map_or(0, |(_, n)| n)),
            LineKind::OnScreen => (LineStyle::OnScreen, 0),
            LineKind::Note => {
                if let Some(role) = spearfia_core::visuals::Role::of(t) {
                    return (
                        LineStyle::Note(NoteKind::Visual(role)),
                        lead + role.prefix().len()
                            + usize::from(
                                t.get(role.prefix().len()..)
                                    .is_some_and(|s| s.starts_with(' ')),
                            ),
                    );
                }
                for (p, k) in [
                    ("- [ ] ", NoteKind::Check(false)),
                    ("- [x] ", NoteKind::Check(true)),
                    ("- ", NoteKind::Bullet),
                    ("* ", NoteKind::Bullet),
                    ("> ", NoteKind::Quote),
                ] {
                    if t.starts_with(p) {
                        return (LineStyle::Note(k), lead + p.len());
                    }
                }
                if t.starts_with('|') {
                    (LineStyle::Note(NoteKind::Table), 0)
                } else if t == ">" {
                    (LineStyle::Note(NoteKind::Quote), lead + 1)
                } else {
                    (LineStyle::Note(NoteKind::Numbered), 0)
                }
            }
        },
    }
}

/// A highlight laid over a byte range: links, the held selection, comments, find results.
#[derive(Debug, Clone)]
pub struct Mark {
    pub range: Range<usize>,
    pub bg: Color32,
    pub underline: Option<Color32>,
    pub color: Option<Color32>,
}

impl Mark {
    pub fn bg(range: Range<usize>, bg: Color32) -> Self {
        Self {
            range,
            bg,
            underline: None,
            color: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub zoom: f32,
    /// Line whose syntax is shown (the caret line while editing).
    pub reveal: Option<usize>,
    pub width: f32,
}

const HIDE: u8 = 1;
const SYNTAX: u8 = 2;
const BOLD: u8 = 4;
const ITALIC: u8 = 8;
const UNDER: u8 = 16;
const STRIKE: u8 = 32;
const LINK: u8 = 64;

fn family(name: &str) -> FontFamily {
    FontFamily::Name(name.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

impl HAlign {
    pub fn parse(v: &str) -> Option<Self> {
        match v {
            "left" => Some(Self::Left),
            "center" | "centre" => Some(Self::Center),
            "right" => Some(Self::Right),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

/// A paragraph's resolved formatting: built-in style, then the front-matter style, then the
/// line's `{...}` override. Inline spans apply on top.
#[derive(Debug, Clone, PartialEq)]
pub struct Para {
    /// Font name; empty means the monospace UI font (front matter).
    pub font: String,
    pub size: f32,
    pub bold: bool,
    pub semibold: bool,
    pub italic: bool,
    pub color: Color32,
    pub highlight: Option<Color32>,
    pub align: HAlign,
    /// Line spacing as in a word processor (1.0 single, 1.15, 1.5, 2.0); line height is
    /// `size × 1.35 × spacing`.
    pub spacing: f32,
}

/// The front-matter style key for a paragraph style.
pub fn style_key(style: LineStyle) -> Option<&'static str> {
    Some(match style {
        LineStyle::Title => "title",
        LineStyle::H1 => "h1",
        LineStyle::H2 => "h2",
        LineStyle::H3 => "h3",
        LineStyle::Spoken => "vo",
        LineStyle::Live => "live",
        LineStyle::OnScreen => "onscreen",
        LineStyle::Note(_) => "note",
        LineStyle::FrontMatter | LineStyle::Blank | LineStyle::Divider => return None,
    })
}

/// Built-in look of each paragraph style.
pub fn builtin(style: LineStyle) -> Para {
    let p =
        |size: f32, bold: bool, semibold: bool, italic: bool, color: Color32, spacing: f32| Para {
            font: crate::fonts::DEFAULT.to_owned(),
            size,
            bold,
            semibold,
            italic,
            color,
            highlight: None,
            align: HAlign::Left,
            spacing,
        };
    match style {
        LineStyle::FrontMatter => Para {
            font: String::new(),
            ..p(11.5, false, false, false, theme::TEXT_MUTED, 1.1)
        },
        LineStyle::Blank => p(15.0, false, false, false, theme::TEXT, 0.67),
        LineStyle::Title => Para {
            align: HAlign::Center,
            ..p(30.0, true, false, false, theme::TEXT, 1.05)
        },
        LineStyle::H1 => p(22.0, true, false, false, theme::TEXT, 1.07),
        LineStyle::H2 => p(18.0, false, true, false, theme::TEXT, 1.07),
        LineStyle::H3 => p(15.5, false, true, false, theme::TEXT_2, 1.1),
        LineStyle::Spoken | LineStyle::Live => p(15.5, false, false, false, theme::TEXT, 1.2),
        LineStyle::OnScreen => p(14.5, false, false, true, theme::TEXT_2, 1.2),
        LineStyle::Note(NoteKind::Table) => p(13.5, false, false, false, theme::TEXT_2, 1.18),
        LineStyle::Note(NoteKind::Quote) => p(14.5, false, false, true, theme::TEXT_2, 1.2),
        LineStyle::Note(_) => p(14.5, false, false, false, theme::TEXT_2, 1.2),
        LineStyle::Divider => Para {
            align: HAlign::Center,
            ..p(13.0, false, true, false, theme::CUE_TRANSITION, 1.6)
        },
    }
}

/// Parse a color value into an egui color.
pub fn color_of(v: &str) -> Option<Color32> {
    attrs::color(v).map(|[r, g, b]| Color32::from_rgb(r, g, b))
}

/// Apply `font`, `size`, `color`, `highlight`, `bold`, `italic`, `align`, `spacing`.
pub fn apply(p: &mut Para, a: &attrs::Attrs) {
    for (k, v) in a {
        match k.as_str() {
            "font" if !v.is_empty() => p.font = v.clone(),
            "size" => {
                if let Ok(x) = v.parse::<f32>() {
                    p.size = x.clamp(4.0, 200.0);
                }
            }
            "color" => {
                if let Some(c) = color_of(v) {
                    p.color = c;
                }
            }
            "highlight" => p.highlight = color_of(v).map(|c| theme::tint(c, 0.38)),
            "bold" => p.bold = v == "true",
            "italic" => p.italic = v == "true",
            "align" => {
                if let Some(x) = HAlign::parse(v) {
                    p.align = x;
                }
            }
            "spacing" => {
                if let Ok(x) = v.parse::<f32>() {
                    p.spacing = x.clamp(0.5, 4.0);
                }
            }
            _ => {}
        }
    }
}

/// The `styles:` in `src`'s front matter.
pub fn sheet(src: &str) -> Vec<(String, attrs::Attrs)> {
    let n = front_matter_lines(src);
    if n == 0 {
        return Vec::new();
    }
    let raw: String = src.split_inclusive('\n').take(n).collect();
    attrs::styles(&raw)
}

/// A style as the document's front matter defines it (built-in + `styles:`).
pub fn styled(style: LineStyle, sheet: &[(String, attrs::Attrs)]) -> Para {
    let mut p = builtin(style);
    if let Some(k) = style_key(style)
        && let Some((_, a)) = sheet.iter().find(|(s, _)| s == k)
    {
        apply(&mut p, a);
    }
    p
}

/// The formatting of line `n` (style + override).
pub fn para_of(src: &str, l: &Line, sheet: &[(String, attrs::Attrs)]) -> Para {
    let mut p = styled(l.style, sheet);
    if !matches!(l.style, LineStyle::FrontMatter | LineStyle::Blank)
        && let Some((_, a)) = attrs::para_attrs(&src[l.range.clone()])
    {
        apply(&mut p, &a);
    }
    p
}

/// The formatting at byte `at` (paragraph, then any span around it).
pub fn props_at(src: &str, lines: &[Line], at: usize) -> Para {
    let sheet = sheet(src);
    let n = attrs_line(src, at);
    let Some(l) = lines.get(n) else {
        return builtin(LineStyle::Spoken);
    };
    let mut p = para_of(src, l, &sheet);
    for sp in attrs::spans(&src[l.range.clone()], l.range.start) {
        if sp.inner.start <= at && at <= sp.inner.end {
            apply(&mut p, &sp.attrs);
        }
    }
    p
}

fn attrs_line(src: &str, at: usize) -> usize {
    src.as_bytes()[..at.min(src.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// The egui font for a paragraph's font, falling back to the default font while a font
/// is still loading. Returns the font and whether italic must be synthesized.
fn font_id(
    ctx: &egui::Context,
    font: &str,
    size: f32,
    bold: bool,
    semibold: bool,
    italic: bool,
) -> (FontId, bool) {
    use crate::fonts::{self, Face};
    if font.is_empty() {
        return (FontId::monospace(size), false);
    }
    let (name, faces) = match fonts::ready(ctx, font) {
        Some(f) => (font, f),
        None => match fonts::ready(ctx, fonts::DEFAULT) {
            Some(f) => (fonts::DEFAULT, f),
            None => return (FontId::proportional(size), italic),
        },
    };
    let (face, fake_italic) = match (bold, semibold, italic) {
        (true, _, true) if faces.bold_italic => (Face::BoldItalic, false),
        (true, _, i) if faces.bold => (Face::Bold, i),
        (true, _, i) if faces.semibold => (Face::SemiBold, i),
        (false, true, i) if faces.semibold => (Face::SemiBold, i),
        (false, true, i) if faces.bold => (Face::Bold, i),
        (_, _, true) if faces.italic => (Face::Italic, false),
        (_, _, i) => (Face::Regular, i),
    };
    (FontId::new(size, fonts::family(name, face)), fake_italic)
}

/// Extra space above a paragraph of this style, added to the end of the line before it.
fn space_before(style: LineStyle, zoom: f32) -> f32 {
    zoom * match style {
        LineStyle::Title => 6.0,
        LineStyle::H1 => 22.0,
        LineStyle::H2 => 14.0,
        LineStyle::H3 => 8.0,
        LineStyle::Divider => 4.0,
        _ => 0.0,
    }
}

/// (open marker, inner text, close marker, flag), as byte ranges.
type InlineMark = (Range<usize>, Range<usize>, Range<usize>, u8);

/// Inline marks found in a line: (open marker, inner, close marker, flag).
fn inline_marks(text: &str, offset: usize, skip: &[Range<usize>]) -> Vec<InlineMark> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let blocked = |i: usize| skip.iter().any(|r| r.contains(&(offset + i)));
    let mut i = 0;
    while i < b.len() {
        if blocked(i) {
            i += 1;
            continue;
        }
        // [text](url)
        if b[i] == b'['
            && let Some(close) = text[i + 1..].find("](")
            && let Some(end) = text[i + 1 + close + 2..].find(')')
        {
            let inner = i + 1..i + 1 + close;
            let url_end = i + 1 + close + 2 + end + 1;
            if !text[inner.clone()].contains('[') && !inner.is_empty() {
                out.push((
                    offset + i..offset + i + 1,
                    offset + inner.start..offset + inner.end,
                    offset + inner.end..offset + url_end,
                    LINK,
                ));
                i = url_end;
                continue;
            }
        }
        let mut matched = false;
        for (m, flag) in [("**", BOLD), ("++", UNDER), ("~~", STRIKE), ("*", ITALIC)] {
            if text[i..].starts_with(m) {
                let from = i + m.len();
                if let Some(rel) = text[from..].find(m) {
                    let close = from + rel;
                    // `*` must not be the start of `**`, and the inner text not empty.
                    if close > from && !(m == "*" && text[from..].starts_with('*')) {
                        out.push((
                            offset + i..offset + from,
                            offset + from..offset + close,
                            offset + close..offset + close + m.len(),
                            flag,
                        ));
                        i = close + m.len();
                        matched = true;
                        break;
                    }
                }
            }
        }
        if !matched {
            i += text[i..].chars().next().map_or(1, char::len_utf8);
        }
    }
    out
}

/// What decides a run's format, per byte.
#[derive(Clone, Copy, PartialEq)]
struct Key {
    flags: u8,
    chip: Option<Color32>,
    span: Option<usize>,
    bg: Color32,
    underline: Option<Color32>,
    color: Option<Color32>,
}

/// Build the layout for `src`. The text of the job is `src` itself.
pub fn job(
    painter: &egui::Painter,
    src: &str,
    lines: &[Line],
    marks: &[Mark],
    look: Look,
) -> LayoutJob {
    let ctx = painter.ctx().clone();
    let mut job = LayoutJob {
        text: src.to_owned(),
        ..Default::default()
    };
    job.wrap.max_width = look.width;
    let z = look.zoom;
    let sheet = sheet(src);
    for (n, l) in lines.iter().enumerate() {
        let reveal = look.reveal == Some(n);
        let para = para_of(src, l, &sheet);
        let line_h = if src
            .get(l.range.clone())
            .is_some_and(|s| s.starts_with("<!-- visual-cue "))
        {
            44.0 * z
        } else {
            para.size * z * 1.35 * para.spacing
        };
        let text = &src[l.range.clone()];
        let len = text.len();
        let mut flags = vec![
            if text.starts_with("<!-- visual-cue ") {
                HIDE
            } else {
                0u8
            };
            len
        ];
        let mut chip: Vec<Option<Color32>> = vec![None; len];
        let mut span_of: Vec<Option<usize>> = vec![None; len];
        let mut spans: Vec<attrs::Span> = Vec::new();
        let hide_or_syntax = if reveal { SYNTAX } else { HIDE };
        let mut leading = 0.0f32;
        let first_section = job.sections.len();

        if l.style != LineStyle::FrontMatter && l.style != LineStyle::Blank {
            // Style prefix.
            for f in flags.iter_mut().take(l.prefix) {
                *f |= hide_or_syntax;
            }
            if !reveal {
                leading = match l.style {
                    LineStyle::Note(NoteKind::Bullet | NoteKind::Check(_)) => 26.0 * z,
                    LineStyle::Note(NoteKind::Quote) => 18.0 * z,
                    _ => 0.0,
                };
            }
            // Paragraph override `{...}` at the end.
            if let Some((r, _)) = attrs::para_attrs(text) {
                for f in &mut flags[r] {
                    *f |= hide_or_syntax;
                }
            }
            // Cues.
            let segs = segments(text, l.range.start);
            let mut cue_spans: Vec<Range<usize>> = Vec::new();
            for s in &segs {
                let spearfia_script::Segment::Cue(d) = s else {
                    continue;
                };
                let span = d.span.start - l.range.start..d.span.end - l.range.start;
                cue_spans.push(d.span.clone());
                let color = crate::widgets::cue_color(&d.name);
                let raw = &text[span.clone()];
                // The visible label: inside [...] if any, else inside {...}.
                let label = if let Some(lb) = raw.find('[') {
                    let rb = raw[lb..].find(']').map_or(raw.len(), |x| lb + x);
                    span.start + lb + 1..span.start + rb
                } else if let Some(lb) = raw.find('{') {
                    let rb = raw.rfind('}').unwrap_or(raw.len());
                    span.start + lb + 1..span.start + rb
                } else {
                    span.clone()
                };
                for i in span.clone() {
                    if reveal {
                        chip[i] = Some(color);
                        flags[i] |= SYNTAX;
                    } else if label.contains(&i) {
                        chip[i] = Some(color);
                    } else {
                        flags[i] |= HIDE;
                    }
                }
            }
            // Formatting spans `[text]{...}`.
            spans = attrs::spans(text, l.range.start);
            for (si, sp) in spans.iter().enumerate() {
                for i in sp.open.clone().chain(sp.close.clone()) {
                    flags[i - l.range.start] |= hide_or_syntax;
                }
                for i in sp.inner.clone() {
                    span_of[i - l.range.start] = Some(si);
                }
                cue_spans.push(sp.open.clone());
                cue_spans.push(sp.close.clone());
            }
            // Inline marks (outside cues and span markup).
            if !matches!(l.style, LineStyle::Note(NoteKind::Table)) {
                for (open, inner, close, flag) in inline_marks(text, l.range.start, &cue_spans) {
                    for i in open.chain(close) {
                        flags[i - l.range.start] |= hide_or_syntax;
                    }
                    for i in inner {
                        flags[i - l.range.start] |= flag;
                    }
                }
            } else {
                for (i, c) in text.bytes().enumerate() {
                    if c == b'|' || (c == b'-' && text.trim_start().starts_with("|-")) {
                        flags[i] |= SYNTAX;
                    }
                }
            }
        }

        // Emit sections: runs of equal formatting, split at char boundaries.
        let key = |i: usize| -> Key {
            let abs = l.range.start + i;
            let (mut bg, mut underline, mut color) = (Color32::TRANSPARENT, None, None);
            for m in marks {
                if m.range.contains(&abs) {
                    if m.bg != Color32::TRANSPARENT {
                        bg = m.bg;
                    }
                    underline = m.underline.or(underline);
                    color = m.color.or(color);
                }
            }
            Key {
                flags: flags[i],
                chip: chip[i],
                span: span_of[i],
                bg,
                underline,
                color,
            }
        };
        let mut first_visible = true;
        let mut push = |job: &mut LayoutJob, r: Range<usize>, k: Key| {
            let mut f = TextFormat {
                line_height: Some(line_h),
                valign: Align::TOP,
                ..Default::default()
            };
            let mut lead = 0.0;
            if k.flags & HIDE != 0 {
                // Hidden syntax: tiny, transparent, and letter-spaced back to ~zero width.
                f.font_id = FontId::proportional(0.5);
                f.color = Color32::TRANSPARENT;
                f.line_height = text.starts_with("<!-- visual-cue ").then_some(line_h);
                f.extra_letter_spacing = -0.2;
            } else {
                if first_visible {
                    lead = leading;
                    first_visible = false;
                }
                if k.flags & SYNTAX != 0 {
                    f.font_id = FontId::monospace((para.size * z * 0.82).max(9.0));
                    f.color = k.chip.unwrap_or(theme::TEXT_MUTED);
                } else if let Some(c) = k.chip {
                    f.font_id = FontId::new(12.5 * z, family(theme::DOC_SEMIBOLD));
                    f.color = c;
                    f.background = theme::tint(c, 0.16);
                    f.expand_bg = 3.0 * z;
                    // Natural height so the chip's background hugs the label.
                    f.line_height = None;
                    f.valign = Align::Center;
                } else {
                    let mut p = para.clone();
                    if let Some(si) = k.span {
                        apply(&mut p, &spans[si].attrs);
                    }
                    let bold = p.bold || k.flags & BOLD != 0;
                    let italic = p.italic || k.flags & ITALIC != 0;
                    let (fid, fake) = font_id(&ctx, &p.font, p.size * z, bold, p.semibold, italic);
                    f.font_id = fid;
                    f.italics = fake;
                    f.color = p.color;
                    if p.size > para.size {
                        f.line_height = Some(p.size * z * 1.35 * para.spacing);
                    }
                    if let Some(h) = p.highlight {
                        f.background = h;
                        f.expand_bg = 1.0 * z;
                    }
                    if k.flags & UNDER != 0 {
                        f.underline = Stroke::new(1.0 * z, f.color);
                    }
                    if k.flags & STRIKE != 0 {
                        f.strikethrough = Stroke::new(1.2 * z, f.color);
                    }
                    if k.flags & LINK != 0 {
                        f.color = theme::ACCENT;
                        f.underline = Stroke::new(1.0 * z, theme::tint(theme::ACCENT, 0.7));
                    }
                }
                if k.bg != Color32::TRANSPARENT {
                    f.background = k.bg;
                    f.expand_bg = 1.5 * z;
                }
                if let Some(u) = k.underline {
                    f.underline = Stroke::new(2.0 * z, u);
                }
                if let Some(c) = k.color {
                    f.color = c;
                }
            }
            job.sections.push(LayoutSection {
                leading_space: lead,
                byte_range: ByteIndex(l.range.start + r.start)..ByteIndex(l.range.start + r.end),
                format: f,
            });
        };
        let mut run_start = 0usize;
        let mut i = 0usize;
        let mut cur = if len > 0 { Some(key(0)) } else { None };
        while i < len {
            let ch = text[i..].chars().next().map_or(1, char::len_utf8);
            let next = i + ch;
            if next < len {
                let k = key(next);
                if Some(k) != cur {
                    push(&mut job, run_start..next, cur.unwrap_or(k));
                    run_start = next;
                    cur = Some(k);
                }
            }
            i = next;
        }
        if len > 0 {
            push(&mut job, run_start..len, cur.unwrap_or_else(|| key(0)));
        }
        // Centered / right-aligned paragraphs: indent each row by its free space.
        if para.align != HAlign::Left && len > 0 && !reveal {
            align_rows(
                painter,
                &mut job,
                first_section,
                l.range.clone(),
                para.align,
                look.width,
            );
        }
        // Line ending (or an empty line's row) in the paragraph's format.
        let end = if n + 1 < lines.len() {
            lines[n + 1].range.start
        } else {
            src.len()
        };
        if end > l.range.end {
            let (fid, _) = font_id(
                &ctx,
                &para.font,
                para.size * z,
                para.bold,
                para.semibold,
                para.italic,
            );
            let mut f = TextFormat {
                font_id: fid,
                line_height: Some(line_h),
                valign: Align::TOP,
                ..Default::default()
            };
            if len == 0 {
                f.font_id.size = f.font_id.size.min(15.0 * z);
            }
            if let Some(next) = lines.get(n + 1) {
                let extra = space_before(next.style, z);
                if extra > 0.0 {
                    f.line_height = Some(line_h + extra);
                }
            }
            job.sections.push(LayoutSection {
                leading_space: if len == 0 { leading } else { 0.0 },
                byte_range: ByteIndex(l.range.end)..ByteIndex(end),
                format: f,
            });
        }
    }
    if job.sections.is_empty() {
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(0)..ByteIndex(0),
            format: TextFormat::default(),
        });
    }
    job
}

/// Lay out one paragraph on its own to find where its rows break, then give each row's
/// first section the leading space that centers (or right-aligns) it.
fn align_rows(
    painter: &egui::Painter,
    job: &mut LayoutJob,
    first: usize,
    range: Range<usize>,
    align: HAlign,
    width: f32,
) {
    let start = range.start;
    let mut sub = LayoutJob {
        text: job.text[range.clone()].to_owned(),
        ..Default::default()
    };
    sub.wrap.max_width = width;
    for s in &job.sections[first..] {
        let mut s = s.clone();
        s.byte_range =
            ByteIndex(s.byte_range.start.0 - start)..ByteIndex(s.byte_range.end.0 - start);
        sub.sections.push(s);
    }
    let g = painter.layout_job(sub);
    // Byte offset (in the paragraph) where each row starts, and its used width.
    let text = &job.text[range];
    let mut rows: Vec<(usize, f32)> = Vec::new();
    let mut chars = 0usize;
    for r in &g.rows {
        let byte = text
            .char_indices()
            .nth(chars)
            .map_or(text.len(), |(b, _)| b);
        let used = r
            .row
            .glyphs
            .iter()
            .filter(|gl| !gl.chr.is_whitespace() && gl.advance_width > 1.0)
            .map(|gl| gl.pos.x + gl.advance_width)
            .fold(0.0f32, f32::max);
        rows.push((byte, used));
        chars += r.row.glyphs.len() + usize::from(r.ends_with_newline);
    }
    for (byte, used) in rows {
        let free = (width - used - 1.0).max(0.0);
        let lead = match align {
            HAlign::Center => free / 2.0,
            HAlign::Right => free,
            HAlign::Left => 0.0,
        };
        let abs = start + byte;
        // Split the section containing `abs` so a section starts exactly there.
        let Some(i) = (first..job.sections.len()).find(|&i| {
            let r = &job.sections[i].byte_range;
            r.start.0 <= abs && abs < r.end.0
        }) else {
            continue;
        };
        if job.sections[i].byte_range.start.0 < abs {
            let mut tail = job.sections[i].clone();
            tail.byte_range.start = ByteIndex(abs);
            tail.leading_space = 0.0;
            job.sections[i].byte_range.end = ByteIndex(abs);
            job.sections.insert(i + 1, tail);
            job.sections[i + 1].leading_space += lead;
        } else {
            job.sections[i].leading_space += lead;
        }
    }
}

/// Char ↔ byte offsets for a source string (the galley counts chars).
pub struct Index {
    char_of_byte: Vec<u32>,
    byte_of_char: Vec<u32>,
}

impl Index {
    pub fn new(src: &str) -> Self {
        let mut char_of_byte = vec![0u32; src.len() + 1];
        let mut byte_of_char = Vec::with_capacity(src.len() + 1);
        let mut c = 0u32;
        for (b, ch) in src.char_indices() {
            byte_of_char.push(b as u32);
            for k in 0..ch.len_utf8() {
                char_of_byte[b + k] = c;
            }
            c += 1;
        }
        char_of_byte[src.len()] = c;
        byte_of_char.push(src.len() as u32);
        Self {
            char_of_byte,
            byte_of_char,
        }
    }

    pub fn char(&self, byte: usize) -> usize {
        self.char_of_byte[byte.min(self.char_of_byte.len() - 1)] as usize
    }

    pub fn byte(&self, ch: usize) -> usize {
        self.byte_of_char[ch.min(self.byte_of_char.len() - 1)] as usize
    }
}

/// Screen rect of the text between two byte offsets' rows (first row top to last row bottom).
pub fn rows_rect(galley: &Galley, origin: Pos2, idx: &Index, r: &Range<usize>) -> Rect {
    let a = galley.pos_from_cursor(CCursor::new(idx.char(r.start)));
    let b = galley.pos_from_cursor(CCursor::new(idx.char(r.end)));
    Rect::from_min_max(
        pos2(origin.x, origin.y + a.top()),
        pos2(origin.x + galley.size().x.max(1.0), origin.y + b.bottom()),
    )
    .translate(vec2(0.0, 0.0))
}

/// Screen position of a byte offset (the caret rect's top-left).
pub fn pos_of(galley: &Galley, origin: Pos2, idx: &Index, byte: usize) -> Rect {
    galley
        .pos_from_cursor(CCursor::new(idx.char(byte)))
        .translate(origin.to_vec2())
}

/// Byte offset under a screen position.
pub fn byte_at(galley: &Galley, origin: Pos2, idx: &Index, p: Pos2) -> usize {
    idx.byte(galley.cursor_from_pos(p - origin).index.0)
}

/// What the margin shows for each line.
#[derive(Default)]
pub struct Margin<'a> {
    /// Line number → whether media covers it (only set for spoken / on-screen lines).
    pub covered: Option<&'a HashMap<usize, bool>>,
    /// Line number → time label for section headings (`0:35`).
    pub times: Option<&'a HashMap<usize, String>>,
}

/// Paint margin decorations. Returns checkbox hit rects (line number, rect).
pub fn paint_margin(
    painter: &egui::Painter,
    galley: &Arc<Galley>,
    origin: Pos2,
    src: &str,
    lines: &[Line],
    look: Look,
    m: &Margin<'_>,
) -> Vec<(usize, Rect)> {
    let idx = Index::new(src);
    let z = look.zoom;
    let mut checks = Vec::new();
    for (n, l) in lines.iter().enumerate() {
        let rows = rows_rect(galley, origin, &idx, &l.range);
        let first = pos_of(
            galley,
            origin,
            &idx,
            l.range.start + l.prefix.min(l.range.len()),
        );
        let reveal = look.reveal == Some(n);
        match l.style {
            LineStyle::Spoken | LineStyle::Live | LineStyle::OnScreen => {
                let covered = m.covered.and_then(|c| c.get(&n)).copied();
                let bar = match (covered, l.style) {
                    (Some(true), _) => theme::OK,
                    (_, LineStyle::OnScreen) if m.covered.is_some() => {
                        theme::tint(theme::DANGER, 0.75)
                    }
                    _ => theme::BORDER_STRONG,
                };
                let x = origin.x - 62.0 * z;
                painter.rect_filled(
                    Rect::from_min_max(
                        pos2(x, rows.top() + 4.0 * z),
                        pos2(x + 3.0 * z, rows.bottom() - 4.0 * z),
                    ),
                    1.5,
                    bar,
                );
                if l.style != LineStyle::OnScreen {
                    let (tag, color) = if l.style == LineStyle::Live {
                        ("LIVE", theme::CUE_LIVE)
                    } else {
                        ("VO", theme::CUE_VO)
                    };
                    painter.text(
                        pos2(x + 12.0 * z, first.center().y),
                        egui::Align2::LEFT_CENTER,
                        tag,
                        FontId::new(10.5 * z, family(theme::DOC_SEMIBOLD)),
                        color,
                    );
                }
            }
            LineStyle::Note(kind) if !reveal => {
                let y = first.center().y;
                let x = origin.x + 8.0 * z;
                match kind {
                    NoteKind::Visual(role) => {
                        painter.text(
                            pos2(origin.x - 12.0 * z, y),
                            egui::Align2::RIGHT_CENTER,
                            role.prefix().trim_end_matches(':'),
                            FontId::proportional(8.0 * z),
                            theme::CUE_VISION,
                        );
                    }
                    NoteKind::Bullet => {
                        painter.circle_filled(pos2(x + 4.0 * z, y), 2.6 * z, theme::TEXT_2);
                    }
                    NoteKind::Check(done) => {
                        let r = Rect::from_center_size(pos2(x + 5.0 * z, y), vec2(13.0, 13.0) * z);
                        if done {
                            painter.rect_filled(r, 3.0, theme::ACCENT);
                            painter.line_segment(
                                [
                                    r.left_center() + vec2(3.0, 0.0) * z,
                                    r.center_bottom() + vec2(-1.0, -3.5) * z,
                                ],
                                Stroke::new(1.6 * z, theme::ACCENT_ON),
                            );
                            painter.line_segment(
                                [
                                    r.center_bottom() + vec2(-1.0, -3.5) * z,
                                    r.right_top() + vec2(-3.0, 3.5) * z,
                                ],
                                Stroke::new(1.6 * z, theme::ACCENT_ON),
                            );
                        } else {
                            painter.rect_stroke(
                                r,
                                3.0,
                                Stroke::new(1.3 * z, theme::TEXT_2),
                                egui::StrokeKind::Inside,
                            );
                        }
                        checks.push((n, r.expand(3.0)));
                    }
                    NoteKind::Quote => {
                        painter.rect_filled(
                            Rect::from_min_max(
                                pos2(origin.x + 2.0 * z, rows.top() + 3.0),
                                pos2(origin.x + 5.0 * z, rows.bottom() - 3.0),
                            ),
                            1.5,
                            theme::BORDER_STRONG,
                        );
                    }
                    _ => {}
                }
            }
            LineStyle::Divider if !reveal => {
                // Rules either side of the centered chip.
                let text = &src[l.range.clone()];
                let (a, b) = match (text.find('['), text.find(']')) {
                    (Some(a), Some(b)) if b > a => (l.range.start + a + 1, l.range.start + b),
                    _ => (l.range.start, l.range.end),
                };
                let ra = pos_of(galley, origin, &idx, a);
                let rb = pos_of(galley, origin, &idx, b);
                let y = ra.center().y;
                let st = Stroke::new(1.0, theme::tint(theme::CUE_TRANSITION, 0.45));
                painter.line_segment([pos2(origin.x, y), pos2(ra.left() - 12.0 * z, y)], st);
                painter.line_segment(
                    [
                        pos2(rb.right() + 12.0 * z, y),
                        pos2(origin.x + look.width, y),
                    ],
                    st,
                );
            }
            LineStyle::H1 => {
                if let Some(t) = m.times.and_then(|t| t.get(&n)) {
                    painter.text(
                        pos2(origin.x + look.width, first.center().y),
                        egui::Align2::RIGHT_CENTER,
                        t,
                        FontId::monospace(11.0 * z),
                        theme::TEXT_MUTED,
                    );
                }
            }
            _ => {}
        }
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "---\ntitle: T\n---\n# Going in Circles\n## Cold Open :music[hype-drop]{#m1}\n\n[ON SCREEN: black]\n\"Hello **there**.\" :sfx[boom]{#s1}\nLIVE: hi\n:transition[hard cut]{#t1}\n- [ ] get the clip\n> note\n| a | b |\n";

    #[test]
    fn classifies_lines() {
        let s = spearfia_script::parse(SRC);
        let ls = lines(SRC, &s);
        let styles: Vec<LineStyle> = ls.iter().map(|l| l.style).collect();
        assert_eq!(
            styles,
            [
                LineStyle::FrontMatter,
                LineStyle::FrontMatter,
                LineStyle::FrontMatter,
                LineStyle::Title,
                LineStyle::H1,
                LineStyle::Blank,
                LineStyle::OnScreen,
                LineStyle::Spoken,
                LineStyle::Live,
                LineStyle::Divider,
                LineStyle::Note(NoteKind::Check(false)),
                LineStyle::Note(NoteKind::Quote),
                LineStyle::Note(NoteKind::Table),
                LineStyle::Blank,
            ]
        );
        assert_eq!(
            &SRC[ls[4].range.start..ls[4].range.start + ls[4].prefix],
            "## "
        );
        assert_eq!(
            &SRC[ls[8].range.start..ls[8].range.start + ls[8].prefix],
            "LIVE: "
        );
        assert_eq!(ls[7].block, Some(3));
    }

    #[test]
    fn finds_inline_marks() {
        let t = "a **b** *c* ++d++ ~~e~~ [f](g) :sfx[x*y*]{#s}";
        let cue = t.find(":sfx").unwrap()..t.len();
        let m = inline_marks(t, 0, &[cue]);
        let inner: Vec<&str> = m.iter().map(|(_, i, _, _)| &t[i.clone()]).collect();
        assert_eq!(inner, ["b", "c", "d", "e", "f"]);
        assert_eq!(&t[m[4].2.clone()], "](g)");
    }

    #[test]
    fn index_maps_multibyte() {
        let s = "a\u{201C}b";
        let i = Index::new(s);
        assert_eq!(i.char(1), 1);
        assert_eq!(i.char(4), 2);
        assert_eq!(i.byte(2), 4);
        assert_eq!(i.byte(3), 5);
    }
}
