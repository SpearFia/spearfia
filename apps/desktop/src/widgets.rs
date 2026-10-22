//! Small shared widgets: chips, buttons, segmented controls, key/value rows.

use crate::theme::{self, tint};
use egui::{
    Color32, CornerRadius, FontId, Frame, Margin, Response, RichText, Sense, Stroke, StrokeKind,
    Ui, Vec2,
};

pub fn text(s: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(s).size(size).color(color)
}

pub fn strong(s: impl Into<String>, size: f32) -> RichText {
    RichText::new(s).size(size).color(theme::TEXT).strong()
}

pub fn muted(s: impl Into<String>) -> RichText {
    RichText::new(s).size(12.0).color(theme::TEXT_MUTED)
}

pub fn mono(s: impl Into<String>, color: Color32) -> RichText {
    RichText::new(s).monospace().size(11.0).color(color)
}

/// A panel frame with a fill and no margin.
pub fn panel(fill: Color32) -> Frame {
    Frame::new().fill(fill)
}

/// A rounded, bordered box.
pub fn card(fill: Color32, stroke: Color32) -> Frame {
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
}

/// Draw a tinted pill of a fixed size, so it wraps like a word in wrapping layouts.
fn pill(
    ui: &mut Ui,
    label: &str,
    font: FontId,
    color: Color32,
    dot: bool,
    pad: Vec2,
    radius: u8,
) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
    let dot_w = if dot { 12.0 } else { 0.0 };
    let size = galley.size() + pad * 2.0 + egui::vec2(dot_w, 0.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect(
            rect,
            CornerRadius::same(radius),
            tint(color, 0.13),
            Stroke::new(1.0, tint(color, 0.45)),
            StrokeKind::Inside,
        );
        if dot {
            p.circle_filled(
                rect.left_center() + egui::vec2(pad.x + 3.0, 0.0),
                3.0,
                color,
            );
        }
        p.galley(rect.min + pad + egui::vec2(dot_w, 0.0), galley, color);
    }
    response
}

/// A tinted tag: colored dot + label. Used for cue chips, roles and statuses.
pub fn chip(ui: &mut Ui, label: &str, color: Color32) -> Response {
    pill(
        ui,
        label,
        FontId::proportional(12.0),
        color,
        true,
        egui::vec2(7.0, 3.0),
        5,
    )
}

/// A small monospace tag without a dot (license, kind, counts).
pub fn tag(ui: &mut Ui, label: &str, color: Color32) -> Response {
    pill(
        ui,
        label,
        FontId::monospace(10.5),
        color,
        false,
        egui::vec2(5.0, 2.0),
        3,
    )
}

/// The color for a directive name (`sfx`, `music`, `transition`, ...).
pub fn cue_color(name: &str) -> Color32 {
    match name {
        "sfx" => theme::CUE_SFX,
        "music" => theme::CUE_MUSIC,
        "transition" => theme::CUE_TRANSITION,
        "clip" => theme::CUE_CLIP,
        "look" => theme::CUE_LOOK,
        "vision" => theme::CUE_VISION,
        _ => theme::TEXT_2,
    }
}

pub fn primary_button(ui: &mut Ui, label: &str) -> Response {
    ui.add(
        egui::Button::new(text(label, 12.5, theme::ACCENT_ON).strong())
            .fill(theme::ACCENT)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(6)),
    )
}

pub fn button(ui: &mut Ui, label: &str) -> Response {
    ui.add(
        egui::Button::new(text(label, 12.5, theme::TEXT))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
            .corner_radius(CornerRadius::same(6)),
    )
}

/// Pick one of `options`, styled like the segmented controls in the mockups.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    Frame::new()
        .fill(theme::BG_INSET)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (v, label) in options {
                    let on = *value == *v;
                    let b = egui::Button::new(text(
                        *label,
                        12.0,
                        if on { theme::ACCENT } else { theme::TEXT_2 },
                    ))
                    .fill(if on {
                        tint(theme::ACCENT, 0.16)
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(if on {
                        Stroke::new(1.0, tint(theme::ACCENT, 0.6))
                    } else {
                        Stroke::NONE
                    })
                    .corner_radius(CornerRadius::same(4));
                    if ui.add(b).clicked() && !on {
                        *value = *v;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// A tab in a title bar: text with an accent underline when active.
pub fn tab(ui: &mut Ui, label: &str, active: bool) -> Response {
    let color = if active {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };
    let r = ui.add(
        egui::Button::new(text(label, 13.0, color))
            .frame(false)
            .min_size(egui::vec2(0.0, 40.0)),
    );
    if active {
        let rect = r.rect;
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(2.0, theme::ACCENT),
        );
    }
    r
}

/// `label    value` on one line, label in muted monospace.
/// A `DragValue` for a value that lives in the script. The script is re-read every frame,
/// so the value being dragged is kept here until the drag ends. Returns the value to write:
/// on release after a drag, or when it was typed or changed with the keyboard. `None` while
/// dragging or when nothing changed.
pub fn drag_value<T>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    value: T,
    build: impl FnOnce(&mut T) -> egui::DragValue<'_>,
) -> Option<T>
where
    T: egui::emath::Numeric + Send + Sync + 'static,
{
    let key = ui.make_persistent_id(id_salt).with("drag-pending");
    let mut v = ui.data(|d| d.get_temp::<T>(key)).unwrap_or(value);
    let r = ui.add(build(&mut v));
    if r.drag_stopped() {
        ui.data_mut(|d| d.remove::<T>(key));
        return (v != value).then_some(v);
    }
    if r.dragged() {
        ui.data_mut(|d| d.insert_temp(key, v));
        return None;
    }
    ui.data_mut(|d| d.remove::<T>(key));
    (r.changed() && v != value).then_some(v)
}

pub fn kv(ui: &mut Ui, key: &str, value: impl Into<RichText>) {
    ui.horizontal_top(|ui| {
        ui.add_sized([92.0, 16.0], egui::Label::new(mono(key, theme::TEXT_MUTED)));
        ui.add(egui::Label::new(value.into()).wrap());
    });
}

/// A 1px horizontal rule across the available width.
pub fn rule(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, theme::BORDER);
}

/// "just now", "5m ago", "3h ago", "2d ago".
pub fn ago(then: u64, now: u64) -> String {
    let s = now.saturating_sub(then);
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", s / 60),
        3600..86_400 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86_400),
    }
}

/// `125.0` seconds → `2:05`.
pub fn mmss(seconds: f32) -> String {
    let s = seconds.max(0.0).round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Like `Ui::dnd_drag_source`, but the drag area sits under the contents, so buttons inside
/// the card still get clicks. The card's rect is remembered from the last frame.
pub fn drag_card<P: std::any::Any + Send + Sync, R>(
    ui: &mut Ui,
    id: egui::Id,
    payload: P,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let rect_id = id.with("drag-card-rect");
    if ui.ctx().is_being_dragged(id) {
        egui::DragAndDrop::set_payload(ui.ctx(), payload);
        let layer = egui::LayerId::new(egui::Order::Tooltip, id);
        let inner = ui.scope_builder(egui::UiBuilder::new().layer_id(layer), add_contents);
        if let Some(pt) = ui.ctx().pointer_interact_pos() {
            let delta = pt - inner.response.rect.center();
            ui.ctx()
                .transform_layer_shapes(layer, egui::emath::TSTransform::from_translation(delta));
        }
        // Keep the space it took.
        if let Some(r) = ui.data(|d| d.get_temp::<egui::Rect>(rect_id)) {
            ui.interact(r, id, Sense::drag());
        }
        return inner;
    }
    let drag = ui
        .data(|d| d.get_temp::<egui::Rect>(rect_id))
        .map(|r| ui.interact(r, id, Sense::drag()));
    let inner = ui.scope(add_contents);
    ui.data_mut(|d| d.insert_temp(rect_id, inner.response.rect));
    if let Some(d) = drag {
        d.on_hover_cursor(egui::CursorIcon::Grab);
    }
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_time() {
        assert_eq!(mmss(125.4), "2:05");
        assert_eq!(mmss(0.0), "0:00");
        assert_eq!(ago(100, 100), "just now");
        assert_eq!(ago(0, 7200), "2h ago");
        assert_eq!(ago(0, 3 * 86_400), "3d ago");
    }
}
