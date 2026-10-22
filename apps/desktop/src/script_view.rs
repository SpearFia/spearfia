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
    pub path: String,
    pub name: String,
    pub role: MediaRole,
}

pub fn role_color(role: MediaRole) -> egui::Color32 {
    match role {
        MediaRole::ARoll => theme::CUE_LIVE,
        MediaRole::Voiceover => theme::CUE_VO,
        MediaRole::BRoll => theme::CUE_CLIP,
        MediaRole::Screen => theme::CUE_LOOK,
        MediaRole::Graphics => theme::CUE_VISION,
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

/// Highlights for links (and the selected link, stronger).
pub fn link_marks(
    src: &str,
    script: &Script,
    resolved: &Resolved,
    selected: Option<&str>,
) -> Vec<Mark> {
    let mut out = Vec::new();
    for (block, spans) in &resolved.spans {
        let words = format::word_ranges(src, script, *block);
        for s in spans {
            let (Some(a), Some(b)) = (words.get(s.first), words.get(s.last)) else {
                continue;
            };
            let strong = selected == Some(s.link_id.as_str());
            let c = role_color(s.role);
            out.push(match s.kind {
                LinkKind::Picture => Mark::bg(
                    a.start..b.end,
                    theme::tint(c, if strong { 0.42 } else { 0.22 }),
                ),
                LinkKind::Audio => Mark {
                    range: a.start..b.end,
                    bg: if strong {
                        theme::tint(c, 0.2)
                    } else {
                        egui::Color32::TRANSPARENT
                    },
                    underline: Some(c),
                    color: None,
                },
            });
        }
    }
    out
}

/// A word range on one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub block: usize,
    pub first: usize,
    pub last: usize,
}

/// Selection state the Media stage keeps between frames.
#[derive(Debug, Default)]
pub struct Interaction {
    /// The held selection: stays until Esc, a click elsewhere, or a link is made.
    pub selection: Option<Selection>,
    /// Where the current selection drag started: (block, word).
    anchor: Option<(usize, usize)>,
}

/// What happened this frame.
#[derive(Debug, Default)]
pub struct Events {
    /// Media dropped on a word range.
    pub dropped: Option<(Selection, Arc<MediaDrag>)>,
    /// A link tag or linked word was clicked.
    pub clicked_link: Option<String>,
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
        let words = format::word_ranges(src, script, sel.block);
        if let (Some(a), Some(b)) = (words.get(sel.first), words.get(sel.last)) {
            marks.push(Mark::bg(a.start..b.end, theme::tint(theme::ACCENT, 0.32)));
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

    // Which block and word is at a point.
    let word_at = |pt: Pos2| -> Option<(usize, usize)> {
        let b = doc::byte_at(&galley, origin, &idx, pt);
        let n = format::line_of(src, b);
        let block = lines.get(n)?.block?;
        if !matches!(script.blocks.get(block), Some(Block::Line { .. })) {
            return None;
        }
        let words = format::word_ranges(src, script, block);
        if words.is_empty() {
            return None;
        }
        let w = words.iter().rposition(|r| r.start <= b).unwrap_or(0);
        Some((block, w))
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
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
    if (resp.drag_started() || resp.clicked())
        && let Some(pt) = pointer
    {
        match word_at(pt) {
            Some((block, w)) => {
                inter.anchor = Some((block, w));
                inter.selection = Some(Selection {
                    block,
                    first: w,
                    last: w,
                });
                if resp.clicked()
                    && let Some(s) = opts
                        .resolved
                        .spans
                        .get(&block)
                        .and_then(|v| v.iter().find(|s| (s.first..=s.last).contains(&w)))
                {
                    ev.clicked_link = Some(s.link_id.clone());
                }
            }
            None => inter.selection = None,
        }
    } else if resp.dragged()
        && let (Some((ab, aw)), Some(pt)) = (inter.anchor, pointer)
        && let Some((block, w)) = word_at(pt)
        && block == ab
    {
        inter.selection = Some(Selection {
            block,
            first: aw.min(w),
            last: aw.max(w),
        });
    }

    // Dropping media: onto the held selection if the pointer is on its line, else the line.
    let dragging = egui::DragAndDrop::has_payload_of_type::<MediaDrag>(ui.ctx());
    let hover = ui.ctx().pointer_hover_pos();
    if dragging
        && let Some(pt) = hover
        && page.contains(pt)
        && let Some((block, _)) = word_at(pt)
    {
        let n_words = format::word_ranges(src, script, block).len();
        let sel = inter
            .selection
            .filter(|s| s.block == block)
            .unwrap_or(Selection {
                block,
                first: 0,
                last: n_words.saturating_sub(1),
            });
        if let Some(Block::Line { line, .. }) = script.blocks.get(block)
            && let Some(l) = lines.get(*line)
        {
            let rows = doc::rows_rect(&galley, origin, &idx, &l.range);
            let rows = Rect::from_min_max(
                pos2(origin.x - 8.0, rows.top()),
                pos2(origin.x + look.width + 8.0, rows.bottom()),
            );
            p.rect_stroke(
                rows,
                4.0,
                Stroke::new(1.0, theme::ACCENT),
                egui::StrokeKind::Outside,
            );
            if let Some(d) = egui::DragAndDrop::payload::<MediaDrag>(ui.ctx()) {
                let what = if sel.first == 0 && sel.last + 1 >= n_words {
                    "this line"
                } else {
                    "the highlight"
                };
                p.text(
                    rows.right_top() + vec2(0.0, -4.0),
                    egui::Align2::RIGHT_BOTTOM,
                    format!("link {} to {what}", d.name),
                    FontId::proportional(11.5),
                    theme::ACCENT,
                );
            }
        }
        if ui.input(|i| i.pointer.any_released())
            && let Some(d) = egui::DragAndDrop::take_payload::<MediaDrag>(ui.ctx())
        {
            ev.dropped = Some((sel, d));
        }
    }
}
