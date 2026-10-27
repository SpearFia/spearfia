//! Sync: push the approved script's links to a new FilmCraft review project (ADR 0005).
//! The push runs on a worker thread; FilmCraft runs headless, so no window opens.

use crate::theme;
use crate::widgets::{self, muted, panel, strong, text};
use egui::Margin;
use spearfia_adapter_filmcraft::{Install, Pushed};
use spearfia_core::channels::Approval;
use spearfia_core::project::Project;
use std::sync::mpsc::{Receiver, TryRecvError};

#[derive(Default)]
pub struct SyncStage {
    running: Option<Receiver<Result<Pushed, String>>>,
    last: Option<Result<Pushed, String>>,
}

impl SyncStage {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        source: &str,
        fc: Option<&Install>,
    ) {
        self.poll(ui.ctx());
        egui::CentralPanel::default()
            .frame(panel(theme::BG_APP).inner_margin(Margin::same(40)))
            .show(ui, |ui| {
                widgets::card(theme::BG_PANEL, theme::BORDER).show(ui, |ui| {
                    ui.set_max_width(720.0);
                    ui.label(strong("Push to FilmCraft", 18.0));
                    ui.add_space(6.0);
                    ui.label(text(
                        "Builds the linked media into a new FilmCraft review project: voiceover sets the \
                         timing, picture fills it, transitions from the script apply. Each push saves a \
                         new file in sync/, so FilmCraft edits are never overwritten.",
                        13.0,
                        theme::TEXT_2,
                    ));
                    ui.add_space(10.0);
                    let approval = project.script_approval(Some(source));
                    let approved = matches!(approval, Ok(Approval::Approved));
                    match &approval {
                        Ok(a) => ui.label(text(a.label(), 12.5, if approved { theme::TEXT } else { theme::WARN })),
                        Err(e) => ui.label(text(e.to_string(), 12.5, theme::DANGER)),
                    };
                    let cli = fc.and_then(|i| i.cli.clone());
                    if cli.is_none() {
                        ui.label(text(
                            format!(
                                "filmcraft-cli not found. Install FilmCraft, add it to PATH, or set {}.",
                                spearfia_adapter_filmcraft::ENV_OVERRIDE
                            ),
                            12.5,
                            theme::WARN,
                        ));
                    }
                    ui.add_space(8.0);
                    let ready = approved && self.running.is_none();
                    ui.horizontal(|ui| {
                        let clicked = ui.add_enabled_ui(ready && cli.is_some(), |ui| widgets::primary_button(ui, "Push to FilmCraft")).inner.clicked();
                        if clicked && let Some(cli) = cli {
                            self.start(project.clone(), cli, ui.ctx().clone());
                        }
                        if self.running.is_some() {
                            ui.spinner();
                            ui.label(muted("Building the review timeline…"));
                        }
                    });
                    ui.add_space(10.0);
                    match &self.last {
                        Some(Ok(p)) => result(ui, p, fc.and_then(|i| i.app.as_deref())),
                        Some(Err(e)) => {
                            ui.label(text(format!("Push failed; nothing was saved. {e}"), 12.5, theme::DANGER));
                        }
                        None => {}
                    }
                });
            });
    }

    fn start(&mut self, project: Project, cli: std::path::PathBuf, ctx: egui::Context) {
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("filmcraft-push".into())
            .spawn(move || {
                let result =
                    std::panic::catch_unwind(|| spearfia_adapter_filmcraft::push(&project, &cli))
                        .map_err(|_| "The push worker failed.".to_owned())
                        .and_then(|r| r.map_err(|e| e.to_string()));
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        match spawned {
            Ok(_) => self.running = Some(rx),
            Err(e) => self.last = Some(Err(format!("Could not start the push: {e}"))),
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.running else { return };
        match rx.try_recv() {
            Ok(r) => {
                self.running = None;
                self.last = Some(r);
            }
            Err(TryRecvError::Disconnected) => {
                self.running = None;
                self.last = Some(Err("The push worker stopped.".into()));
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
        }
    }
}

fn result(ui: &mut egui::Ui, p: &Pushed, app: Option<&std::path::Path>) {
    let s = p.summary();
    ui.label(strong(format!("Saved {}", p.review.display()), 13.0));
    ui.label(muted(format!(
        "{} clips, {} transitions",
        s["clips"], s["transitions"]
    )));
    if let Some(app) = app
        && widgets::button(ui, "Open in FilmCraft").clicked()
        && let Err(e) = std::process::Command::new(app).arg(&p.review).spawn()
    {
        ui.label(text(
            format!("Could not open FilmCraft: {e}"),
            12.5,
            theme::DANGER,
        ));
    }
    if p.issues.is_empty() {
        ui.label(muted("Every link was placed."));
        return;
    }
    ui.add_space(6.0);
    ui.label(text(
        format!("{} link(s) or cue(s) need attention:", p.issues.len()),
        12.5,
        theme::WARN,
    ));
    egui::ScrollArea::vertical()
        .max_height(260.0)
        .show(ui, |ui| {
            for issue in &p.issues {
                ui.label(muted(format!("• {}", issue.describe())));
            }
        });
}
