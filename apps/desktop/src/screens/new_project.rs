//! New Project: basics, vision, script start, media folders, FilmCraft. Shows what will
//! be created, then calls `spearfia_core::project::create_project`.

use crate::app::{App, View};
use crate::theme;
use crate::widgets::{self, card, mono, muted, panel, strong, text};
use egui::{Align, Layout, Margin, Panel};
use spearfia_core::project::{
    self, FilmCraftConnection, FilmCraftLink, Format, MediaFolder, NewProject, NewVision,
    ScriptStart, Timebase,
};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptMode {
    Sections,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Conn {
    Bridge,
    Headless,
}

pub struct NewProjectForm {
    name: String,
    series: String,
    /// Folder new projects go in; the project folder is `<parent>/<slug>`.
    parent: String,
    /// The project folder, once the user edits it by hand.
    root_override: Option<String>,
    format: Format,
    resolution: [u32; 2],
    timebase: Timebase,
    runtime: String,
    preset: Option<&'static str>,
    goal: String,
    avg_shot: f32,
    cut_style: &'static str,
    script_mode: ScriptMode,
    import_path: String,
    folders: Vec<(MediaFolder, bool)>,
    filmcraft: bool,
    sequence: String,
    conn: Conn,
    port: u16,
    error: Option<String>,
}

impl NewProjectForm {
    pub fn new(projects_dir: Option<PathBuf>, series: Option<String>) -> Self {
        let preset = spearfia_vision::builtin_presets().first();
        Self {
            name: String::new(),
            series: series.unwrap_or_default(),
            parent: projects_dir
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            root_override: None,
            format: Format::LongForm,
            resolution: Format::LongForm.default_resolution(),
            timebase: Timebase::FPS_59_94,
            runtime: "10:00".into(),
            preset: preset.map(|p| p.name),
            goal: String::new(),
            avg_shot: preset.map_or(2.0, |p| p.avg_shot_seconds),
            cut_style: preset.map_or("jump", |p| p.cut_style),
            script_mode: ScriptMode::Sections,
            import_path: String::new(),
            folders: MediaFolder::defaults()
                .into_iter()
                .map(|f| (f, true))
                .collect(),
            filmcraft: true,
            sequence: "Part 1".into(),
            conn: Conn::Bridge,
            port: 9876,
            error: None,
        }
    }

    fn slug(&self) -> String {
        let s = project::slug(&self.name);
        if s.is_empty() {
            "new-project".into()
        } else {
            s
        }
    }

    fn root(&self) -> PathBuf {
        match &self.root_override {
            Some(r) => PathBuf::from(r),
            None if self.parent.trim().is_empty() => PathBuf::new(),
            None => PathBuf::from(self.parent.trim()).join(self.slug()),
        }
    }

    fn fcproj_name(&self) -> String {
        format!("{}.fcproj", self.slug())
    }

    fn runtime_seconds(&self) -> Option<u32> {
        let t = self.runtime.trim();
        match t.split_once(':') {
            Some((m, s)) => {
                Some(m.trim().parse::<u32>().ok()? * 60 + s.trim().parse::<u32>().ok()?)
            }
            None => t.parse::<u32>().ok().map(|m| m * 60),
        }
    }

    fn to_spec(&self) -> Result<NewProject, String> {
        if self.name.trim().is_empty() {
            return Err("Give the project a name.".into());
        }
        let root = self.root();
        if root.as_os_str().is_empty() {
            return Err("Choose where the project folder goes.".into());
        }
        let script = match self.script_mode {
            ScriptMode::Sections => ScriptStart::Sections,
            ScriptMode::Import if self.import_path.trim().is_empty() => {
                return Err("Choose the script file to import.".into());
            }
            ScriptMode::Import => ScriptStart::Import(PathBuf::from(self.import_path.trim())),
        };
        Ok(NewProject {
            name: self.name.trim().to_owned(),
            series: Some(self.series.clone()),
            root,
            format: self.format,
            resolution: self.resolution,
            timebase: self.timebase,
            target_runtime_seconds: self.runtime_seconds(),
            vision: NewVision {
                extends: self.preset.map(Into::into),
                goal: self.goal.clone(),
                avg_shot_seconds: Some(self.avg_shot),
                cut_style: Some(self.cut_style.into()),
            },
            script,
            media_folders: self
                .folders
                .iter()
                .filter(|(_, on)| *on)
                .map(|(f, _)| f.clone())
                .collect(),
            filmcraft: self.filmcraft.then(|| FilmCraftLink {
                project_file: self.fcproj_name(),
                sequence: self.sequence.clone(),
                connection: match self.conn {
                    Conn::Bridge => FilmCraftConnection::Bridge { port: self.port },
                    Conn::Headless => FilmCraftConnection::Headless,
                },
                created: false,
            }),
        })
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let View::NewProject(form) = &mut app.view else {
        return;
    };
    let mut create = false;
    let mut cancel = false;

    Panel::top("np-header")
        .exact_size(64.0)
        .frame(panel(theme::BG_PANEL).inner_margin(Margin::symmetric(28, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.vertical(|ui| {
                    ui.add_space(12.0);
                    ui.label(strong("New project", 17.0));
                    ui.label(muted("You can change all of this later."));
                });
            });
        });
    Panel::bottom("np-footer")
        .exact_size(56.0)
        .frame(panel(theme::BG_PANEL).inner_margin(Margin::symmetric(28, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                if let Some(e) = &form.error {
                    ui.label(text(e, 12.5, theme::DANGER));
                } else {
                    ui.label(muted("Opens on the Script page when done."));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if form.filmcraft {
                        "Create project + FilmCraft project"
                    } else {
                        "Create project"
                    };
                    create = widgets::primary_button(ui, label).clicked();
                    cancel = widgets::button(ui, "Cancel").clicked();
                });
            });
        });
    Panel::right("np-preview")
        .exact_size(340.0)
        .frame(panel(theme::BG_APP).inner_margin(Margin::same(20)))
        .show(ui, |ui| preview(form, ui));
    egui::CentralPanel::default()
        .frame(panel(theme::BG_APP).inner_margin(Margin::symmetric(28, 20)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.columns(2, |cols| {
                        basics(form, &mut cols[0]);
                        cols[0].add_space(14.0);
                        vision(form, &mut cols[0]);
                        cols[0].add_space(14.0);
                        script(form, &mut cols[0]);
                        media(form, &mut cols[1]);
                        cols[1].add_space(14.0);
                        filmcraft(form, &mut cols[1], app.filmcraft.is_some());
                    });
                });
        });

    if cancel {
        app.go_home();
        return;
    }
    if create {
        let spec = match form.to_spec() {
            Ok(s) => s,
            Err(e) => {
                form.error = Some(e);
                return;
            }
        };
        match project::create_project(&spec) {
            Ok(p) => {
                if let Some(parent) = p.root.parent() {
                    app.settings.projects_dir = Some(parent.to_path_buf());
                }
                app.enter_workspace(p);
            }
            Err(e) => form.error = Some(e.to_string()),
        }
    }
}

fn section(ui: &mut egui::Ui, n: u8, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    card(theme::BG_PANEL, theme::BORDER).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::tag(ui, &n.to_string(), theme::TEXT);
            ui.label(strong(title, 13.5));
        });
        ui.add_space(8.0);
        add(ui);
    });
}

fn field_label(ui: &mut egui::Ui, s: &str) {
    ui.label(text(s, 11.5, theme::TEXT_2));
}

fn basics(f: &mut NewProjectForm, ui: &mut egui::Ui) {
    section(ui, 1, "Basics", |ui| {
        field_label(ui, "Project name");
        if ui
            .add(
                egui::TextEdit::singleline(&mut f.name)
                    .hint_text("Ep 2 · First Contact")
                    .desired_width(f32::INFINITY),
            )
            .changed()
        {
            f.error = None;
        }
        field_label(ui, "Series (optional)");
        ui.add(
            egui::TextEdit::singleline(&mut f.series)
                .hint_text("Learning to Drift")
                .desired_width(f32::INFINITY),
        );
        field_label(ui, "Format");
        let before = f.format;
        widgets::segmented(ui, &mut f.format, &Format::ALL.map(|x| (x, x.label())));
        if f.format != before {
            f.resolution = f.format.default_resolution();
        }
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                field_label(ui, "Resolution");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut f.resolution[0]).range(16..=8192));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut f.resolution[1]).range(16..=8192));
                });
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                field_label(ui, "Frame rate");
                egui::ComboBox::from_id_salt("np-fps")
                    .selected_text(f.timebase.label())
                    .show_ui(ui, |ui| {
                        for t in Timebase::COMMON {
                            ui.selectable_value(&mut f.timebase, t, t.label());
                        }
                    });
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                field_label(ui, "Target runtime");
                ui.add(egui::TextEdit::singleline(&mut f.runtime).desired_width(70.0));
            });
        });
    });
}

fn vision(f: &mut NewProjectForm, ui: &mut egui::Ui) {
    section(ui, 2, "Vision", |ui| {
        ui.label(muted(
            "Steers the AI's script notes, catalog ranking and pacing. Pick a preset, then adjust.",
        ));
        ui.add_space(4.0);
        for p in spearfia_vision::builtin_presets() {
            let on = f.preset == Some(p.name);
            let r = card(
                if on {
                    theme::tint(theme::CUE_VISION, 0.08)
                } else {
                    theme::BG_RAISED
                },
                if on { theme::CUE_VISION } else { theme::BORDER },
            )
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    text(
                        p.title,
                        12.5,
                        if on { theme::CUE_VISION } else { theme::TEXT },
                    )
                    .strong(),
                );
                ui.label(muted(p.goal));
                ui.horizontal_wrapped(|ui| {
                    for (k, w) in p.weights {
                        widgets::tag(
                            ui,
                            &format!("{k} {w:+.1}"),
                            if *w >= 0.0 { theme::OK } else { theme::DANGER },
                        );
                    }
                });
            })
            .response
            .interact(egui::Sense::click());
            if r.clicked() {
                f.preset = Some(p.name);
                f.avg_shot = p.avg_shot_seconds;
                f.cut_style = p.cut_style;
            }
        }
        field_label(ui, "Goal for this video");
        ui.add(
            egui::TextEdit::multiline(&mut f.goal)
                .hint_text("What should the viewer feel or learn?")
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            field_label(ui, "Avg shot");
            ui.add(
                egui::DragValue::new(&mut f.avg_shot)
                    .range(0.3..=20.0)
                    .speed(0.1)
                    .suffix(" s"),
            );
            ui.add_space(12.0);
            field_label(ui, "Cut style");
            egui::ComboBox::from_id_salt("np-cut")
                .selected_text(f.cut_style)
                .show_ui(ui, |ui| {
                    for c in spearfia_vision::CUT_STYLES {
                        ui.selectable_value(&mut f.cut_style, c, c);
                    }
                });
        });
    });
}

fn script(f: &mut NewProjectForm, ui: &mut egui::Ui) {
    section(ui, 3, "Script", |ui| {
        widgets::segmented(
            ui,
            &mut f.script_mode,
            &[
                (ScriptMode::Sections, "Blank with sections"),
                (ScriptMode::Import, "Import .md / .txt"),
            ],
        );
        if f.script_mode == ScriptMode::Import {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut f.import_path)
                        .desired_width(ui.available_width() - 80.0),
                );
                if widgets::button(ui, "Browse").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Script", &["md", "markdown", "txt"])
                        .pick_file()
                {
                    f.import_path = p.display().to_string();
                }
            });
            ui.label(muted(
                "Google Docs and .docx import come later. Export as Markdown for now.",
            ));
        }
    });
}

fn media(f: &mut NewProjectForm, ui: &mut egui::Ui) {
    section(ui, 4, "Media folders", |ui| {
        field_label(ui, "Projects folder");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut f.parent)
                    .hint_text(r"E:\Media\Learning to Drift")
                    .desired_width(ui.available_width() - 80.0),
            );
            if widgets::button(ui, "Browse").clicked()
                && let Some(p) = rfd::FileDialog::new().pick_folder()
            {
                f.parent = p.display().to_string();
                f.root_override = None;
            }
        });
        let root = f.root();
        ui.horizontal(|ui| {
            ui.label(muted("Project folder:"));
            let shown = if root.as_os_str().is_empty() {
                "not set".to_owned()
            } else {
                root.display().to_string()
            };
            ui.add(egui::Label::new(mono(shown.clone(), theme::TEXT_2)).truncate())
                .on_hover_text(shown);
        });
        ui.add_space(6.0);
        field_label(ui, "Subfolders (each has a role; subfolders inherit it)");
        egui::Frame::new()
            .fill(theme::BG_INSET)
            .corner_radius(6)
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for (folder, on) in &mut f.folders {
                    ui.horizontal(|ui| {
                        ui.checkbox(on, egui::RichText::new(&folder.path).monospace());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            widgets::tag(ui, folder.role.label(), theme::TEXT_2);
                        });
                    });
                }
            });
        ui.label(muted("Existing files in the folder are left alone."));
    });
}

fn filmcraft(f: &mut NewProjectForm, ui: &mut egui::Ui, found: bool) {
    section(ui, 5, "FilmCraft", |ui| {
        ui.checkbox(
            &mut f.filmcraft,
            "Pair this project with a FilmCraft project",
        );
        if !f.filmcraft {
            return;
        }
        if found {
            widgets::chip(ui, "FilmCraft found", theme::OK);
        } else {
            widgets::chip(
                ui,
                "FilmCraft not found: the link is saved and used once it is",
                theme::WARN,
            );
        }
        ui.add_space(4.0);
        widgets::kv(ui, "project file", mono(f.fcproj_name(), theme::TEXT));
        ui.horizontal(|ui| {
            field_label(ui, "Sequence");
            ui.add(egui::TextEdit::singleline(&mut f.sequence).desired_width(160.0));
        });
        field_label(ui, "Connection");
        widgets::segmented(
            ui,
            &mut f.conn,
            &[
                (Conn::Bridge, "Bridge to running app"),
                (Conn::Headless, "Headless (filmcraft-cli mcp)"),
            ],
        );
        if f.conn == Conn::Bridge {
            ui.horizontal(|ui| {
                field_label(ui, "Control port");
                ui.add(egui::DragValue::new(&mut f.port).range(1024..=65535));
            });
        }
        ui.label(muted(
            "The .fcproj is made on the first push, with tracks V1 A-roll, V2 B-roll, V3 Looks, \
             A1 VO, A2 SFX, A3 Music.",
        ));
    });
}

fn preview(f: &NewProjectForm, ui: &mut egui::Ui) {
    ui.label(strong("Will create", 13.5));
    ui.add_space(8.0);
    let root = f.root();
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| f.slug());
    let mut rows: Vec<(String, egui::Color32)> = vec![
        (format!("{name}/"), theme::TEXT),
        ("├ project.json".into(), theme::TEXT_2),
        ("├ vision.yaml".into(), theme::CUE_VISION),
        ("├ script/script.md".into(), theme::TEXT_2),
        ("├ links.json".into(), theme::TEXT_2),
    ];
    for (folder, on) in &f.folders {
        if *on {
            rows.push((format!("├ {}/", folder.path), theme::CUE_CLIP));
        }
    }
    for d in [
        "analysis/",
        "sync/",
        "cache/  (ignored)",
        "exports/  (ignored)",
    ] {
        rows.push((format!("├ {d}"), theme::TEXT_MUTED));
    }
    if f.filmcraft {
        rows.push((format!("└ {}  (first push)", f.fcproj_name()), theme::OK));
    }
    egui::Frame::new()
        .fill(theme::BG_INSET)
        .corner_radius(6)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (r, c) in rows {
                ui.label(mono(r, c));
            }
        });
    ui.add_space(12.0);
    if let Some(p) = f.preset.and_then(spearfia_vision::preset) {
        card(theme::BG_PANEL, theme::BORDER).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(strong("vision.yaml", 12.5));
            ui.label(mono(format!("extends: {}", p.name), theme::CUE_VISION));
            ui.label(mono(
                format!("avg_shot_seconds: {}", f.avg_shot),
                theme::TEXT_2,
            ));
            ui.label(mono(format!("cut_style: {}", f.cut_style), theme::TEXT_2));
        });
    }
}
