//! The script laid out over time, shared by the Music and Sound effects stages: the view
//! (zoom and scroll), the full-video overview strip, the ruler and the script cards.

use crate::theme;
use crate::widgets::{self, mmss};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};
use spearfia_script::timing::{Span, Timeline};
use spearfia_script::{LineKind, Segment};

/// Width of the lane names on the left of a timeline.
pub const LABEL_W: f32 = 78.0;
/// Seconds visible when the view is first fitted.
pub const FIT_SECONDS: f32 = 75.0;
/// Zoom limits, in pixels per second.
pub const MIN_PX: f32 = 1.0;
pub const MAX_PX: f32 = 400.0;

/// What part of the video a timeline shows.
#[derive(Debug, Clone, Copy, Default)]
pub struct View {
    /// Pixels per second; `None` fits the view to the width.
    pub zoom: Option<f32>,
    /// Seconds at the left edge.
    pub start: f32,
    /// Pixels per second the timeline was last drawn at, so zooming from a fitted view
    /// starts from what is on screen.
    pub shown_px: Option<f32>,
}

impl View {
    pub fn px_per_sec(&self, width: f32, total: f32) -> f32 {
        self.zoom
            .unwrap_or_else(|| width / total.clamp(10.0, FIT_SECONDS))
            .clamp(MIN_PX, MAX_PX)
    }

    /// Fit, zoom in and zoom out buttons (in a right-to-left layout).
    pub fn zoom_buttons(&mut self, ui: &mut egui::Ui) {
        if widgets::button(ui, "Fit").clicked() {
            self.zoom = None;
            self.start = 0.0;
        }
        if widgets::button(ui, "+").on_hover_text("Zoom in").clicked() {
            self.zoom_by(1.5);
        }
        if widgets::button(ui, "−").on_hover_text("Zoom out").clicked() {
            self.zoom_by(1.0 / 1.5);
        }
    }

    /// Zoom by `factor` from the current scale (the fitted one, if the view is fitted).
    pub fn zoom_by(&mut self, factor: f32) {
        let base = self.zoom.or(self.shown_px).unwrap_or(10.0);
        self.zoom = Some((base * factor).clamp(MIN_PX, MAX_PX));
    }

    /// Put `t` near the left of the view.
    pub fn show_time(&mut self, t: f32) {
        self.start = (t - 3.0).max(0.0);
    }

    /// The whole video in one strip, with sections on top and `paint` drawing into the row
    /// below (given the row and time → x). The box shows what the timeline is looking at;
    /// click or drag to move it.
    pub fn overview(
        &mut self,
        ui: &mut egui::Ui,
        tl: &Timeline,
        paint: impl FnOnce(&egui::Painter, Rect, &dyn Fn(f32) -> f32),
    ) {
        let w = ui.available_width();
        let (outer, _) = ui.allocate_exact_size(vec2(w, 74.0), Sense::hover());
        let p = ui.painter();
        p.rect(
            outer,
            CornerRadius::same(8),
            theme::BG_PANEL,
            Stroke::new(1.0, theme::BORDER),
            StrokeKind::Inside,
        );
        let inner = outer.shrink2(vec2(12.0, 10.0));
        p.text(
            inner.left_top(),
            Align2::LEFT_TOP,
            format!("FULL VIDEO  0:00 – {}", mmss(tl.total)),
            FontId::monospace(10.0),
            theme::TEXT_MUTED,
        );
        p.text(
            inner.right_top(),
            Align2::RIGHT_TOP,
            "click or drag to move the view",
            FontId::monospace(10.0),
            theme::TEXT_MUTED,
        );
        let strip =
            Rect::from_min_max(pos2(inner.left(), inner.top() + 16.0), inner.right_bottom());
        let total = tl.total.max(1.0);
        let x = |t: f32| strip.left() + strip.width() * (t / total).clamp(0.0, 1.0);

        let sec_row = Rect::from_min_size(strip.min, vec2(strip.width(), 16.0));
        for s in &tl.sections {
            let r = Rect::from_x_y_ranges(x(s.span.start)..=x(s.span.end) - 1.0, sec_row.y_range());
            p.rect_filled(r, 2.0, theme::BG_HOVER);
            let c = p.with_clip_rect(r.shrink(2.0));
            c.text(
                r.left_center() + vec2(4.0, 0.0),
                Align2::LEFT_CENTER,
                &s.title,
                FontId::monospace(10.0),
                theme::TEXT_2,
            );
        }
        let bar_row = Rect::from_min_size(
            pos2(strip.left(), sec_row.bottom() + 5.0),
            vec2(strip.width(), 9.0),
        );
        paint(p, bar_row, &x);

        // The view box.
        let px = self.px_per_sec(ui.available_width() - LABEL_W - 24.0, tl.total);
        let visible = (ui.available_width() - LABEL_W - 24.0) / px;
        let vb = Rect::from_x_y_ranges(
            x(self.start)..=x(self.start + visible),
            sec_row.top() - 3.0..=bar_row.bottom() + 3.0,
        );
        p.rect_stroke(
            vb,
            3.0,
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Outside,
        );

        let r = ui.interact(strip, ui.id().with("overview"), Sense::click_and_drag());
        if (r.clicked() || r.dragged())
            && let Some(pos) = r.interact_pointer_pos()
        {
            let t = (pos.x - strip.left()) / strip.width() * total;
            self.start = (t - visible / 2.0).clamp(0.0, (total - visible).max(0.0));
        }
        if r.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }

    /// Pan with the wheel or by dragging the background of `area`; Ctrl+wheel zooms.
    /// Returns the time ↔ x mapping and the background's response.
    pub fn pan(&mut self, ui: &mut egui::Ui, area: Rect, total: f32) -> (Geo, egui::Response) {
        let px = self.px_per_sec(area.width(), total);
        self.shown_px = Some(px);
        let visible = area.width() / px;
        let max_start = (total - visible * 0.9).max(0.0);
        let bg = ui.interact(area, ui.id().with("tl-bg"), Sense::click_and_drag());
        if bg.dragged() {
            self.start -= bg.drag_delta().x / px;
        }
        let mut px = px;
        if bg.hovered() {
            let (scroll, zoom) = ui.input_mut(|i| {
                let s = i.smooth_scroll_delta;
                i.smooth_scroll_delta = Vec2::ZERO;
                (s, i.zoom_delta())
            });
            if zoom != 1.0
                && let Some(pos) = bg.hover_pos()
            {
                let at = self.start + (pos.x - area.left()) / px;
                let new_px = (px * zoom).clamp(MIN_PX, MAX_PX);
                self.zoom = Some(new_px);
                self.start = at - (pos.x - area.left()) / new_px;
                px = new_px;
            } else {
                self.start -= (scroll.x + scroll.y) / px;
            }
        }
        self.start = self.start.clamp(0.0, max_start.max(0.0));
        (
            Geo {
                x0: area.left(),
                px,
                start: self.start,
            },
            bg,
        )
    }
}

/// Time ↔ x for the visible part of a timeline.
pub struct Geo {
    pub x0: f32,
    pub px: f32,
    pub start: f32,
}

impl Geo {
    pub fn x(&self, t: f32) -> f32 {
        self.x0 + (t - self.start) * self.px
    }

    pub fn t(&self, x: f32) -> f32 {
        self.start + (x - self.x0) / self.px
    }

    /// The span's rectangle in a lane, with `gap` pixels trimmed off the right edge.
    pub fn rect(&self, s: Span, y: egui::Rangef, gap: f32) -> Rect {
        let l = self.x(s.start);
        let r = (self.x(s.end) - gap).max(l);
        Rect::from_x_y_ranges(l..=r, y)
    }
}

pub fn ruler_ticks(p: &egui::Painter, ruler: Rect, geo: &Geo, total: f32) {
    let step = [1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0]
        .into_iter()
        .find(|s| s * geo.px >= 70.0)
        .unwrap_or(600.0);
    let first = (geo.start / step).floor() * step;
    let mut t = first;
    while geo.x(t) <= ruler.right() && t <= total + step {
        let x = geo.x(t);
        if x >= ruler.left() - 1.0 {
            p.line_segment(
                [pos2(x, ruler.bottom() - 6.0), pos2(x, ruler.bottom())],
                Stroke::new(1.0, theme::BORDER_STRONG),
            );
            p.text(
                pos2(x + 3.0, ruler.top() + 3.0),
                Align2::LEFT_TOP,
                mmss(t),
                FontId::monospace(10.0),
                theme::TEXT_MUTED,
            );
        }
        t += step;
    }
    p.line_segment(
        [ruler.left_bottom(), ruler.right_bottom()],
        Stroke::new(1.0, theme::BORDER),
    );
}

/// The card's frame and VO/LIVE tag; returns the y where text starts.
pub fn card_frame(p: &egui::Painter, r: Rect, kind: LineKind) -> f32 {
    let (fill, stroke) = match kind {
        LineKind::Spoken => (theme::BG_RAISED, theme::tint(theme::CUE_VO, 0.35)),
        LineKind::Live => (theme::BG_RAISED, theme::tint(theme::CUE_LIVE, 0.35)),
        LineKind::OnScreen | LineKind::Note => (theme::BG_INSET, theme::BORDER_STRONG),
    };
    p.rect(
        r,
        CornerRadius::same(4),
        fill,
        Stroke::new(1.0, stroke),
        StrokeKind::Inside,
    );
    let mut y = r.top() + 6.0;
    if kind.is_spoken() && r.width() >= 22.0 {
        let (tag, color) = if kind == LineKind::Live {
            ("LIVE", theme::CUE_LIVE)
        } else {
            ("VO", theme::CUE_VO)
        };
        p.with_clip_rect(r.shrink(3.0).intersect(p.clip_rect()))
            .text(
                pos2(r.left() + 6.0, y),
                Align2::LEFT_TOP,
                tag,
                FontId::monospace(9.5),
                color,
            );
        y += 14.0;
    }
    y
}

/// A script line as a card: VO/LIVE tag, then the words wrapped inside.
pub fn script_card(p: &egui::Painter, r: Rect, kind: LineKind, segments: &[Segment]) {
    let y = card_frame(p, r, kind);
    if r.width() < 54.0 {
        return;
    }
    let c = p.with_clip_rect(r.shrink(3.0).intersect(p.clip_rect()));
    let mut job = LayoutJob::default();
    job.wrap.max_width = (r.width() - 12.0).max(10.0);
    job.append(
        &plain_text(segments),
        0.0,
        TextFormat {
            font_id: FontId::proportional(11.5),
            color: if kind.is_spoken() {
                theme::TEXT
            } else {
                theme::TEXT_2
            },
            italics: kind == LineKind::OnScreen,
            ..Default::default()
        },
    );
    let g = c.layout_job(job);
    c.galley(pos2(r.left() + 6.0, y), g, theme::TEXT);
}

pub fn plain_text(segments: &[Segment]) -> String {
    segments
        .iter()
        .filter_map(|s| match s {
            Segment::Text(t) => Some(t.as_str()),
            Segment::Cue(_) => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
        .replace("**", "")
        .replace("++", "")
        .replace("~~", "")
}

pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

/// A cue's id, or `@<byte offset>` for one without.
pub fn cue_key(d: &spearfia_script::Directive) -> String {
    d.id.clone().unwrap_or_else(|| format!("@{}", d.span.start))
}

/// Lane names down the left of a timeline.
pub fn lane_labels(p: &egui::Painter, x: f32, lanes: &[(Rect, &str)]) {
    for (r, label) in lanes {
        p.text(
            pos2(x, r.center().y),
            Align2::LEFT_CENTER,
            *label,
            FontId::monospace(10.5),
            theme::TEXT_MUTED,
        );
    }
}

/// A row of swatches and names under a timeline, with a hint on the right.
pub fn legend(p: &egui::Painter, r: Rect, items: &[(egui::Color32, &str)], hint: &str) {
    let mut lx = r.left();
    for (color, label) in items {
        let sw = Rect::from_min_size(pos2(lx, r.center().y - 4.0), vec2(14.0, 8.0));
        p.rect_filled(sw, 2.0, *color);
        let t = p.text(
            pos2(sw.right() + 6.0, r.center().y),
            Align2::LEFT_CENTER,
            *label,
            FontId::monospace(10.5),
            theme::TEXT_MUTED,
        );
        lx = t.right() + 18.0;
    }
    p.text(
        r.right_center(),
        Align2::RIGHT_CENTER,
        hint,
        FontId::monospace(10.0),
        theme::TEXT_MUTED,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_starts_from_the_fitted_scale() {
        let mut v = View::default();
        // A fitted 23 s project, 850 px wide: about 37 px/s.
        v.shown_px = Some(v.px_per_sec(850.0, 23.0));
        let fitted = v.shown_px.unwrap();
        v.zoom_by(1.5);
        assert!((v.zoom.unwrap() - fitted * 1.5).abs() < 1e-3);
        let mut v = View {
            shown_px: Some(fitted),
            ..View::default()
        };
        v.zoom_by(1.0 / 1.5);
        assert!(v.zoom.unwrap() < fitted);
        let mut v = View {
            zoom: Some(MAX_PX),
            ..View::default()
        };
        v.zoom_by(1.5);
        assert_eq!(v.zoom, Some(MAX_PX), "stored zoom is clamped too");
    }
}
