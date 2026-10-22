//! Projects tab: recent projects as cards, an info panel, and open / new actions.

use crate::app::App;
use crate::theme;
use crate::widgets::{self, mono, muted, panel, strong, text};
use egui::{Align, CornerRadius, Layout, Margin, Panel, Sense, Stroke, Vec2};
use spearfia_core::project::{Project, ProjectFile};
use std::path::{Path, PathBuf};

const CARD_W: f32 = 260.0;

/// What a card shows, read once from the project folder.
#[derive(Debug, Clone)]
pub struct CardInfo {
    pub file: ProjectFile,
    pub vision: Option<String>,
    pub sections: usize,
    pub est_seconds: f32,
}

impl CardInfo {
    fn load(path: &Path) -> Option<Self> {
        let p = Project::open(path).ok()?;
        let script = p
            .read_script()
            .map(|s| spearfia_script::parse(&s))
            .unwrap_or_default();
        Some(Self {
            vision: p.vision_preset(),
            sections: script.outline().len(),
            est_seconds: script.est_seconds(),
            file: p.file,
        })
    }
}

fn info<'a>(app: &'a mut App, path: &Path) -> Option<&'a CardInfo> {
    app.cards
        .entry(path.to_path_buf())
        .or_insert_with(|| CardInfo::load(path))
        .as_ref()
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    Panel::top("projects-toolbar")
        .exact_size(46.0)
        .frame(panel(theme::BG_APP).inner_margin(Margin::symmetric(20, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(strong("Projects", 13.5));
                ui.label(muted(format!("{} recent", app.recents.projects.len())));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.project_search)
                            .hint_text("Search projects")
                            .desired_width(240.0),
                    );
                });
            });
        });

    Panel::bottom("projects-footer")
        .exact_size(52.0)
        .frame(panel(theme::BG_PANEL).inner_margin(Margin::symmetric(20, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                if widgets::button(ui, "Open project folder…").clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    app.open_project(&dir);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let selected = app
                        .selected
                        .and_then(|i| app.recents.projects.get(i))
                        .map(|r| r.path.clone());
                    let open = ui.add_enabled(
                        selected.is_some(),
                        egui::Button::new(text("Open", 12.5, theme::ACCENT_ON).strong())
                            .fill(theme::ACCENT),
                    );
                    if open.clicked()
                        && let Some(p) = selected
                    {
                        app.open_project(&p);
                    }
                    if widgets::button(ui, "New Project").clicked() {
                        app.start_new_project();
                    }
                });
            });
        });

    Panel::right("projects-info")
        .exact_size(340.0)
        .frame(panel(theme::BG_PANEL).inner_margin(Margin::same(18)))
        .show(ui, |ui| info_panel(app, ui));

    egui::CentralPanel::default()
        .frame(panel(theme::BG_APP).inner_margin(Margin::same(24)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| grid(app, ui));
        });
}

fn grid(app: &mut App, ui: &mut egui::Ui) {
    let query = app.project_search.to_lowercase();
    let rows: Vec<(usize, PathBuf, String, u64)> = app
        .recents
        .projects
        .iter()
        .enumerate()
        .filter(|(_, r)| query.is_empty() || r.name.to_lowercase().contains(&query))
        .map(|(i, r)| (i, r.path.clone(), r.name.clone(), r.last_opened))
        .collect();

    let gap = 20.0;
    let per_row = (((ui.available_width() + gap) / (CARD_W + gap)).floor() as usize).max(1);
    // Slot 0 is the New Project card; the rest are projects.
    let mut slots: Vec<Option<(usize, PathBuf, String, u64)>> = vec![None];
    slots.extend(rows.into_iter().map(Some));
    ui.spacing_mut().item_spacing = Vec2::splat(gap);
    for chunk in slots.chunks(per_row) {
        ui.horizontal_top(|ui| {
            for slot in chunk {
                match slot {
                    None => {
                        if new_card(ui).clicked() {
                            app.start_new_project();
                        }
                    }
                    Some((i, path, name, opened)) => {
                        let ago = widgets::ago(*opened, app.now);
                        let selected = app.selected == Some(*i);
                        let info = info(app, path).cloned();
                        let r = project_card(ui, name, &ago, info.as_ref(), selected);
                        if r.clicked() {
                            app.selected = Some(*i);
                        }
                        if r.double_clicked() {
                            app.open_project(path);
                        }
                    }
                }
            }
        });
    }
    if app.recents.projects.is_empty() {
        ui.add_space(12.0);
        ui.label(muted(
            "No projects yet. Create one, or open an existing project folder.",
        ));
    }
}

fn new_card(ui: &mut egui::Ui) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(CARD_W, 250.0), Sense::click());
    let hover = r.hovered();
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(8),
        if hover {
            theme::BG_PANEL
        } else {
            theme::BG_APP
        },
        Stroke::new(1.0, theme::BORDER_STRONG),
        egui::StrokeKind::Inside,
    );
    let c = rect.center();
    let plus = egui::Rect::from_center_size(c - egui::vec2(0.0, 22.0), egui::vec2(56.0, 56.0));
    p.rect_filled(plus, CornerRadius::same(12), theme::BG_RAISED);
    p.text(
        plus.center(),
        egui::Align2::CENTER_CENTER,
        "+",
        egui::FontId::proportional(28.0),
        theme::ACCENT,
    );
    p.text(
        c + egui::vec2(0.0, 26.0),
        egui::Align2::CENTER_CENTER,
        "New Project",
        egui::FontId::proportional(14.0),
        theme::TEXT,
    );
    p.text(
        c + egui::vec2(0.0, 46.0),
        egui::Align2::CENTER_CENTER,
        "vision, folders, FilmCraft",
        egui::FontId::proportional(11.5),
        theme::TEXT_MUTED,
    );
    r
}

/// A stand-in thumbnail: a tinted gradient with two diagonal slashes, seeded by name.
fn thumbnail(ui: &egui::Ui, rect: egui::Rect, seed: &str) {
    let h = seed
        .bytes()
        .fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(u32::from(b)));
    let palettes = [
        (0x73, 0x47, 0x2E),
        (0x4C, 0x5C, 0x70),
        (0x40, 0x52, 0x38),
        (0x5C, 0x38, 0x4C),
    ];
    let (r, g, b) = palettes[(h as usize) % palettes.len()];
    let p = ui.painter().with_clip_rect(rect);
    let top = CornerRadius {
        nw: 8,
        ne: 8,
        sw: 0,
        se: 0,
    };
    p.rect_filled(rect, top, egui::Color32::from_rgb(r, g, b));
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), egui::Color32::TRANSPARENT);
    mesh.colored_vertex(rect.right_top(), egui::Color32::TRANSPARENT);
    mesh.colored_vertex(rect.right_bottom(), egui::Color32::from_black_alpha(200));
    mesh.colored_vertex(rect.left_bottom(), egui::Color32::from_black_alpha(140));
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(mesh);
    for f in [0.34, 0.62] {
        let x = rect.left() + rect.width() * f;
        p.line_segment(
            [
                egui::pos2(x + 40.0, rect.top()),
                egui::pos2(x - 40.0, rect.bottom()),
            ],
            Stroke::new(4.0, egui::Color32::from_white_alpha(170)),
        );
    }
}

fn project_card(
    ui: &mut egui::Ui,
    name: &str,
    ago: &str,
    info: Option<&CardInfo>,
    selected: bool,
) -> egui::Response {
    let stroke = if selected {
        theme::ACCENT
    } else {
        theme::BORDER
    };
    let r = ui
        .allocate_ui_with_layout(
            egui::vec2(CARD_W, 0.0),
            Layout::top_down(Align::Min),
            |ui| {
                egui::Frame::new()
                    .fill(theme::BG_PANEL)
                    .stroke(Stroke::new(if selected { 1.5 } else { 1.0 }, stroke))
                    .corner_radius(CornerRadius::same(8))
                    .show(ui, |ui| {
                        ui.set_width(CARD_W);
                        let (thumb, _) =
                            ui.allocate_exact_size(egui::vec2(CARD_W, 146.0), Sense::hover());
                        thumbnail(ui, thumb, name);
                        if let Some(i) = info {
                            let fmt = format!(
                                "{} · {}",
                                match i.file.format {
                                    spearfia_core::project::Format::Short => "9:16",
                                    spearfia_core::project::Format::LongForm => "16:9",
                                    spearfia_core::project::Format::Square => "1:1",
                                },
                                i.file.settings.timebase.label()
                            );
                            badge(ui, thumb.left_top() + egui::vec2(8.0, 8.0), &fmt, None);
                            if i.file.filmcraft.is_some() {
                                let w = 82.0;
                                badge(
                                    ui,
                                    thumb.right_top() + egui::vec2(-w - 8.0, 8.0),
                                    "FilmCraft",
                                    Some(theme::OK),
                                );
                            }
                        }
                        egui::Frame::new()
                            .inner_margin(Margin::same(12))
                            .show(ui, |ui| {
                                ui.set_width(CARD_W - 24.0);
                                ui.label(strong(name, 14.0));
                                ui.horizontal(|ui| {
                                    match info.and_then(|i| i.vision.as_deref()) {
                                        Some(v) => {
                                            widgets::tag(ui, v, theme::CUE_VISION);
                                        }
                                        None if info.is_none() => {
                                            widgets::tag(ui, "folder missing", theme::DANGER);
                                        }
                                        None => {}
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(mono(ago, theme::TEXT_MUTED));
                                    });
                                });
                                if let Some(i) = info {
                                    ui.label(mono(
                                        format!(
                                            "{} sections · {} est",
                                            i.sections,
                                            widgets::mmss(i.est_seconds)
                                        ),
                                        theme::TEXT_2,
                                    ));
                                }
                            });
                    })
                    .response
            },
        )
        .inner;
    r.interact(Sense::click())
}

fn badge(ui: &egui::Ui, pos: egui::Pos2, label: &str, dot: Option<egui::Color32>) {
    let font = egui::FontId::monospace(10.5);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, theme::TEXT);
    let pad = egui::vec2(6.0, 3.0);
    let dot_w = if dot.is_some() { 10.0 } else { 0.0 };
    let rect = egui::Rect::from_min_size(pos, galley.size() + pad * 2.0 + egui::vec2(dot_w, 0.0));
    let p = ui.painter();
    p.rect_filled(
        rect,
        CornerRadius::same(4),
        egui::Color32::from_black_alpha(170),
    );
    if let Some(c) = dot {
        p.circle_filled(rect.left_center() + egui::vec2(9.0, 0.0), 3.0, c);
    }
    p.galley(rect.min + pad + egui::vec2(dot_w, 0.0), galley, theme::TEXT);
}

fn info_panel(app: &mut App, ui: &mut egui::Ui) {
    let Some(idx) = app.selected.filter(|i| *i < app.recents.projects.len()) else {
        ui.label(muted("Select a project to see its details."));
        return;
    };
    let recent = app.recents.projects[idx].clone();
    let info = info(app, &recent.path).cloned();

    let (thumb, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 170.0), Sense::hover());
    thumbnail(ui, thumb, &recent.name);
    ui.add_space(8.0);
    ui.label(strong(&recent.name, 16.0));
    let Some(i) = info else {
        ui.label(text(
            format!(
                "{} can't be read. It may have moved.",
                recent.path.display()
            ),
            12.0,
            theme::DANGER,
        ));
        if widgets::button(ui, "Remove from list").clicked() {
            app.recents.remove(&recent.path);
            app.selected = None;
            app.save_state();
        }
        return;
    };
    if let Some(s) = &i.file.series {
        ui.label(muted(s.clone()));
    }
    ui.add_space(6.0);
    let f = &i.file;
    widgets::kv(
        ui,
        "format",
        text(
            format!(
                "{} · {}×{} · {}",
                f.format.label(),
                f.settings.resolution[0],
                f.settings.resolution[1],
                f.settings.timebase.label()
            ),
            12.0,
            theme::TEXT,
        ),
    );
    widgets::kv(
        ui,
        "vision",
        text(
            i.vision.clone().unwrap_or_else(|| "none".into()),
            12.0,
            theme::CUE_VISION,
        ),
    );
    let target = f
        .target_runtime_seconds
        .map(|t| format!(" of {} target", widgets::mmss(t as f32)))
        .unwrap_or_default();
    widgets::kv(
        ui,
        "runtime",
        text(
            format!("{} est{target}", widgets::mmss(i.est_seconds)),
            12.0,
            theme::TEXT,
        ),
    );
    widgets::kv(
        ui,
        "folder",
        text(recent.path.display().to_string(), 12.0, theme::TEXT_2),
    );
    match &f.filmcraft {
        Some(fc) => widgets::kv(
            ui,
            "FilmCraft",
            text(
                format!(
                    "{} · \"{}\"{}",
                    fc.project_file,
                    fc.sequence,
                    if fc.created {
                        ""
                    } else {
                        " · made on first push"
                    }
                ),
                12.0,
                theme::OK,
            ),
        ),
        None => widgets::kv(ui, "FilmCraft", text("not linked", 12.0, theme::TEXT_MUTED)),
    }
    ui.add_space(8.0);
    widgets::rule(ui);
    ui.add_space(8.0);
    ui.label(strong("Media folders", 12.5));
    for m in &f.media_folders {
        ui.horizontal(|ui| {
            ui.label(mono(&m.path, theme::TEXT_2));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::tag(ui, m.role.label(), theme::TEXT_2);
            });
        });
    }
    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.horizontal(|ui| {
            if widgets::primary_button(ui, "  Open  ").clicked() {
                app.open_project(&recent.path);
            }
            if widgets::button(ui, "Remove from list").clicked() {
                app.recents.remove(&recent.path);
                app.cards.remove(&recent.path);
                app.selected = None;
                app.save_state();
            }
        });
    });
}
