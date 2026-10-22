//! Colors and style. The tokens match the "Spear Fia / Color" variables in the Figma file.

use egui::{Color32, CornerRadius, Stroke, Visuals};

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const BG_APP: Color32 = hex(0x0D0E10);
pub const BG_PANEL: Color32 = hex(0x141619);
pub const BG_RAISED: Color32 = hex(0x1B1E22);
pub const BG_HOVER: Color32 = hex(0x23272C);
pub const BG_INSET: Color32 = hex(0x101214);
pub const BORDER: Color32 = hex(0x272B31);
pub const BORDER_STRONG: Color32 = hex(0x383D45);
pub const TEXT: Color32 = hex(0xE9EBEE);
pub const TEXT_2: Color32 = hex(0xA2A8B0);
pub const TEXT_MUTED: Color32 = hex(0x68707A);
pub const ACCENT: Color32 = hex(0xF0A43A);
pub const ACCENT_ON: Color32 = hex(0x1A1206);
pub const OK: Color32 = hex(0x4CC38A);
pub const WARN: Color32 = hex(0xE5B454);
pub const DANGER: Color32 = hex(0xE5675F);

pub const CUE_SFX: Color32 = hex(0xE8846A);
pub const CUE_MUSIC: Color32 = hex(0xA88BFA);
pub const CUE_TRANSITION: Color32 = hex(0x5CC8E0);
pub const CUE_CLIP: Color32 = hex(0x6CC487);
pub const CUE_LOOK: Color32 = hex(0xF0A43A);
pub const CUE_VISION: Color32 = hex(0xE58BC0);
pub const CUE_VO: Color32 = hex(0x8FB3FF);
pub const CUE_LIVE: Color32 = hex(0xF07178);

/// `color` at `alpha` (0..=1), for tinted chip backgrounds and borders.
pub fn tint(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (alpha * 255.0) as u8)
}

/// The document's default font faces (IBM Plex Sans, SIL OFL 1.1; see `assets/fonts`).
/// Each face is its own egui family; see [`crate::fonts`].
pub const DOC: &str = "font:IBM Plex Sans";
pub const DOC_ITALIC: &str = "font:IBM Plex Sans:italic";
pub const DOC_SEMIBOLD: &str = "font:IBM Plex Sans:semibold";
pub const DOC_BOLD: &str = "font:IBM Plex Sans:bold";

pub fn apply(ctx: &egui::Context) {
    crate::fonts::init(ctx);
    let mut v = Visuals::dark();
    v.panel_fill = BG_APP;
    v.window_fill = BG_RAISED;
    v.window_stroke = Stroke::new(1.0, BORDER_STRONG);
    v.extreme_bg_color = BG_INSET;
    v.faint_bg_color = BG_PANEL;
    v.code_bg_color = BG_INSET;
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARN;
    v.error_fg_color = DANGER;
    v.selection.bg_fill = tint(ACCENT, 0.35);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);

    let w = &mut v.widgets;
    for (state, fill) in [
        (&mut w.noninteractive, BG_PANEL),
        (&mut w.inactive, BG_RAISED),
        (&mut w.hovered, BG_HOVER),
        (&mut w.active, BG_HOVER),
        (&mut w.open, BG_HOVER),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.corner_radius = CornerRadius::same(6);
    }
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_2);
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.bg_stroke = Stroke::new(1.0, TEXT_MUTED);
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, TEXT);

    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.interact_size.y = 28.0;
        // App UI, not a document: labels shouldn't grab clicks for text selection.
        s.interaction.selectable_labels = false;
    });
}
