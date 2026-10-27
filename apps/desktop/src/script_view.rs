//! The Media stage's script view: the script as a read-only page, laid out by the same
//! document engine as the Script page ([`crate::doc`]) so it looks exactly the same, plus
//! word selection and drop-to-link.
//!
//! Links color exact words: picture links get a tinted background in the role's color,
//! audio links an underline. Each link's file name sits beside the page, level with it.

use crate::doc::{self, Look, Mark};
use crate::theme;
use egui::{CornerRadius, FontId, Pos2, Rect, Sense, Stroke, pos2, vec2};
use spearfia_core::links::{self, LinkKind, Links};
use spearfia_core::project::MediaRole;
use spearfia_script::format::{self, line_words as words_of};
use spearfia_script::{Block, LineKind, Script, Segment};
use std::collections::HashMap;
use std::sync::Arc;

/// What's dragged from the media panel onto the script.
#[derive(Debug, Clone)]
pub struct MediaDrag {
    pub clips: Vec<spearfia_core::media::MediaFile>,
}

pub fn role_color(role: MediaRole) -> egui::Color32 {
    match role {
        MediaRole::ARoll => theme::CUE_LIVE,
        MediaRole::Voiceover => theme::CUE_VO,
        MediaRole::BRoll => theme::CUE_CLIP,
        MediaRole::Screen => theme::CUE_LOOK,
        MediaRole::Graphics => theme::CUE_VISION,
        MediaRole::Music => theme::CUE_MUSIC,
        MediaRole::Sfx => theme::CUE_SFX,
        MediaRole::Other => theme::TEXT_2,
    }
}

/// A link placed on a line's words.
#[derive(Debug, Clone)]
pub struct Span {
    pub link_id: String,
    pub first: usize,
    pub last: usize,
    pub role: MediaRole,
    pub kind: LinkKind,
    pub label: String,
    pub media_path: String,
}

/// Links resolved against the current script.
#[derive(Debug, Default)]
pub struct Resolved {
    /// Block index → spans on that line.
    pub spans: HashMap<usize, Vec<Span>>,
    /// Links whose words are no longer in the script.
    pub lost: Vec<String>,
}

/// The words of a line, in order, skipping cues and inline formatting marks.
pub fn line_words(segments: &[Segment]) -> Vec<&str> {
    words_of(segments)
}

pub fn resolve(script: &Script, links: &Links) -> Resolved {
    let mut blocks: Vec<usize> = Vec::new();
    let mut lines: Vec<(usize, Vec<&str>)> = Vec::new();
    for (i, b) in script.blocks.iter().enumerate() {
        if let Block::Line { segments, line, .. } = b {
            blocks.push(i);
            lines.push((*line, line_words(segments)));
        }
    }
    let mut out = Resolved::default();
    for l in &links.links {
        match links::locate(&l.quote, l.line, &lines) {
            Some(a) => out.spans.entry(blocks[a.line]).or_default().push(Span {
                link_id: l.id.clone(),
                first: a.first_word,
                last: a.last_word,
                role: l.role,
                kind: l.kind,
                label: file_name(&l.media.path),
                media_path: l.media.path.clone(),
            }),
            None => out.lost.push(l.id.clone()),
        }
    }
    out
}

pub fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}

/// Line number → covered by media, for the margin's coverage bars.
pub fn coverage(script: &Script, resolved: &Resolved) -> HashMap<usize, bool> {
    script
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| match b {
            Block::Line { kind, line, .. } if *kind != LineKind::Note => {
                Some((*line, resolved.spans.contains_key(&i)))
            }
            _ => None,
        })
        .collect()
}

/// Merge link coverage before painting: overlaps become more transparent, not darker.
pub fn link_marks(
    src: &str,
    script: &Script,
    resolved: &Resolved,
    selected: Option<&str>,
) -> Vec<Mark> {
    let mut out = Vec::new();
    for (block, spans) in &resolved.spans {
        let words = format::word_ranges(src, script, *block);
        let mut boundaries: Vec<usize> = spans
            .iter()
            .flat_map(|s| {
                [
                    s.first.min(words.len()),
                    s.last.saturating_add(1).min(words.len()),
                ]
            })
            .collect();
        boundaries.sort_unstable();
        boundaries.dedup();
        for pair in boundaries.windows(2) {
            let (Some(a), Some(b)) = (words.get(pair[0]), words.get(pair[1].saturating_sub(1)))
            else {
                continue;
            };
            let active: Vec<&Span> = spans
                .iter()
                .filter(|s| (s.first..=s.last).contains(&pair[0]))
                .collect();
            let Some(first) = active.first() else {
                continue;
            };
            let count = active
                .iter()
                .map(|s| s.media_path.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len()
                .max(1);
            let strong = active.iter().any(|s| selected == Some(s.link_id.as_str()));
            let c = role_color(first.role);
            out.push(Mark {
                range: a.start..b.end,
                bg: theme::tint(
                    c,
                    (if strong { 0.42 } else { 0.28 }) / (count as f32).sqrt(),
                ),
                underline: active
                    .iter()
                    .any(|s| s.kind == LinkKind::Audio)
                    .then_some(c),
                color: None,
            });
        }
    }
    out
}

/// Inclusive word endpoints, in document order. Blank lines and headings may lie between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub block: usize,
    pub first: usize,
    pub end_block: usize,
    pub last: usize,
}

impl Selection {
    pub fn between(a: (usize, usize), b: (usize, usize)) -> Self {
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        Self {
            block: start.0,
            first: start.1,
            end_block: end.0,
            last: end.1,
        }
    }

    pub fn line(script: &Script, block: usize) -> Option<Self> {
        let Block::Line { kind, segments, .. } = script.blocks.get(block)? else {
            return None;
        };
        if *kind == LineKind::Note {
            return None;
        }
        let last = line_words(segments).len().checked_sub(1)?;
        Some(Self::between((block, 0), (block, last)))
    }

    pub fn section(script: &Script, heading: usize) -> Option<Self> {
        let Block::Heading { level, .. } = script.blocks.get(heading)? else {
            return None;
        };
        let mut parts = script
            .blocks
            .iter()
            .enumerate()
            .skip(heading.saturating_add(1))
            .take_while(|(_, b)| !matches!(b, Block::Heading { level: next, .. } if next <= level))
            .filter_map(|(b, _)| Self::line(script, b));
        let first = parts.next()?;
        let last = parts.last().unwrap_or(first);
        Some(Self::between((first.block, 0), (last.block, last.last)))
    }

    /// Split a selection into existing per-line anchors, keeping links.json compatible.
    pub fn parts(self, script: &Script) -> Vec<Self> {
        if self.end_block < self.block {
            return Vec::new();
        }
        script
            .blocks
            .iter()
            .enumerate()
            .skip(self.block)
            .take(self.end_block.saturating_sub(self.block).saturating_add(1))
            .filter_map(|(b, _)| {
                let mut line = Self::line(script, b)?;
                if b == self.block {
                    line.first = self.first;
                }
                if b == self.end_block {
                    line.last = self.last.min(line.last);
                }
                (line.first <= line.last).then_some(line)
            })
            .collect()
    }

    pub fn quote(self, script: &Script) -> Option<String> {
        let quotes: Vec<String> = self
            .parts(script)
            .iter()
            .filter_map(|part| {
                let Block::Line { segments, .. } = script.blocks.get(part.block)? else {
                    return None;
                };
                Some(line_words(segments).get(part.first..=part.last)?.join(" "))
            })
            .collect();
        (!quotes.is_empty()).then(|| quotes.join("\n"))
    }

    pub fn from_source(src: &str, script: &Script, range: std::ops::Range<usize>) -> Option<Self> {
        let words = source_words(src, script);
        if range.is_empty() {
            let line = format::line_of(src, range.start);
            let (block, b) = script.blocks.iter().enumerate().find(|(_, b)| match b {
                Block::Line { line: n, .. } | Block::Heading { line: n, .. } => *n == line,
                _ => false,
            })?;
            return if matches!(b, Block::Heading { .. }) {
                Self::section(script, block)
            } else {
                Self::line(script, block)
            };
        }
        let mut inside = words
            .iter()
            .filter(|w| w.range.end > range.start && w.range.start < range.end);
        let first = inside.next()?;
        let last = inside.next_back().unwrap_or(first);
        Some(Self::between(
            (first.block, first.index),
            (last.block, last.index),
        ))
    }
}

struct SourceWord {
    block: usize,
    index: usize,
    range: std::ops::Range<usize>,
}

fn source_words(src: &str, script: &Script) -> Vec<SourceWord> {
    script
        .blocks
        .iter()
        .enumerate()
        .filter(|(b, _)| Selection::line(script, *b).is_some())
        .flat_map(|(block, _)| {
            format::word_ranges(src, script, block)
                .into_iter()
                .enumerate()
                .map(move |(index, range)| SourceWord {
                    block,
                    index,
                    range,
                })
        })
        .collect()
}

fn nearest_word(words: &[SourceWord], byte: usize) -> Option<(usize, usize)> {
    words
        .iter()
        .min_by_key(|w| {
            if byte < w.range.start {
                w.range.start - byte
            } else {
                byte.saturating_sub(w.range.end)
            }
        })
        .map(|w| (w.block, w.index))
}

/// Selection state the Media stage keeps between frames.
#[derive(Debug, Default)]
pub struct Interaction {
    /// The held selection: stays while clips are added, until Esc or a new selection.
    pub selection: Option<Selection>,
    /// Where the current selection drag started: (block, word).
    anchor: Option<(usize, usize)>,
    hover_keys: Vec<String>,
    hover_since: f64,
}

/// What happened this frame.
#[derive(Debug, Default)]
pub struct Events {
    /// Media dropped on a word range.
    pub dropped: Option<(Selection, Arc<MediaDrag>)>,
    /// A link tag or linked word was clicked.
    pub clicked_link: Option<String>,
    /// Linked clips under a word after a one-second hover.
    pub hovered_links: Vec<String>,
}

pub struct Options<'a> {
    pub src: &'a str,
    pub resolved: &'a Resolved,
    /// `Some` enables word selection and dropping media.
    pub interaction: Option<&'a mut Interaction>,
    pub scroll_to: Option<usize>,
    pub selected_link: Option<&'a str>,
}

/// Width kept beside the page for link names.
const SIDE_W: f32 = 200.0;

/// The page rectangle, centered in `avail` with `side` px kept free on its right.
pub fn page_rect(avail: Rect, zoom: f32, side: f32, height: f32) -> Rect {
    let w = doc::PAGE_W * zoom;
    let x = avail.left() + ((avail.width() - w - side) / 2.0).max(24.0);
    Rect::from_min_size(pos2(x, avail.top() + 28.0), vec2(w, height))
}

/// Paint the page sheet (shadow, paper, border).
pub fn paint_page(p: &egui::Painter, r: Rect) {
    p.rect_filled(
        r.translate(vec2(0.0, 3.0)).expand(1.0),
        3.0,
        egui::Color32::from_black_alpha(90),
    );
    p.rect(
        r,
        CornerRadius::same(2),
        theme::BG_RAISED,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
}

/// Draw the script as a read-only page, scrollable.
pub fn page(ui: &mut egui::Ui, script: &Script, mut opts: Options<'_>) -> Events {
    let mut ev = Events::default();
    if let Some(i) = opts.interaction.as_deref_mut() {
        if !ui.input(|inp| inp.pointer.primary_down()) {
            i.anchor = None;
        }
        if ui.input(|inp| inp.key_pressed(egui::Key::Escape)) {
            i.selection = None;
        }
    }
    let src = opts.src;
    egui::Frame::new().fill(theme::BG_INSET).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("media-script")
            .auto_shrink([false; 2])
            .show(ui, |ui| body(ui, script, src, &mut opts, &mut ev));
    });
    ev
}

fn body(ui: &mut egui::Ui, script: &Script, src: &str, opts: &mut Options<'_>, ev: &mut Events) {
    let lines = doc::lines(src, script);
    let mut marks = link_marks(src, script, opts.resolved, opts.selected_link);
    if let Some(sel) = opts.interaction.as_deref().and_then(|i| i.selection) {
        for part in sel.parts(script) {
            let words = format::word_ranges(src, script, part.block);
            for (index, range) in words
                .iter()
                .enumerate()
                .filter(|(index, _)| (part.first..=part.last).contains(index))
            {
                let linked =
                    opts.resolved.spans.get(&part.block).is_some_and(|spans| {
                        spans.iter().any(|s| (s.first..=s.last).contains(&index))
                    });
                marks.push(Mark {
                    range: range.clone(),
                    bg: if linked {
                        egui::Color32::TRANSPARENT
                    } else {
                        theme::tint(theme::ACCENT, 0.24)
                    },
                    underline: Some(theme::ACCENT),
                    color: None,
                });
            }
        }
    }
    let look = Look {
        zoom: 1.0,
        reveal: None,
        width: doc::text_width(1.0),
    };
    let job = doc::job(ui.painter(), src, &lines, &marks, look);
    let galley = ui.painter().layout_job(job);
    let avail = ui.available_rect_before_wrap();
    let page = page_rect(
        avail,
        1.0,
        SIDE_W,
        galley.size().y + doc::MARGIN_T + doc::MARGIN_B,
    );
    let (_, _) = ui.allocate_exact_size(vec2(avail.width(), page.height() + 64.0), Sense::hover());
    let origin = page.min + vec2(doc::MARGIN_L, doc::MARGIN_T);
    let p = ui.painter().clone();
    paint_page(&p, page);
    p.galley(origin, galley.clone(), theme::TEXT);
    let covered = coverage(script, opts.resolved);
    doc::paint_margin(
        &p,
        &galley,
        origin,
        src,
        &lines,
        look,
        &doc::Margin {
            covered: Some(&covered),
            times: None,
        },
    );
    let idx = doc::Index::new(src);

    // Link names beside the page, level with the end of each link.
    let mut tags: Vec<(f32, &Span)> = Vec::new();
    for (block, spans) in &opts.resolved.spans {
        let words = format::word_ranges(src, script, *block);
        for s in spans {
            if let Some(w) = words.get(s.last) {
                tags.push((doc::pos_of(&galley, origin, &idx, w.end).center().y, s));
            }
        }
    }
    tags.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut next_y = f32::MIN;
    for (y, s) in tags {
        let y = y.max(next_y);
        let on = opts.selected_link == Some(s.link_id.as_str());
        let color = if on {
            theme::ACCENT
        } else {
            role_color(s.role)
        };
        let g = p.layout_no_wrap(s.label.clone(), FontId::monospace(11.0), color);
        let r = Rect::from_min_size(
            pos2(page.right() + 14.0, y - 10.0),
            g.size() + vec2(14.0, 6.0),
        );
        p.rect(
            r,
            4.0,
            theme::tint(color, 0.12),
            Stroke::new(1.0, theme::tint(color, 0.45)),
            egui::StrokeKind::Inside,
        );
        p.galley(r.min + vec2(7.0, 3.0), g, color);
        let resp = ui
            .interact(r, ui.id().with(("link-tag", &s.link_id)), Sense::click())
            .on_hover_text("Click to select this link");
        if resp.clicked() {
            ev.clicked_link = Some(s.link_id.clone());
        }
        next_y = y + r.height() + 4.0;
    }

    let words = source_words(src, script);
    let word_at = |pt: Pos2| nearest_word(&words, doc::byte_at(&galley, origin, &idx, pt));
    let heading_at = |pt: Pos2| {
        let byte = doc::byte_at(&galley, origin, &idx, pt);
        let block = lines.get(format::line_of(src, byte))?.block?;
        matches!(script.blocks.get(block), Some(Block::Heading { .. })).then_some(block)
    };

    if let Some(target) = opts.scroll_to
        && let Some(
            Block::Heading { line, .. } | Block::Line { line, .. } | Block::Divider { line, .. },
        ) = script.blocks.get(target)
        && let Some(l) = lines.get(*line)
    {
        ui.scroll_to_rect(
            doc::rows_rect(&galley, origin, &idx, &l.range),
            Some(egui::Align::TOP),
        );
    }

    let text_rect = Rect::from_min_size(origin, vec2(look.width, galley.size().y)).expand(4.0);
    let Some(inter) = opts.interaction.as_deref_mut() else {
        return;
    };
    let resp = ui.interact(
        text_rect,
        ui.id().with("media-words"),
        Sense::click_and_drag(),
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    let dragging = egui::DragAndDrop::has_payload_of_type::<MediaDrag>(ui.ctx());
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
    if !dragging && (resp.drag_started() || resp.clicked()) {
        let start = ui.input(|i| i.pointer.press_origin()).or(pointer);
        if let Some(pt) = start {
            if resp.clicked()
                && let Some(heading) = heading_at(pt)
            {
                inter.selection = Selection::section(script, heading);
            } else if let Some(word) = word_at(pt) {
                inter.anchor = Some(word);
                inter.selection = Some(Selection::between(
                    word,
                    pointer.and_then(word_at).unwrap_or(word),
                ));
                if resp.clicked()
                    && let Some(span) = opts.resolved.spans.get(&word.0).and_then(|spans| {
                        spans.iter().find(|s| (s.first..=s.last).contains(&word.1))
                    })
                {
                    ev.clicked_link = Some(span.link_id.clone());
                }
            }
        }
    } else if !dragging
        && resp.dragged()
        && let (Some(anchor), Some(pt)) = (inter.anchor, pointer)
        && let Some(word) = word_at(pt)
    {
        inter.selection = Some(Selection::between(anchor, word));
        let viewport = ui.clip_rect();
        if pt.y < viewport.top() + 22.0 {
            ui.scroll_with_delta(vec2(0.0, 12.0));
        } else if pt.y > viewport.bottom() - 22.0 {
            ui.scroll_with_delta(vec2(0.0, -12.0));
        }
        ui.ctx().request_repaint();
    }

    // Hover only actual linked text, not the nearest word across blank space.
    let hovered: Vec<&Span> = if resp.hovered() && !dragging && !ui.input(|i| i.pointer.any_down())
    {
        pointer
            .and_then(|pt| {
                let byte = doc::byte_at(&galley, origin, &idx, pt);
                let word = words.iter().find(|w| w.range.contains(&byte))?;
                Some(
                    opts.resolved
                        .spans
                        .get(&word.block)
                        .into_iter()
                        .flatten()
                        .filter(|s| (s.first..=s.last).contains(&word.index))
                        .collect(),
                )
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let keys: Vec<String> = hovered.iter().map(|s| s.link_id.clone()).collect();
    let now = ui.input(|i| i.time);
    if keys != inter.hover_keys {
        inter.hover_keys = keys;
        inter.hover_since = now;
    }
    if !hovered.is_empty() {
        let elapsed = now - inter.hover_since;
        if elapsed >= 1.0 {
            ev.hovered_links = inter.hover_keys.clone();
            egui::Tooltip::for_widget(&resp).at_pointer().show(|ui| {
                ui.strong("Linked clips");
                let mut seen = std::collections::HashSet::new();
                for span in &hovered {
                    if seen.insert(&span.media_path) {
                        ui.label(&span.label).on_hover_text(&span.media_path);
                    }
                }
            });
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(
                    (1.0 - elapsed).max(0.0),
                ));
        }
    }

    if dragging
        && let Some(pt) = ui.ctx().pointer_hover_pos()
        && page.contains(pt)
    {
        let target = if let Some(heading) = heading_at(pt) {
            Selection::section(script, heading)
        } else {
            word_at(pt).and_then(|(block, _)| {
                inter
                    .selection
                    .filter(|s| (s.block..=s.end_block).contains(&block))
                    .or_else(|| Selection::line(script, block))
            })
        };
        if let Some(sel) = target {
            for part in sel.parts(script) {
                let ranges = format::word_ranges(src, script, part.block);
                if let (Some(a), Some(b)) = (ranges.get(part.first), ranges.get(part.last)) {
                    let rows = doc::rows_rect(&galley, origin, &idx, &(a.start..b.end));
                    p.rect_stroke(
                        rows.expand(4.0),
                        4.0,
                        Stroke::new(1.0, theme::ACCENT),
                        egui::StrokeKind::Outside,
                    );
                }
            }
            if let Some(d) = egui::DragAndDrop::payload::<MediaDrag>(ui.ctx()) {
                let label = if d.clips.len() == 1 {
                    d.clips.first().map(|f| f.name.clone()).unwrap_or_default()
                } else {
                    format!("{} clips", d.clips.len())
                };
                p.text(
                    pt + vec2(12.0, 22.0),
                    egui::Align2::LEFT_TOP,
                    format!("Link {label} to the selection"),
                    FontId::proportional(11.5),
                    theme::ACCENT,
                );
            }
            if ui.input(|i| i.pointer.any_released())
                && let Some(d) = egui::DragAndDrop::take_payload::<MediaDrag>(ui.ctx())
            {
                ev.dropped = Some((sel, d));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "## Intro\none **two**\n\nthree café 😀\n> a note\n## Next\nfour five\n";
    #[test]
    fn selections_cross_blank_lines_sections_and_unicode_in_both_directions() {
        for source in [SOURCE.to_owned(), SOURCE.replace('\n', "\r\n")] {
            let script = spearfia_script::parse(&source);
            let range = source.find("two").unwrap()..source.find("five").unwrap() + 4;
            let selection = Selection::from_source(&source, &script, range).unwrap();
            assert_eq!(
                selection.quote(&script).unwrap(),
                "two\nthree café 😀\nfour five"
            );
            assert_eq!(
                Selection::between(
                    (selection.end_block, selection.last),
                    (selection.block, selection.first)
                ),
                selection
            );
            assert_eq!(selection.parts(&script).len(), 3);
            assert!(
                nearest_word(
                    &source_words(&source, &script),
                    source.find("three").unwrap() - 1
                )
                .is_some()
            );
            assert!(
                Selection::between((usize::MAX, 0), (usize::MAX, 0))
                    .parts(&script)
                    .is_empty()
            );
        }
    }
    #[test]
    fn section_selection_skips_notes_and_stops_at_next_peer_heading() {
        let script = spearfia_script::parse(SOURCE);
        let selection = Selection::section(&script, 0).unwrap();
        assert_eq!(selection.quote(&script).unwrap(), "one two\nthree café 😀");
        assert_eq!(selection.parts(&script).len(), 2);
        assert!(Selection::section(&spearfia_script::parse("## Empty\n"), 0).is_none());
    }
    fn span(id: &str, path: &str, first: usize, last: usize) -> Span {
        Span {
            link_id: id.into(),
            media_path: path.into(),
            first,
            last,
            role: MediaRole::BRoll,
            kind: LinkKind::Picture,
            label: path.into(),
        }
    }
    #[test]
    fn overlaps_are_lighter_only_where_the_ranges_intersect() {
        let source = "one two three four\n";
        let script = spearfia_script::parse(source);
        let resolved = Resolved {
            spans: HashMap::from([(
                0,
                vec![span("k1", "a.mp4", 0, 2), span("k2", "b.mp4", 1, 3)],
            )]),
            lost: vec![],
        };
        let marks = link_marks(source, &script, &resolved, None);
        assert_eq!(marks.len(), 3);
        assert_eq!(&source[marks[1].range.clone()], "two three");
        assert!(marks[1].bg.a() < marks[0].bg.a());
        assert_eq!(marks[0].bg.a(), marks[2].bg.a());
    }
    #[test]
    fn duplicate_links_to_the_same_clip_do_not_fade_the_highlight_again() {
        let source = "one two\n";
        let script = spearfia_script::parse(source);
        let mut resolved = Resolved {
            spans: HashMap::from([(0, vec![span("k1", "a.mp4", 0, 1)])]),
            lost: vec![],
        };
        let before = link_marks(source, &script, &resolved, None);
        resolved
            .spans
            .get_mut(&0)
            .unwrap()
            .push(span("k2", "a.mp4", 0, 1));
        assert_eq!(
            link_marks(source, &script, &resolved, None)[0].bg,
            before[0].bg
        );
    }
    #[test]
    fn hover_waits_one_second_then_shows_all_clip_names() {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let source = "one two\n\nthree four\n";
        let script = spearfia_script::parse(source);
        let resolved = Resolved {
            spans: HashMap::from([(
                0,
                vec![
                    span("k1", "opening.mp4", 0, 1),
                    span("k2", "wide-shot.mp4", 0, 1),
                ],
            )]),
            lost: vec![],
        };
        let mut interaction = Interaction::default();
        let mut frame = |time, events| {
            let input = egui::RawInput {
                time: Some(time),
                events,
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1400.0, 900.0))),
                ..Default::default()
            };
            let mut event = Events::default();
            let mut output = ctx.run_ui(input, |ui| {
                event = page(
                    ui,
                    &script,
                    Options {
                        src: source,
                        resolved: &resolved,
                        interaction: Some(&mut interaction),
                        scroll_to: None,
                        selected_link: None,
                    },
                );
            });
            output.textures_delta.clear();
            (output, event)
        };
        let (initial, _) = frame(0.0, vec![]);
        let text = initial
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text.contains("three four") => {
                    Some(text)
                }
                _ => None,
            })
            .unwrap();
        let pos = doc::pos_of(&text.galley, text.pos, &doc::Index::new(source), 1).center();
        let (_, event) = frame(0.1, vec![egui::Event::PointerMoved(pos)]);
        assert!(event.hovered_links.is_empty());
        let (_, event) = frame(0.6, vec![]);
        assert!(event.hovered_links.is_empty());
        let (_, event) = frame(1.2, vec![]);
        assert_eq!(event.hovered_links, ["k1", "k2"]);
        let (output, _) = frame(1.3, vec![]);
        let text: String = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(text.contains("Linked clips"), "{text}");
        assert!(text.contains("opening.mp4") && text.contains("wide-shot.mp4"));
    }
}
