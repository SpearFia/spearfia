//! `spearfia-desktop`: the Spear Fia desktop app. Native Rust UI with egui, the same
//! toolkit FilmCraft uses. Project changes go through `spearfia-core`.

// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod doc;
mod fonts;
mod icons;
mod screens;
mod script_view;
mod theme;
mod widgets;

use std::path::Path;

const APP_NAME: &str = "Spear Fia";

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1100.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

/// Open a folder in the system file manager.
pub fn reveal(dir: &Path) {
    let cmd = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(dir).spawn();
}
