//! Media stage: the project's linked folders on the left, and in the middle either a grid
//! of clips or the script (read-only) for linking. To link: highlight words (the highlight
//! stays), then drag a clip from the folders onto it. Dropping on a line with nothing
//! highlighted links the whole line.

use crate::script_view::{self, Interaction, MediaDrag, Resolved, Selection};
use crate::theme;
use crate::widgets::{self, mono, muted, panel, strong, text};
use egui::{Align, CornerRadius, Layout, Margin, Panel, Sense, Stroke};
use spearfia_core::links::{Link, LinkKind, LinkMedia, Links};
use spearfia_core::media::{self, FolderNode, MediaFile, MediaKind};
use spearfia_core::project::{MediaRole, Project};
use spearfia_script::{Block, Script};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Grid,
    Script,
}

const ROLES: [MediaRole; 6] = [
    MediaRole::ARoll,
    MediaRole::Voiceover,
    MediaRole::BRoll,
    MediaRole::Screen,
    MediaRole::Graphics,
    MediaRole::Other,
];

#[derive(Clone)]
struct FolderDrag {
    path: String,
}

enum FolderAction {
    Import(String, Vec<PathBuf>),
    Move(String, Vec<String>),
}

struct NewFolder {
    parent: String,
    name: String,
    role: MediaRole,
}

pub struct MediaStage {
    tree: Vec<FolderNode>,
    new_folder: Option<NewFolder>,
    folder_action: Option<FolderAction>,
    drop_pointer: Option<egui::Pos2>,
    copying: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
    notice: Option<String>,
    selected_folder: Option<String>,
    expanded: HashSet<String>,
    view: View,
    interaction: Interaction,
    selected_link: Option<String>,
    selected_clips: HashSet<String>,
    hovered_paths: HashSet<String>,
    reveal_hovered: bool,
    /// A folder picked with "Link folder", waiting for its role.
    pending: Option<(PathBuf, MediaRole)>,
    error: Option<String>,
    /// Block to bring into view on the next frame.
    scroll_to: Option<usize>,
}

impl MediaStage {
    /// Show the script view with `sel` highlighted, ready for a clip to be dropped on it
    /// (from "Link clip" on the Script page).
    pub fn focus_selection(&mut self, sel: Selection) {
        self.view = View::Script;
        self.selected_link = None;
        self.interaction.selection = Some(sel);
        self.scroll_to = Some(sel.block);
    }

    pub fn new(project: &Project) -> Self {
        let mut s = Self {
            tree: Vec::new(),
            new_folder: None,
            folder_action: None,
            drop_pointer: None,
            copying: None,
            notice: None,
            selected_folder: None,
            expanded: HashSet::new(),
            view: View::Script,
            interaction: Interaction::default(),
            selected_link: None,
            selected_clips: HashSet::new(),
            hovered_paths: HashSet::new(),
            reveal_hovered: false,
            pending: None,
            error: None,
            scroll_to: None,
        };
        s.rescan(project);
        s
    }

    fn rescan(&mut self, project: &Project) {
        self.tree = media::scan(project);
        for n in &self.tree {
            self.expanded.insert(n.path.clone());
        }
        let still_there = self
            .selected_folder
            .as_deref()
            .is_some_and(|p| self.tree.iter().any(|n| n.find(p).is_some()));
        if !still_there {
            // Default to the first folder that has media in it.
            self.selected_folder = self
                .tree
                .iter()
                .flat_map(|n| std::iter::once(n).chain(n.children.iter()))
                .find(|n| !n.files.is_empty())
                .or(self.tree.first())
                .map(|n| n.path.clone());
        }
    }

    fn folder(&self) -> Option<&FolderNode> {
        let p = self.selected_folder.as_deref()?;
        self.tree.iter().find_map(|n| n.find(p))
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &mut Project,
        script: &Script,
        src: &str,
        links: &mut Links,
    ) {
        self.poll_copy(project, ui.ctx());
        self.drop_pointer =
            if ui.input(|i| !i.raw.hovered_files.is_empty() || !i.raw.dropped_files.is_empty()) {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(30));
                crate::file_drop::pointer(ui.ctx())
            } else {
                None
            };
        Panel::left("media-folders")
            .exact_size(300.0)
            .frame(panel(theme::BG_PANEL).inner_margin(Margin::symmetric(10, 14)))
            .show(ui, |ui| self.folders_panel(ui, project, links));
        self.apply_folder_action(project, links, ui.ctx());
        let resolved = script_view::resolve(script, links);

        Panel::right("media-selection")
            .exact_size(330.0)
            .frame(panel(theme::BG_PANEL).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        self.side_panel(ui, project, script, links, &resolved)
                    });
            });

        egui::CentralPanel::default()
            .frame(panel(theme::BG_APP))
            .show(ui, |ui| {
                self.header(ui);
                match self.view {
                    View::Grid => self.grid(ui, links),
                    View::Script => {
                        let ev = script_view::page(
                            ui,
                            script,
                            script_view::Options {
                                src,
                                resolved: &resolved,
                                interaction: Some(&mut self.interaction),
                                scroll_to: self.scroll_to.take(),
                                selected_link: self.selected_link.as_deref(),
                            },
                        );
                        let hovered_paths: HashSet<String> = links
                            .links
                            .iter()
                            .filter(|l| ev.hovered_links.contains(&l.id))
                            .map(|l| l.media.path.clone())
                            .collect();
                        if hovered_paths != self.hovered_paths {
                            self.reveal_hovered = !hovered_paths.is_empty();
                            self.hovered_paths = hovered_paths;
                            ui.ctx().request_repaint();
                        }
                        if let Some(id) = ev.clicked_link {
                            self.selected_link = Some(id);
                        } else if ev.dropped.is_none() && self.interaction.selection.is_some() {
                            // Selecting words clears the selected link.
                            self.selected_link = None;
                        }
                        if let Some((sel, drag)) = ev.dropped {
                            self.link(project, script, links, sel, &drag);
                        }
                    }
                }
            });
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(theme::BG_PANEL)
            .inner_margin(Margin::symmetric(16, 7))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    widgets::segmented(
                        ui,
                        &mut self.view,
                        &[(View::Grid, "Grid"), (View::Script, "Script")],
                    );
                    match self.view {
                        View::Grid => {
                            let (name, n) = self
                                .folder()
                                .map(|f| (f.path.clone(), f.all_files().len()))
                                .unwrap_or_default();
                            ui.label(strong(name, 13.5));
                            ui.label(mono(format!("{n} files"), theme::TEXT_MUTED));
                        }
                        View::Script => {
                            ui.label(strong("Link footage to the script", 13.5));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                widgets::chip(
                                    ui,
                                    "Read-only · edit words on Script",
                                    theme::TEXT_MUTED,
                                );
                            });
                        }
                    }
                });
            });
    }

    // ---- left panel -------------------------------------------------------------------

    fn folders_panel(&mut self, ui: &mut egui::Ui, project: &mut Project, links: &Links) {
        ui.horizontal(|ui| {
            let root =
                ui.add(egui::Label::new(strong("Project media", 12.5)).sense(Sense::hover()));
            self.folder_target(ui, &root, ".", true);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(egui::Button::new(text("+ Link folder", 12.0, theme::ACCENT)).frame(false))
                    .clicked()
                    && let Some(dir) = rfd::FileDialog::new()
                        .set_directory(&project.root)
                        .pick_folder()
                {
                    self.pending = Some((dir, MediaRole::BRoll));
                }
                if ui
                    .add(egui::Button::new(text("Rescan", 12.0, theme::TEXT_2)).frame(false))
                    .clicked()
                {
                    self.rescan(project);
                }
            });
        });
        ui.label(mono(project.root.display().to_string(), theme::TEXT_MUTED));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.copying.is_none(), egui::Button::new("+ New folder"))
                .clicked()
            {
                let parent = self.selected_folder.clone().unwrap_or_else(|| ".".into());
                let role = self.folder().map_or(MediaRole::Other, |n| n.role);
                self.new_folder = Some(NewFolder {
                    parent,
                    name: String::new(),
                    role,
                });
            }
            if self.copying.is_some() {
                ui.spinner();
                ui.label(muted("Copying files…"));
            }
        });
        if let Some(form) = &mut self.new_folder {
            let mut done = None;
            widgets::card(theme::BG_RAISED, theme::ACCENT).show(ui, |ui| {
                ui.label(muted("New folder in"));
                egui::ComboBox::from_id_salt("new-media-parent")
                    .selected_text(if form.parent == "." {
                        "Project media"
                    } else {
                        &form.parent
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut form.parent, ".".into(), "Project media");
                        fn choices(ui: &mut egui::Ui, nodes: &[FolderNode], parent: &mut String) {
                            for n in nodes {
                                if n.exists {
                                    ui.selectable_value(parent, n.path.clone(), &n.path);
                                }
                                choices(ui, &n.children, parent);
                            }
                        }
                        choices(ui, &self.tree, &mut form.parent);
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut form.name)
                        .hint_text("Folder name")
                        .desired_width(240.0),
                );
                egui::ComboBox::from_id_salt("new-media-role")
                    .selected_text(form.role.label())
                    .show_ui(ui, |ui| {
                        for role in ROLES {
                            ui.selectable_value(&mut form.role, role, role.label());
                        }
                    });
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, "Create").clicked() {
                        done = Some(true);
                    }
                    if widgets::button(ui, "Cancel").clicked() {
                        done = Some(false);
                    }
                });
            });
            if done == Some(true) {
                match project.create_media_folder(&form.parent, &form.name, form.role) {
                    Ok(path) => {
                        self.expanded.insert(form.parent.clone());
                        self.selected_folder = Some(path);
                        self.new_folder = None;
                        self.error = None;
                        self.rescan(project);
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
            } else if done == Some(false) {
                self.new_folder = None;
            }
        }
        if let Some(notice) = &self.notice {
            ui.label(muted(notice));
        }

        if let Some((dir, role)) = &mut self.pending {
            let mut done = None;
            widgets::card(theme::BG_RAISED, theme::ACCENT).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(text(format!("Link {}", dir.display()), 12.0, theme::TEXT));
                ui.horizontal(|ui| {
                    ui.label(muted("Role"));
                    egui::ComboBox::from_id_salt("link-role")
                        .selected_text(role.label())
                        .show_ui(ui, |ui| {
                            for r in ROLES {
                                ui.selectable_value(role, r, r.label());
                            }
                        });
                });
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, "Link").clicked() {
                        done = Some(true);
                    }
                    if widgets::button(ui, "Cancel").clicked() {
                        done = Some(false);
                    }
                });
            });
            match done {
                Some(true) => {
                    let (dir, role) = self.pending.take().expect("pending folder");
                    match project.add_media_folder(&dir, role) {
                        Ok(()) => {
                            self.selected_folder = Some(media::display_path(project, &dir));
                            self.rescan(project);
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                Some(false) => self.pending = None,
                None => {}
            }
        }
        if let Some(e) = self.error.clone() {
            ui.horizontal(|ui| {
                ui.label(text(e, 12.0, theme::DANGER));
                if ui.small_button("×").clicked() {
                    self.error = None;
                }
            });
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                if self.tree.is_empty() {
                    ui.label(muted("No media folders yet. Use + Link folder."));
                }
                let selected_files = self.selected_files();
                let tree = std::mem::take(&mut self.tree);
                for n in &tree {
                    self.node(ui, n, 0, links, &selected_files);
                }
                self.tree = tree;
                ui.add_space(10.0);
                ui.label(muted(
                    "Drop Explorer files onto a folder to copy them. Drag files or folders here to move them; drop folders on Project media to make them top-level.",
                ));
            });
    }

    fn node(
        &mut self,
        ui: &mut egui::Ui,
        n: &FolderNode,
        depth: usize,
        links: &Links,
        selected_files: &[MediaFile],
    ) {
        let hovered = n
            .all_files()
            .iter()
            .any(|f| self.hovered_paths.contains(&f.path));
        let open = self.expanded.contains(&n.path) || hovered;
        let selected = self.selected_folder.as_deref() == Some(n.path.as_str());
        let r = ui
            .dnd_drag_source(
                egui::Id::new(("media-folder", &n.path)),
                FolderDrag {
                    path: n.path.clone(),
                },
                |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(depth as f32 * 14.0);
                        let has_children = !n.children.is_empty();
                        let (ar, aresp) =
                            ui.allocate_exact_size(egui::vec2(12.0, 16.0), Sense::click());
                        if has_children {
                            let c = ar.center();
                            let pts = if open {
                                vec![
                                    c + egui::vec2(-3.5, -2.0),
                                    c + egui::vec2(3.5, -2.0),
                                    c + egui::vec2(0.0, 2.5),
                                ]
                            } else {
                                vec![
                                    c + egui::vec2(-2.0, -3.5),
                                    c + egui::vec2(2.5, 0.0),
                                    c + egui::vec2(-2.0, 3.5),
                                ]
                            };
                            ui.painter().add(egui::Shape::convex_polygon(
                                pts,
                                theme::TEXT_MUTED,
                                Stroke::NONE,
                            ));
                            if aresp.clicked() {
                                if open {
                                    self.expanded.remove(&n.path);
                                } else {
                                    self.expanded.insert(n.path.clone());
                                }
                            }
                        }
                        folder_icon(
                            ui,
                            if selected {
                                theme::ACCENT
                            } else {
                                theme::TEXT_2
                            },
                        );
                        let name = if n.exists {
                            n.name.clone()
                        } else {
                            format!("{} (missing)", n.name)
                        };
                        ui.label(text(
                            name,
                            12.5,
                            if !n.exists {
                                theme::DANGER
                            } else if selected {
                                theme::TEXT
                            } else {
                                theme::TEXT_2
                            },
                        ));
                        if depth == 0 || n.role != MediaRole::Other {
                            widgets::tag(ui, n.role.label(), script_view::role_color(n.role));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(mono(n.all_files().len().to_string(), theme::TEXT_MUTED));
                        });
                    })
                },
            )
            .response
            .interact(Sense::click_and_drag());
        if n.exists {
            self.folder_target(ui, &r, &n.path, false);
        }
        if selected || hovered {
            ui.painter().rect_filled(
                r.rect.expand(2.0),
                4.0,
                theme::tint(
                    if hovered {
                        theme::CUE_CLIP
                    } else {
                        theme::TEXT
                    },
                    if hovered { 0.18 } else { 0.05 },
                ),
            );
        }
        if r.clicked() {
            self.selected_folder = Some(n.path.clone());
        }

        // The selected folder lists its own clips as drag sources.
        if selected || n.files.iter().any(|f| self.hovered_paths.contains(&f.path)) {
            for f in &n.files {
                self.clip_row(ui, f, depth + 1, links, selected_files);
            }
            if n.files.is_empty() && n.children.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space((depth + 1) as f32 * 14.0 + 16.0);
                    ui.label(muted("empty"));
                });
            }
        }
        if open {
            for c in &n.children {
                self.node(ui, c, depth + 1, links, selected_files);
            }
        }
    }

    fn folder_target(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        path: &str,
        root: bool,
    ) {
        if self.copying.is_some() {
            return;
        }
        let external =
            ui.input(|i| !i.raw.hovered_files.is_empty() || !i.raw.dropped_files.is_empty());
        let over = if external {
            self.drop_pointer
                .is_some_and(|p| response.rect.contains(p) && ui.clip_rect().contains(p))
        } else {
            response.contains_pointer()
        };
        let folder = response.dnd_hover_payload::<FolderDrag>();
        let clips = if root {
            None
        } else {
            response.dnd_hover_payload::<MediaDrag>()
        };
        let active = folder
            .as_ref()
            .is_some_and(|f| f.path != path && !std::path::Path::new(path).starts_with(&f.path))
            || clips.is_some()
            || (external && !root && over);
        if active {
            ui.painter().rect_filled(
                response.rect.expand(2.0),
                4.0,
                theme::tint(theme::ACCENT, 0.23),
            );
            ui.painter().rect_stroke(
                response.rect.expand(2.0),
                4.0,
                Stroke::new(1.5, theme::ACCENT),
                egui::StrokeKind::Inside,
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
            if !root {
                self.expanded.insert(path.to_owned());
            }
        }
        let released_folder = folder
            .as_ref()
            .and_then(|_| response.dnd_release_payload::<FolderDrag>());
        if let Some(drag) = released_folder {
            if drag.path != path {
                self.folder_action = Some(FolderAction::Move(path.into(), vec![drag.path.clone()]));
            }
        } else if !root {
            if let Some(drag) = clips
                .as_ref()
                .and_then(|_| response.dnd_release_payload::<MediaDrag>())
            {
                self.folder_action = Some(FolderAction::Move(
                    path.into(),
                    drag.clips.iter().map(|f| f.path.clone()).collect(),
                ));
            } else if over && external {
                let sources = ui.input(|i| {
                    i.raw
                        .dropped_files
                        .iter()
                        .map(|f| f.path().to_path_buf())
                        .collect::<Vec<_>>()
                });
                if !sources.is_empty() {
                    self.folder_action = Some(FolderAction::Import(path.into(), sources));
                }
            }
        }
    }

    fn apply_folder_action(
        &mut self,
        project: &mut Project,
        links: &mut Links,
        ctx: &egui::Context,
    ) {
        let Some(action) = self.folder_action.take() else {
            return;
        };
        self.error = None;
        self.notice = None;
        match action {
            FolderAction::Import(dest, sources) => {
                self.selected_folder = Some(dest.clone());
                self.expanded.insert(dest.clone());
                let snapshot = project.clone();
                let ctx = ctx.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                match std::thread::Builder::new().name("media-import".into()).spawn(move || {
                    let result = std::panic::catch_unwind(|| snapshot.import_media_files(&dest, &sources))
                        .map_err(|_| "The media copy worker failed. Originals are unchanged; rescan the destination before retrying.".to_owned())
                        .and_then(|r| r.map_err(|e| e.to_string()));
                    let _ = tx.send(result);
                    ctx.request_repaint();
                }) {
                    Ok(_) => self.copying = Some(rx),
                    Err(e) => self.error = Some(format!("Could not start copying files: {e}")),
                }
            }
            FolderAction::Move(parent, paths) => {
                let mut count = 0;
                for path in paths {
                    match project.move_media(&path, &parent, links) {
                        Ok(moved) => {
                            count += 1;
                            let remap = |p: &String| {
                                std::path::Path::new(p)
                                    .strip_prefix(&path)
                                    .ok()
                                    .map(|tail| {
                                        std::path::Path::new(&moved)
                                            .join(tail)
                                            .to_string_lossy()
                                            .replace('\\', "/")
                                    })
                                    .unwrap_or_else(|| p.clone())
                            };
                            self.selected_clips = self.selected_clips.iter().map(remap).collect();
                            self.expanded = self.expanded.iter().map(remap).collect();
                            self.selected_folder = Some(if project.resolve(&moved).is_dir() {
                                moved
                            } else {
                                parent.clone()
                            });
                        }
                        Err(e) => {
                            self.error = Some(format!("{count} item(s) moved. {e}"));
                            break;
                        }
                    }
                }
                self.expanded.insert(parent);
                self.hovered_paths.clear();
                self.rescan(project);
            }
        }
    }

    fn poll_copy(&mut self, project: &Project, ctx: &egui::Context) {
        let result = self.copying.as_ref().map(|rx| rx.try_recv());
        match result {
            Some(Ok(result)) => {
                self.copying = None;
                match result {
                    Ok(paths) => {
                        self.notice = Some(format!(
                            "Copied {} file(s). Originals kept in place.",
                            paths.len()
                        ))
                    }
                    Err(e) => self.error = Some(e),
                }
                self.rescan(project);
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.copying = None;
                self.error = Some("The media copy worker stopped. Rescan before retrying; originals are unchanged.".into());
                self.rescan(project);
            }
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
            None => {}
        }
    }

    fn selected_files(&self) -> Vec<MediaFile> {
        let mut seen = HashSet::new();
        self.tree
            .iter()
            .flat_map(|n| n.all_files())
            .filter(|f| self.selected_clips.contains(&f.path) && seen.insert(f.path.clone()))
            .cloned()
            .collect()
    }

    fn clip_row(
        &mut self,
        ui: &mut egui::Ui,
        f: &MediaFile,
        depth: usize,
        links: &Links,
        selected_files: &[MediaFile],
    ) {
        let hovered = self.hovered_paths.contains(&f.path);
        let uses = links.uses(&f.path);
        let row = ui
            .horizontal(|ui| {
                ui.add_space(depth as f32 * 14.0 + 4.0);
                let mut checked = self.selected_clips.contains(&f.path);
                if ui
                    .checkbox(&mut checked, "")
                    .on_hover_text("Include this clip when linking the selection")
                    .changed()
                {
                    if checked {
                        self.selected_clips.insert(f.path.clone());
                    } else {
                        self.selected_clips.remove(&f.path);
                    }
                }
                let clips = if checked {
                    let mut clips = selected_files.to_vec();
                    if !clips.iter().any(|clip| clip.path == f.path) {
                        clips.push(f.clone());
                    }
                    clips
                } else {
                    vec![f.clone()]
                };
                let clips = if clips.is_empty() {
                    vec![f.clone()]
                } else {
                    clips
                };
                ui.dnd_drag_source(
                    egui::Id::new(("clip", &f.path)),
                    MediaDrag { clips },
                    |ui| {
                        egui::Frame::new()
                            .fill(if hovered {
                                theme::tint(theme::CUE_CLIP, 0.25)
                            } else {
                                theme::BG_RAISED
                            })
                            .stroke(Stroke::new(
                                1.0,
                                if hovered {
                                    theme::CUE_CLIP
                                } else {
                                    theme::BORDER
                                },
                            ))
                            .corner_radius(5)
                            .inner_margin(Margin::symmetric(6, 4))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    thumb(ui, egui::vec2(40.0, 24.0), f);
                                    ui.vertical(|ui| {
                                        ui.set_max_width(155.0);
                                        ui.add(
                                            egui::Label::new(text(&f.name, 12.0, theme::TEXT))
                                                .truncate(),
                                        );
                                        ui.label(mono(
                                            if uses > 0 {
                                                format!("{} · linked {uses}×", size(f.bytes))
                                            } else {
                                                size(f.bytes)
                                            },
                                            if uses > 0 {
                                                theme::CUE_CLIP
                                            } else {
                                                theme::TEXT_MUTED
                                            },
                                        ));
                                    });
                                });
                            });
                    },
                )
                .response
                .on_hover_text(
                    "Drag onto the script. Dragging a checked clip links all checked clips.",
                );
            })
            .response;
        if hovered && self.reveal_hovered {
            ui.scroll_to_rect(row.rect, Some(egui::Align::Center));
            self.reveal_hovered = false;
        }
    }

    // ---- grid ---------------------------------------------------------------------------

    fn grid(&mut self, ui: &mut egui::Ui, links: &Links) {
        let files: Vec<MediaFile> = self
            .folder()
            .map(|f| f.all_files().into_iter().cloned().collect())
            .unwrap_or_default();
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin::same(20))
                    .show(ui, |ui| {
                        if files.is_empty() {
                            ui.label(muted(
                            "No media in this folder yet. Copy clips into it, then press Rescan.",
                        ));
                            return;
                        }
                        let card_w = 200.0;
                        let gap = 14.0;
                        let per_row = (((ui.available_width() + gap) / (card_w + gap)).floor()
                            as usize)
                            .max(1);
                        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                        for chunk in files.chunks(per_row) {
                            ui.horizontal_top(|ui| {
                                for f in chunk {
                                    grid_card(ui, f, card_w, links.uses(&f.path));
                                }
                            });
                        }
                    });
            });
    }

    // ---- right panel --------------------------------------------------------------------

    fn side_panel(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        script: &Script,
        links: &mut Links,
        resolved: &Resolved,
    ) {
        if let Some(id) = self.selected_link.clone() {
            if let Some(l) = links.links.iter().find(|l| l.id == id).cloned() {
                ui.horizontal(|ui| {
                    ui.label(strong("Link", 12.5));
                    ui.label(mono(format!("#{}", l.id), theme::TEXT_MUTED));
                });
                widgets::card(
                    theme::tint(script_view::role_color(l.role), 0.08),
                    script_view::role_color(l.role),
                )
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(text(quoted(&l.quote), 13.0, theme::TEXT).italics());
                });
                widgets::kv(ui, "media", text(&l.media.path, 12.0, theme::TEXT));
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [92.0, 16.0],
                        egui::Label::new(mono("role", theme::TEXT_MUTED)),
                    );
                    widgets::tag(ui, l.role.label(), script_view::role_color(l.role));
                    widgets::tag(
                        ui,
                        match l.kind {
                            LinkKind::Picture => "picture",
                            LinkKind::Audio => "audio",
                        },
                        theme::TEXT_2,
                    );
                });
                if resolved.lost.contains(&l.id) {
                    ui.label(text(
                        "These words are no longer in the script.",
                        12.0,
                        theme::DANGER,
                    ));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if widgets::button(ui, "Unlink").clicked() {
                        self.error = links
                            .remove_saved(project, &id)
                            .err()
                            .map(|e| format!("Couldn't save links.json: {e}"));
                        self.selected_link = None;
                    }
                    if widgets::button(ui, "Show in folder").clicked()
                        && let Some(dir) = project.resolve(&l.media.path).parent()
                    {
                        crate::reveal(dir);
                    }
                });
                ui.add_space(8.0);
                widgets::rule(ui);
                ui.add_space(8.0);
            } else {
                self.selected_link = None;
            }
        }

        if let Some(sel) = self.interaction.selection {
            let quote = quote_of(script, sel).unwrap_or_default();
            ui.label(strong("Selection", 12.5));
            widgets::card(theme::tint(theme::ACCENT, 0.08), theme::ACCENT).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(text(quoted(&quote), 13.0, theme::TEXT).italics());
            });
            ui.label(muted(
                "Drag clips onto the highlight, or check clips on the left and link them together. The selection stays; Esc clears it.",
            ));
            let selected = self.selected_files();
            if ui
                .add_enabled(
                    !selected.is_empty(),
                    egui::Button::new(format!("Link {} checked clip(s)", selected.len())),
                )
                .clicked()
            {
                self.link(project, script, links, sel, &MediaDrag { clips: selected });
            }
            if widgets::button(ui, "Clear selection").clicked() {
                self.interaction.selection = None;
            }
            ui.add_space(6.0);
            let matches = name_matches(&quote, &self.tree);
            if !matches.is_empty() {
                ui.label(strong("Name matches", 12.0));
                let mut chosen = None;
                for (f, score) in matches.iter().take(5) {
                    ui.horizontal(|ui| {
                        thumb(ui, egui::vec2(40.0, 24.0), f);
                        ui.vertical(|ui| {
                            ui.label(text(&f.name, 12.0, theme::TEXT));
                            ui.label(mono(format!("{score} matching word(s)"), theme::TEXT_MUTED));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("Link").clicked() {
                                chosen = Some(MediaDrag {
                                    clips: vec![f.clone()],
                                });
                            }
                        });
                    });
                }
                if let Some(d) = chosen {
                    self.link(project, script, links, sel, &d);
                }
                ui.label(muted("Matched on file names for now. Matching on transcripts and detected moments comes with analysis."));
            }
            ui.add_space(8.0);
            widgets::rule(ui);
            ui.add_space(8.0);
        } else if self.selected_link.is_none() {
            ui.label(strong("How linking works", 12.5));
            for s in [
                "1  Drag across words, lines and blank breaks, or click a section heading to select the section.",
                "2  Check multiple clips and link them together, or drag clips onto the selection. Drop on a heading to link the section.",
                "3  Linked text stays highlighted; overlaps are more transparent. Hover for a second to see the linked clip names and highlight them on the left.",
            ] {
                ui.label(text(s, 12.0, theme::TEXT_2));
            }
            ui.add_space(8.0);
            widgets::rule(ui);
            ui.add_space(8.0);
        }

        coverage(ui, script, resolved);
        if !resolved.lost.is_empty() {
            ui.add_space(6.0);
            widgets::chip(
                ui,
                &format!(
                    "{} link(s) lost: their words were edited away",
                    resolved.lost.len()
                ),
                theme::DANGER,
            );
            for id in &resolved.lost {
                if let Some(l) = links.links.iter().find(|l| &l.id == id) {
                    let r = ui.add(
                        egui::Label::new(mono(
                            format!("#{} \"{}\"", l.id, l.quote),
                            theme::TEXT_MUTED,
                        ))
                        .sense(Sense::click()),
                    );
                    if r.clicked() {
                        self.selected_link = Some(l.id.clone());
                    }
                }
            }
        }
    }

    fn link(
        &mut self,
        project: &Project,
        script: &Script,
        links: &mut Links,
        sel: Selection,
        drag: &MediaDrag,
    ) {
        let requests = match link_requests(script, sel, &drag.clips) {
            Ok(requests) => requests,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        if requests.is_empty() {
            return;
        }
        match links.add_batch(project, requests) {
            Ok(_) => {
                self.error = None;
                self.interaction.selection = Some(sel);
                self.selected_link = None;
            }
            Err(e) => self.error = Some(format!("Couldn't link the clips: {e}")),
        }
    }
}

/// Wrap in quotes unless the words already start with one.
fn quoted(q: &str) -> String {
    if q.starts_with('"') {
        q.to_owned()
    } else {
        format!("\"{q}\"")
    }
}

fn quote_of(script: &Script, sel: Selection) -> Option<String> {
    sel.quote(script)
}

fn link_requests(
    script: &Script,
    selection: Selection,
    clips: &[MediaFile],
) -> Result<Vec<Link>, spearfia_core::CoreError> {
    let parts = selection.parts(script);
    if !parts
        .len()
        .checked_mul(clips.len())
        .is_some_and(|n| n <= spearfia_core::links::MAX_BATCH_LINKS)
    {
        return Err(spearfia_core::CoreError::Invalid(
            "Link at most 10,000 clip/line pairs at a time.".into(),
        ));
    }
    let mut requests = Vec::new();
    let mut bytes = 0usize;
    for part in parts {
        let Some(Block::Line { line, .. }) = script.blocks.get(part.block) else {
            continue;
        };
        let Some(quote) = part.quote(script) else {
            continue;
        };
        for clip in clips {
            bytes = bytes.checked_add(quote.len()).and_then(|n| n.checked_add(clip.path.len()))
                .filter(|n| *n <= spearfia_core::links::MAX_BATCH_BYTES)
                .ok_or_else(|| spearfia_core::CoreError::Invalid("The media link selection is too large. Link smaller groups of lines or clips.".into()))?;
            requests.push(Link {
                id: String::new(),
                quote: quote.clone(),
                line: *line,
                media: LinkMedia {
                    path: clip.path.clone(),
                    hash: None,
                },
                role: clip.role,
                kind: LinkKind::for_role(clip.role),
                in_point: None,
                out_point: None,
            });
        }
    }
    Ok(requests)
}

/// Files whose names share words with the quote (`barrier-hit_02` ↔ "into a barrier").
fn name_matches(quote: &str, tree: &[FolderNode]) -> Vec<(MediaFile, usize)> {
    let stop = [
        "a", "an", "the", "and", "of", "to", "in", "on", "you", "your", "into", "it", "is", "with",
    ];
    let words: HashSet<String> = quote
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.len() > 2 && !stop.contains(&w.as_str()))
        .map(|w| w.trim_end_matches('s').to_owned())
        .collect();
    let mut out: Vec<(MediaFile, usize)> = tree
        .iter()
        .flat_map(|n| n.all_files())
        .filter_map(|f| {
            let name = f
                .name
                .rsplit_once('.')
                .map_or(f.name.as_str(), |(n, _)| n)
                .to_lowercase();
            let score = name
                .split(|c: char| !c.is_alphanumeric())
                .map(|t| t.trim_end_matches('s'))
                .filter(|t| t.len() > 2 && words.contains(*t))
                .count();
            (score > 0).then(|| (f.clone(), score))
        })
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.name.cmp(&b.0.name)));
    out
}

/// Per section: how many lines have media.
fn coverage(ui: &mut egui::Ui, script: &Script, resolved: &Resolved) {
    ui.label(strong("Coverage", 12.5));
    let mut rows: Vec<(String, usize, usize)> = Vec::new();
    for (i, b) in script.blocks.iter().enumerate() {
        match b {
            Block::Heading { level, text, .. } if *level >= 2 => rows.push((text.clone(), 0, 0)),
            Block::Line { kind, .. } if *kind != spearfia_script::LineKind::Note => {
                if rows.is_empty() {
                    rows.push(("(before first section)".into(), 0, 0));
                }
                let r = rows.last_mut().expect("row");
                r.2 += 1;
                if resolved.spans.contains_key(&i) {
                    r.1 += 1;
                }
            }
            _ => {}
        }
    }
    for (title, covered, total) in rows {
        if total == 0 {
            continue;
        }
        ui.horizontal(|ui| {
            ui.label(text(title, 12.0, theme::TEXT_2));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(mono(
                    format!("{covered}/{total}"),
                    if covered == total {
                        theme::OK
                    } else {
                        theme::WARN
                    },
                ));
            });
        });
        let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 4.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, 2.0, theme::BORDER);
        let mut fill = r;
        fill.set_width(r.width() * covered as f32 / total as f32);
        p.rect_filled(fill, 2.0, theme::OK);
        ui.add_space(4.0);
    }
}

fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / f64::from(1u32 << 30)),
        b if b >= 1 << 20 => format!("{:.0} MB", b as f64 / f64::from(1u32 << 20)),
        b if b >= 1 << 10 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{b} B"),
    }
}

fn folder_icon(ui: &mut egui::Ui, color: egui::Color32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(15.0, 12.0), Sense::hover());
    let p = ui.painter();
    let tab = egui::Rect::from_min_size(r.min, egui::vec2(6.0, 3.0));
    p.rect_filled(tab, 1.0, color);
    p.rect_filled(
        egui::Rect::from_min_max(r.min + egui::vec2(0.0, 2.0), r.max),
        2.0,
        color,
    );
}

/// A stand-in thumbnail: role-tinted box with the file type. Real frames come with analysis.
fn thumb(ui: &mut egui::Ui, size: egui::Vec2, f: &MediaFile) {
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    let c = script_view::role_color(f.role);
    p.rect_filled(r, CornerRadius::same(3), theme::tint(c, 0.18));
    p.rect_stroke(
        r,
        CornerRadius::same(3),
        Stroke::new(1.0, theme::tint(c, 0.4)),
        egui::StrokeKind::Inside,
    );
    let label = match f.kind {
        MediaKind::Video => "▶",
        MediaKind::Audio => "♪",
        MediaKind::Image => "▢",
    };
    p.text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(size.y * 0.45),
        c,
    );
}

fn grid_card(ui: &mut egui::Ui, f: &MediaFile, w: f32, uses: usize) {
    ui.allocate_ui_with_layout(egui::vec2(w, 0.0), Layout::top_down(Align::Min), |ui| {
        egui::Frame::new()
            .fill(theme::BG_PANEL)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(8)
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(w - 16.0);
                thumb(ui, egui::vec2(w - 16.0, (w - 16.0) * 9.0 / 16.0), f);
                ui.add(egui::Label::new(text(&f.name, 12.0, theme::TEXT)).truncate());
                ui.horizontal(|ui| {
                    widgets::tag(ui, f.kind.label(), theme::TEXT_2);
                    widgets::tag(ui, f.role.label(), script_view::role_color(f.role));
                    ui.label(mono(size(f.bytes), theme::TEXT_MUTED));
                });
                ui.label(mono(
                    if uses > 0 {
                        format!("linked to {uses} line(s)")
                    } else {
                        "not linked".into()
                    },
                    if uses > 0 {
                        theme::CUE_CLIP
                    } else {
                        theme::TEXT_MUTED
                    },
                ));
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_matching() {
        let f = |name: &str| MediaFile {
            path: format!("B-roll/{name}"),
            name: name.into(),
            kind: MediaKind::Video,
            role: MediaRole::BRoll,
            bytes: 1,
        };
        let tree = vec![FolderNode {
            path: "B-roll".into(),
            name: "B-roll".into(),
            role: MediaRole::BRoll,
            exists: true,
            children: vec![],
            files: vec![
                f("barrier-hit_02.mp4"),
                f("clean-slide.mp4"),
                f("celebration.mp4"),
            ],
        }];
        let m = name_matches("you spinning into a barrier,", &tree);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].0.name, "barrier-hit_02.mp4");
        assert_eq!(name_matches("a clean slide,", &tree)[0].1, 2);
    }

    #[test]
    fn sizes() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(3 << 20), "3 MB");
    }
    #[test]
    fn two_clips_cover_every_selected_line_without_changing_the_script() {
        let source = "## A\none two\n\nthree four\n> note\n## B\nfive six\n";
        let script = spearfia_script::parse(source);
        let selection = Selection::section(&script, 0).unwrap();
        let clips: Vec<_> = ["a.mp4", "b.mp4"]
            .iter()
            .map(|name| MediaFile {
                path: format!("B-roll/{name}"),
                name: (*name).into(),
                kind: MediaKind::Video,
                role: MediaRole::BRoll,
                bytes: 0,
            })
            .collect();
        let requests = link_requests(&script, selection, &clips).unwrap();
        assert_eq!(requests.len(), 4);
        assert_eq!(
            requests.iter().map(|l| l.line).collect::<Vec<_>>(),
            [1, 1, 3, 3]
        );
        let links = Links {
            links: requests
                .into_iter()
                .enumerate()
                .map(|(i, mut l)| {
                    l.id = format!("k{i}");
                    l
                })
                .collect(),
            ..Default::default()
        };
        let resolved = script_view::resolve(&script, &links);
        assert!(resolved.lost.is_empty());
        assert!(resolved.spans.values().all(|spans| spans.len() == 2));
        assert_eq!(script, spearfia_script::parse(source));
    }

    fn media_project() -> Project {
        let root = std::env::temp_dir().join(format!(
            "spearfia-sidebar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("B-roll/Scene")).unwrap();
        std::fs::create_dir(root.join("Graphics")).unwrap();
        let project = Project { root, file: serde_json::from_value(serde_json::json!({
            "name":"Sidebar", "spec_version":1, "adapter":"filmcraft",
            "media_folders":[{"path":"B-roll","role":"b-roll"},{"path":"Graphics","role":"graphics"}]
        })).unwrap() };
        project.save().unwrap();
        project
    }

    fn sidebar_frame(
        ctx: &egui::Context,
        stage: &mut MediaStage,
        project: &mut Project,
        input: egui::RawInput,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 850.0),
                )),
                ..input
            },
            |ui| stage.folders_panel(ui, project, &Links::default()),
        );
        output.textures_delta.clear();
        output
    }

    fn mouse(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn folder_drag_reaches_the_hovered_folder_and_clip_payload_survives_type_checks() {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let mut project = media_project();
        let mut stage = MediaStage::new(&project);
        sidebar_frame(&ctx, &mut stage, &mut project, egui::RawInput::default());
        let source = ctx
            .read_response(egui::Id::new(("media-folder", "B-roll/Scene")))
            .unwrap()
            .rect
            .center();
        let target = ctx
            .read_response(egui::Id::new(("media-folder", "Graphics")))
            .unwrap()
            .rect
            .center();
        for events in [
            vec![egui::Event::PointerMoved(source), mouse(source, true)],
            vec![egui::Event::PointerMoved(source + egui::vec2(12.0, 0.0))],
            vec![egui::Event::PointerMoved(target)],
            vec![],
        ] {
            sidebar_frame(
                &ctx,
                &mut stage,
                &mut project,
                egui::RawInput {
                    events,
                    ..Default::default()
                },
            );
        }
        assert!(egui::DragAndDrop::has_payload_of_type::<FolderDrag>(&ctx));
        sidebar_frame(
            &ctx,
            &mut stage,
            &mut project,
            egui::RawInput {
                events: vec![mouse(target, false)],
                ..Default::default()
            },
        );
        assert!(
            matches!(stage.folder_action.take(), Some(FolderAction::Move(parent, paths)) if parent == "Graphics" && paths == ["B-roll/Scene"])
        );

        // Checking a folder payload must not consume a clip payload of another type.
        egui::DragAndDrop::set_payload(
            &ctx,
            MediaDrag {
                clips: vec![MediaFile {
                    path: "B-roll/a.mp4".into(),
                    name: "a.mp4".into(),
                    kind: MediaKind::Video,
                    role: MediaRole::BRoll,
                    bytes: 0,
                }],
            },
        );
        sidebar_frame(
            &ctx,
            &mut stage,
            &mut project,
            egui::RawInput {
                events: vec![mouse(target, true)],
                ..Default::default()
            },
        );
        sidebar_frame(
            &ctx,
            &mut stage,
            &mut project,
            egui::RawInput {
                events: vec![mouse(target, false)],
                ..Default::default()
            },
        );
        assert!(
            matches!(stage.folder_action.take(), Some(FolderAction::Move(parent, paths)) if parent == "Graphics" && paths == ["B-roll/a.mp4"])
        );
        std::fs::remove_dir_all(&project.root).unwrap();
    }

    #[derive(Debug)]
    struct Dropped(PathBuf);
    impl egui::DroppedFile for Dropped {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn explorer_drop_targets_native_cursor_even_when_egui_pointer_is_missing() {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let mut project = media_project();
        let mut stage = MediaStage::new(&project);
        sidebar_frame(&ctx, &mut stage, &mut project, egui::RawInput::default());
        let target = ctx
            .read_response(egui::Id::new(("media-folder", "Graphics")))
            .unwrap()
            .rect
            .center();
        stage.drop_pointer = Some(target);
        let path = std::env::temp_dir().join("explorer-clip.mp4");
        let output = sidebar_frame(
            &ctx,
            &mut stage,
            &mut project,
            egui::RawInput {
                hovered_files: vec![egui::HoveredFile {
                    path: Some(path.clone()),
                    ..Default::default()
                }],
                events: vec![egui::Event::PointerGone],
                focused: false,
                ..Default::default()
            },
        );
        assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.stroke.width == 1.5 && r.stroke.color == theme::ACCENT)));
        sidebar_frame(
            &ctx,
            &mut stage,
            &mut project,
            egui::RawInput {
                dropped_files: vec![std::sync::Arc::new(Dropped(path.clone()))],
                focused: false,
                ..Default::default()
            },
        );
        assert!(
            matches!(stage.folder_action.take(), Some(FolderAction::Import(parent, paths)) if parent == "Graphics" && paths == [path])
        );
        std::fs::remove_dir_all(&project.root).unwrap();
    }
}
