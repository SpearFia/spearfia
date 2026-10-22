//! The screen the app opens on: a title bar with Projects / Catalog tabs.

use crate::app::{App, HomeTab};
use crate::screens::{catalog, projects};
use crate::theme;
use crate::widgets::{self, panel, text};
use egui::{Align, Layout, Margin, Panel};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    Panel::top("home-title")
        .exact_size(44.0)
        .show_separator_line(false)
        .frame(panel(theme::BG_PANEL).inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                if widgets::tab(ui, "Projects", app.home_tab == HomeTab::Projects).clicked() {
                    app.home_tab = HomeTab::Projects;
                }
                ui.add_space(8.0);
                if widgets::tab(ui, "Catalog", app.home_tab == HomeTab::Catalog).clicked() {
                    app.home_tab = HomeTab::Catalog;
                }
                ui.add_space(24.0);
                brand(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    filmcraft_status(app, ui);
                });
            });
        });
    error_banner(app, ui);
    match app.home_tab {
        HomeTab::Projects => projects::show(app, ui),
        HomeTab::Catalog => catalog::show(app, ui),
    }
}

pub fn brand(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    let p = ui.painter();
    p.line_segment(
        [r.left_bottom(), r.right_top() + egui::vec2(-4.0, 4.0)],
        egui::Stroke::new(2.0, theme::ACCENT),
    );
    p.add(egui::Shape::convex_polygon(
        vec![
            r.right_top() + egui::vec2(-7.0, 0.0),
            r.right_top(),
            r.right_top() + egui::vec2(0.0, 7.0),
        ],
        theme::ACCENT,
        egui::Stroke::NONE,
    ));
    ui.label(widgets::strong("Spear Fia", 14.0));
}

pub fn filmcraft_status(app: &App, ui: &mut egui::Ui) {
    let (color, label, tip) = match &app.filmcraft {
        Some(i) => (
            theme::OK,
            "FilmCraft found",
            i.folder()
                .map(|f| f.display().to_string())
                .unwrap_or_default()
                .to_string(),
        ),
        None => (
            theme::WARN,
            "FilmCraft not found",
            format!(
                "Install FilmCraft, add it to PATH, or set {} to its folder.",
                spearfia_adapter_filmcraft::ENV_OVERRIDE
            ),
        ),
    };
    widgets::chip(ui, label, color).on_hover_text(tip);
}

fn error_banner(app: &mut App, ui: &mut egui::Ui) {
    let Some(msg) = app.error.clone() else { return };
    Panel::top("home-error")
        .show_separator_line(false)
        .frame(panel(theme::tint(theme::DANGER, 0.15)).inner_margin(Margin::symmetric(16, 8)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(text(msg, 12.5, theme::DANGER));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Dismiss").clicked() {
                        app.error = None;
                    }
                });
            });
        });
}
