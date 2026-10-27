//! New Project: basics, vision, script start, media folders, FilmCraft, as steps that open
//! one after another (any step can be opened at any time). Shows what will
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
    channels: Vec<spearfia_core::channels::Channel>,
    channel: Option<String>,
    series: String,
    /// Folder new projects go in; the project folder is `<parent>/<slug>`.
    parent: String,
    /// The project folder, once the user edits it by hand.
    root_override: Option<String>,
    format: Format,
    resolution: [u32; 2],
    timebase: Timebase,
    runtime: String,
    preset: Option<String>,
    visions: Vec<spearfia_core::visions::Vision>,
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
    /// Which of the five steps are expanded. Only Basics starts open; Continue opens the
    /// next one, and any header can be clicked open at any time.
    open: [bool; STEPS],
    /// Steps the user has finished with Continue.
    done: [bool; STEPS],
    /// Scroll this step into view on the next frame (after Continue opens it).
    scroll_to: Option<usize>,
}

const STEPS: usize = 5;

impl NewProjectForm {
    pub fn new(
        projects_dir: Option<PathBuf>,
        series: Option<String>,
        visions: Vec<spearfia_core::visions::Vision>,
        channels: Vec<spearfia_core::channels::Channel>,
    ) -> Self {
        let preset = visions.first();
        Self {
            name: String::new(),
            channels,
            channel: None,
            series: series.unwrap_or_default(),
            parent: projects_dir
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            root_override: None,
            format: Format::LongForm,
            resolution: Format::LongForm.default_resolution(),
            timebase: Timebase::FPS_59_94,
            runtime: "10:00".into(),
            preset: preset.map(|p| p.name().to_owned()),
            goal: String::new(),
            avg_shot: preset.map_or(2.0, |p| p.avg_shot_seconds()),
            cut_style: preset.map_or("jump", |p| p.cut_style()),
            visions,
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
            open: [true, false, false, false, false],
            done: [false; STEPS],
            scroll_to: None,
        }
    }

    pub fn choose_channel(&mut self, id: &str) -> Result<(), String> {
        if !self.channels.iter().any(|c| c.id == id) {
            return Err("The selected channel is no longer available.".into());
        }
        self.channel = Some(id.to_owned());
        Ok(())
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
        let channel_brief = match &self.channel {
            Some(id) => Some(
                self.channels
                    .iter()
                    .find(|c| &c.id == id)
                    .ok_or_else(|| "The selected channel is no longer available.".to_owned())?
                    .brief()
                    .map_err(|e| e.to_string())?,
            ),
            None => None,
        };
        Ok(NewProject {
            channel_brief,
            name: self.name.trim().to_owned(),
            series: Some(self.series.clone()),
            root,
            format: self.format,
            resolution: self.resolution,
            timebase: self.timebase,
            target_runtime_seconds: self.runtime_seconds(),
            vision: NewVision {
                extends: self.preset.clone(),
                template: self
                    .preset
                    .as_deref()
                    .and_then(|name| self.visions.iter().find(|v| v.name() == name))
                    .map(|v| v.document.clone()),
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
                    // One centered column, top to bottom: each step opens below the one
                    // before. Text and fields are a size up from the rest of the app,
                    // since this is the first screen a new user fills in.
                    let w = ui.available_width().min(880.0);
                    let pad = (ui.available_width() - w) / 2.0;
                    ui.horizontal(|ui| {
                        ui.add_space(pad);
                        ui.vertical(|ui| {
                            ui.set_width(w);
                            let s = ui.style_mut();
                            for (style, size) in [
                                (egui::TextStyle::Body, 15.0),
                                (egui::TextStyle::Button, 15.0),
                                (egui::TextStyle::Monospace, 14.0),
                                (egui::TextStyle::Small, 12.5),
                            ] {
                                if let Some(f) = s.text_styles.get_mut(&style) {
                                    f.size = size;
                                }
                            }
                            s.spacing.interact_size.y = 32.0;
                            s.spacing.button_padding = egui::vec2(12.0, 6.0);
                            s.spacing.item_spacing.y = 8.0;
                            basics(form, ui);
                            ui.add_space(14.0);
                            vision(form, ui);
                            ui.add_space(14.0);
                            script(form, ui);
                            ui.add_space(14.0);
                            media(form, ui);
                            ui.add_space(14.0);
                            filmcraft(form, ui, app.filmcraft.is_some());
                        });
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

/// One step of the form: a header that expands or collapses it (showing a one-line summary
/// when collapsed), the step's fields, and Continue, which is enabled once `ready` is `Ok`
/// and opens the next step. Any step can be opened by its header, ready or not.
fn step(
    ui: &mut egui::Ui,
    f: &mut NewProjectForm,
    i: usize,
    title: &str,
    summary: String,
    ready: Result<(), &str>,
    body: impl FnOnce(&mut NewProjectForm, &mut egui::Ui),
) {
    let open = f.open[i];
    let finished = f.done[i] && ready.is_ok();
    let border = if open {
        theme::BORDER_STRONG
    } else {
        theme::BORDER
    };
    let r = card(theme::BG_PANEL, border)
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let head = ui
                .horizontal(|ui| {
                    widgets::tag(
                        ui,
                        &(i + 1).to_string(),
                        if finished { theme::OK } else { theme::TEXT },
                    );
                    ui.label(strong(title, 17.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut glyph = |icon, color| {
                            let (r, _) = ui
                                .allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            crate::icons::paint(ui.painter(), r, icon, color);
                        };
                        if !open {
                            glyph(crate::icons::Icon::Down, theme::TEXT_MUTED);
                        }
                        if finished {
                            glyph(crate::icons::Icon::Check, theme::OK);
                        }
                        if !open {
                            ui.add(egui::Label::new(muted(summary)).truncate());
                        }
                    });
                })
                .response
                .interact(egui::Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if head.clicked() {
                f.open[i] = !open;
            }
            if !open {
                return;
            }
            ui.add_space(8.0);
            body(f, ui);
            if i + 1 < STEPS {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let go = ui
                        .add_enabled_ui(ready.is_ok(), |ui| widgets::primary_button(ui, "Continue"))
                        .inner;
                    if let Err(why) = ready {
                        ui.label(muted(why));
                    }
                    if go.clicked() {
                        f.done[i] = true;
                        f.open[i] = false;
                        f.open[i + 1] = true;
                        f.scroll_to = Some(i + 1);
                    }
                });
            }
        });
    if f.scroll_to == Some(i) {
        r.response.scroll_to_me(Some(Align::TOP));
        f.scroll_to = None;
    }
}

fn field_label(ui: &mut egui::Ui, s: &str) {
    ui.label(text(s, 13.5, theme::TEXT_2));
}

fn basics(f: &mut NewProjectForm, ui: &mut egui::Ui) {
    let summary = format!(
        "{} · {} · {}×{} · {}",
        if f.name.trim().is_empty() {
            "Untitled"
        } else {
            f.name.trim()
        },
        f.format.label(),
        f.resolution[0],
        f.resolution[1],
        f.timebase.label()
    );
    let ready = if f.name.trim().is_empty() {
        Err("Add a project name to continue.")
    } else if f.root().as_os_str().is_empty() {
        Err("Choose a projects folder to continue.")
    } else {
        Ok(())
    };
    step(ui, f, 0, "Basics", summary, ready, |f, ui| {
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
        field_label(ui, "Channel (optional)");
        egui::ComboBox::from_id_salt("new-project-channel")
            .selected_text(
                f.channels
                    .iter()
                    .find(|c| Some(&c.id) == f.channel.as_ref())
                    .map_or("No channel", |c| c.title.as_str()),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.channel, None, "No channel");
                for c in &f.channels {
                    ui.selectable_value(&mut f.channel, Some(c.id.clone()), &c.title);
                }
            });
        ui.label(muted(
            "The video keeps a snapshot of its channel guides; its Vision can vary by episode.",
        ));
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
    });
}

fn vision(f: &mut NewProjectForm, ui: &mut egui::Ui) {
    let preset = f
        .preset
        .as_deref()
        .and_then(|name| f.visions.iter().find(|v| v.name() == name))
        .map_or("No preset", |p| p.title());
    let summary = format!("{preset} · {:.1}s shots · {} cuts", f.avg_shot, f.cut_style);
    step(ui, f, 1, "Vision", summary, Ok(()), |f, ui| {
        ui.label(muted(
            "Steers the AI's script notes, catalog ranking and pacing. Pick a preset, then adjust.",
        ));
        ui.add_space(4.0);
        for p in &f.visions {
            let on = f.preset.as_deref() == Some(p.name());
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
                        p.title(),
                        15.0,
                        if on { theme::CUE_VISION } else { theme::TEXT },
                    )
                    .strong(),
                );
                ui.label(muted(p.goal()));
                ui.horizontal_wrapped(|ui| {
                    for (k, w) in p
                        .weight_map()
                        .into_iter()
                        .collect::<std::collections::BTreeMap<_, _>>()
                    {
                        widgets::tag(
                            ui,
                            &format!("{k} {w:+.1}"),
                            if w >= 0.0 { theme::OK } else { theme::DANGER },
                        );
                    }
                });
            })
            .response
            .interact(egui::Sense::click());
            if r.clicked() {
                f.preset = Some(p.name().to_owned());
                f.avg_shot = p.avg_shot_seconds();
                f.cut_style = p.cut_style();
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
    let import = f.script_mode == ScriptMode::Import;
    let file = std::path::Path::new(f.import_path.trim())
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    let summary = match (import, &file) {
        (true, Some(n)) => format!("Import {n}"),
        (true, None) => "Import a file".into(),
        (false, _) => "Blank with sections".into(),
    };
    let ready = if import && file.is_none() {
        Err("Choose the file to import, or start blank.")
    } else {
        Ok(())
    };
    step(ui, f, 2, "Script", summary, ready, |f, ui| {
        widgets::segmented(
            ui,
            &mut f.script_mode,
            &[
                (ScriptMode::Sections, "Blank with sections"),
                (ScriptMode::Import, "Import .docx / .md / .txt"),
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
                        .add_filter("Script", &["docx", "md", "markdown", "txt"])
                        .pick_file()
                {
                    f.import_path = p.display().to_string();
                }
            });
            ui.label(muted(
                "Word headings become sections; bold, italic, lists and tables carry over. \
                 From Google Docs: File → Download → Microsoft Word (.docx).",
            ));
        }
    });
}

fn media(f: &mut NewProjectForm, ui: &mut egui::Ui) {
    let on = f.folders.iter().filter(|(_, on)| *on).count();
    let summary = format!("{on} subfolder{}", if on == 1 { "" } else { "s" });
    step(ui, f, 3, "Media folders", summary, Ok(()), |f, ui| {
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
    let summary = match (f.filmcraft, f.conn) {
        (false, _) => "Not paired".to_owned(),
        (true, Conn::Bridge) => format!("Paired · bridge on port {}", f.port),
        (true, Conn::Headless) => "Paired · headless".to_owned(),
    };
    step(ui, f, 4, "FilmCraft", summary, Ok(()), |f, ui| {
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
    if f.channel.is_some() {
        rows.push(("├ channel.md  (channel guides)".into(), theme::CUE_VISION));
    }
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
    if let Some(p) = f
        .preset
        .as_deref()
        .and_then(|name| f.visions.iter().find(|v| v.name() == name))
    {
        card(theme::BG_PANEL, theme::BORDER).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(strong("vision.yaml", 12.5));
            ui.label(mono(format!("extends: {}", p.name()), theme::CUE_VISION));
            ui.label(mono(
                format!("avg_shot_seconds: {}", f.avg_shot),
                theme::TEXT_2,
            ));
            ui.label(mono(format!("cut_style: {}", f.cut_style), theme::TEXT_2));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_template_reaches_the_project_request() {
        let custom = spearfia_core::visions::Vision::parse(r#"{"name":"science","goal":"Explain clearly","weights":{"diagram":0.9},"script_rules":["Use examples"]}"#).unwrap();
        let mut templates = spearfia_vision::builtin_templates();
        templates.push(custom.clone());
        let mut form = NewProjectForm::new(Some(std::env::temp_dir()), None, templates, Vec::new());
        form.name = "Vision test".into();
        form.preset = Some("science".into());
        let request = form.to_spec().unwrap();
        assert_eq!(request.vision.extends.as_deref(), Some("science"));
        assert_eq!(request.vision.template, Some(custom.document));
    }
    #[test]
    fn chosen_channel_embeds_saved_guides_in_new_video_request() {
        let dir = std::env::temp_dir().join(format!(
            "spearfia-new-channel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let channel = spearfia_core::channels::create(&dir, "Channel test").unwrap();
        channel
            .add_document("analysis.md", "# Analysis\nUse recurring diagrams.")
            .unwrap();
        let mut form = NewProjectForm::new(
            Some(dir.clone()),
            None,
            spearfia_vision::builtin_templates(),
            vec![channel.clone()],
        );
        form.name = "Episode one".into();
        form.choose_channel(&channel.id).unwrap();
        let request = form.to_spec().unwrap();
        let brief = request.channel_brief.unwrap();
        assert!(brief.contains("Channel test") && brief.contains("Use recurring diagrams."));
        assert!(form.choose_channel("missing").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
