//! `spearfia-desktop`: the Spear Fia desktop app. Native Rust UI with egui, the same
//! toolkit FilmCraft uses. All user actions will go through `spearfia-core`'s command layer.

// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

const APP_NAME: &str = "Spear Fia";
const TAGLINE: &str = "The start of something great";

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([960.0, 600.0])
            .with_min_inner_size([480.0, 320.0]),
        ..Default::default()
    };
    eframe::run_native(APP_NAME, options, Box::new(|_cc| Ok(Box::new(SpearFiaApp))))
}

struct SpearFiaApp;

impl eframe::App for SpearFiaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                ui.heading(egui::RichText::new(TAGLINE).size(40.0).strong());
            });
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tagline_is_set() {
        assert_eq!(super::TAGLINE, "The start of something great");
    }
}
