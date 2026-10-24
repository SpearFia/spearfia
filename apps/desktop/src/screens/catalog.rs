//! Catalog tab: browse every asset (transitions, effects, music, SFX, templates), search,
//! see full metadata, and add source folders.

use crate::app::App;
use crate::theme;
use crate::widgets::{self, mono, muted, panel, strong, text};
use egui::{Align, Layout, Margin, Panel, RichText, Sense};
use spearfia_catalog::{AssetKind, Entry, LicenseStatus, Query};
use spearfia_core::app_state::CatalogSourceSetting;

#[derive(Debug, Default)]
pub struct CatalogView {
    pub kind: Option<AssetKind>,
    pub search: String,
    pub open_only: bool,
    /// Rank by this preset's weights.
    pub vision: Option<String>,
    pub show_visions: bool,
    pub visions: super::vision_catalog::VisionView,
    /// Asset id of the selected entry.
    pub selected: Option<String>,
    pub show_json: bool,
}

fn kind_color(k: AssetKind) -> egui::Color32 {
    match k {
        AssetKind::Transition => theme::CUE_TRANSITION,
        AssetKind::Effect | AssetKind::Recipe | AssetKind::Lut => theme::CUE_LOOK,
        AssetKind::Music => theme::CUE_MUSIC,
        AssetKind::Sfx => theme::CUE_SFX,
        AssetKind::Template | AssetKind::Preset | AssetKind::Font => theme::CUE_VISION,
        AssetKind::Broll => theme::CUE_CLIP,
    }
}

fn license_color(s: LicenseStatus) -> egui::Color32 {
    match s {
        LicenseStatus::Open => theme::OK,
        LicenseStatus::Check => theme::WARN,
        LicenseStatus::Unknown => theme::DANGER,
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    Panel::left("catalog-sidebar")
        .exact_size(250.0)
        .frame(panel(theme::BG_PANEL).inner_margin(Margin::symmetric(10, 14)))
        .show(ui, |ui| sidebar(app, ui));
    if app.catalog_view.show_visions {
        Panel::right("vision-inspector")
            .exact_size(440.0)
            .frame(panel(theme::BG_PANEL).inner_margin(Margin::same(16)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("vision-inspector-scroll")
                    .show(ui, |ui| super::vision_catalog::inspector(app, ui));
            });
        egui::CentralPanel::default()
            .frame(panel(theme::BG_APP).inner_margin(Margin::symmetric(20, 16)))
            .show(ui, |ui| super::vision_catalog::list(app, ui));
        return;
    }
    Panel::right("catalog-inspector")
        .exact_size(370.0)
        .frame(panel(theme::BG_PANEL).inner_margin(Margin::same(16)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| inspector(app, ui));
        });
    egui::CentralPanel::default()
        .frame(panel(theme::BG_APP).inner_margin(Margin::symmetric(20, 16)))
        .show(ui, |ui| list(app, ui));
}

fn section_label(ui: &mut egui::Ui, s: &str) {
    ui.add_space(10.0);
    ui.label(mono(s, theme::TEXT_MUTED));
    ui.add_space(2.0);
}

fn sidebar(app: &mut App, ui: &mut egui::Ui) {
    section_label(ui, "KIND");
    let total = app.catalog.entries.len();
    let mut pick = |ui: &mut egui::Ui,
                    kind: Option<AssetKind>,
                    label: &str,
                    n: usize,
                    color,
                    vision_page: bool| {
        let on = app.catalog_view.show_visions == vision_page
            && (vision_page || app.catalog_view.kind == kind);
        let r = ui
            .horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 16.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, color);
                ui.label(text(
                    label,
                    12.5,
                    if on { theme::TEXT } else { theme::TEXT_2 },
                ));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(mono(n.to_string(), theme::TEXT_MUTED));
                });
            })
            .response
            .interact(Sense::click());
        if on {
            ui.painter()
                .rect_filled(r.rect.expand(3.0), 5.0, theme::tint(theme::TEXT, 0.06));
        }
        if r.clicked() {
            app.catalog_view.kind = kind;
            app.catalog_view.show_visions = vision_page;
        }
    };
    pick(ui, None, "All assets", total, theme::TEXT_MUTED, false);
    for k in AssetKind::ALL {
        let n = app.catalog.count(k);
        if n > 0
            || matches!(
                k,
                AssetKind::Transition
                    | AssetKind::Effect
                    | AssetKind::Music
                    | AssetKind::Sfx
                    | AssetKind::Template
            )
        {
            pick(ui, Some(k), k.label(), n, kind_color(k), false);
        }
    }

    pick(
        ui,
        None,
        "Vision",
        app.visions.len(),
        theme::CUE_VISION,
        true,
    );

    section_label(ui, "SOURCES");
    let mut changed = false;
    let mut remove = None;
    for (i, s) in app.settings.catalog_sources.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut s.enabled, "").changed();
            ui.vertical(|ui| {
                ui.label(text(
                    &s.name,
                    12.5,
                    if s.enabled {
                        theme::TEXT
                    } else {
                        theme::TEXT_MUTED
                    },
                ));
                ui.label(mono(s.path.display().to_string(), theme::TEXT_MUTED));
            });
        })
        .response
        .context_menu(|ui| {
            if ui.button("Remove source").clicked() {
                remove = Some(i);
                ui.close();
            }
        });
    }
    if let Some(i) = remove {
        app.settings.catalog_sources.remove(i);
        changed = true;
    }
    if app.settings.catalog_sources.is_empty() {
        ui.label(muted("No sources yet."));
    }
    ui.add_space(4.0);
    if ui
        .add(egui::Button::new(text("+ Add source folder", 12.0, theme::ACCENT)).frame(false))
        .clicked()
        && let Some(dir) = rfd::FileDialog::new().pick_folder()
    {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.display().to_string());
        app.settings.catalog_sources.push(CatalogSourceSetting {
            name,
            path: dir,
            enabled: true,
        });
        changed = true;
    }
    if changed {
        app.save_state();
        app.reload_catalog();
    }

    if !app.catalog.errors.is_empty() {
        ui.add_space(10.0);
        let n = app.catalog.errors.len();
        let tip: String = app
            .catalog
            .errors
            .iter()
            .map(|e| format!("{}: {}\n", e.path.display(), e.message))
            .collect();
        widgets::chip(ui, &format!("{n} file(s) couldn't be read"), theme::DANGER)
            .on_hover_text(tip);
    }

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        if widgets::button(ui, "Reload").clicked() {
            app.reload_catalog();
        }
        ui.label(muted(
            "Each asset is a folder with its file and an <id>.asset.json descriptor.",
        ));
    });
}

fn current_query(app: &App) -> Query {
    Query {
        text: app.catalog_view.search.clone(),
        kind: app.catalog_view.kind,
        only_open_licenses: app.catalog_view.open_only,
        weights: app
            .catalog_view
            .vision
            .as_deref()
            .and_then(|name| app.visions.iter().find(|v| v.name() == name))
            .map(|p| p.weight_map())
            .unwrap_or_default(),
    }
}

fn list(app: &mut App, ui: &mut egui::Ui) {
    let q = current_query(app);
    let hits: Vec<Entry> = app.catalog.search(&q).into_iter().cloned().collect();

    ui.horizontal(|ui| {
        let title = app.catalog_view.kind.map_or("All assets", AssetKind::label);
        ui.label(strong(title, 15.0));
        ui.label(mono(format!("{} shown", hits.len()), theme::TEXT_MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut app.catalog_view.search)
                    .hint_text("Search names, tags, summaries")
                    .desired_width(280.0),
            );
        });
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.catalog_view.open_only, "Open licenses only");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = app.catalog_view.vision.as_deref().unwrap_or("none");
            egui::ComboBox::from_id_salt("rank-vision")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.catalog_view.vision, None, "none");
                    for p in &app.visions {
                        ui.selectable_value(
                            &mut app.catalog_view.vision,
                            Some(p.name().to_owned()),
                            p.title(),
                        );
                    }
                });
            ui.label(muted("rank by vision"));
        });
    });
    ui.add_space(10.0);

    if app.catalog.entries.is_empty() {
        widgets::card(theme::BG_PANEL, theme::BORDER).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(strong("The catalog is empty", 14.0));
            ui.label(muted(
                "Add a source folder on the left (for example C:\\dev\\spearfia\\plugins or \
                 starter-pack). Spear Fia reads every *.asset.json inside it.",
            ));
        });
        return;
    }

    let weights = q.weights.clone();
    egui::Frame::new()
        .fill(theme::BG_PANEL)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("catalog-table")
                        .num_columns(6)
                        .spacing([14.0, 10.0])
                        .striped(false)
                        .show(ui, |ui| {
                            for h in ["NAME", "KIND", "TAGS", "LICENSE", "SOURCE", "FIT"] {
                                ui.label(mono(h, theme::TEXT_MUTED));
                            }
                            ui.end_row();
                            for e in &hits {
                                let a = &e.asset;
                                let sel =
                                    app.catalog_view.selected.as_deref() == Some(a.id.as_str());
                                let name = ui.add(
                                    egui::Label::new(text(
                                        &a.name,
                                        12.5,
                                        if sel { theme::ACCENT } else { theme::TEXT },
                                    ))
                                    .sense(Sense::click()),
                                );
                                if name.clicked() {
                                    app.catalog_view.selected = Some(a.id.clone());
                                }
                                widgets::tag(ui, a.kind.key(), kind_color(a.kind));
                                ui.horizontal(|ui| {
                                    for t in a.tags.iter().take(3) {
                                        widgets::tag(ui, t, theme::TEXT_2);
                                    }
                                    if a.tags.len() > 3 {
                                        ui.label(mono(
                                            format!("+{}", a.tags.len() - 3),
                                            theme::TEXT_MUTED,
                                        ));
                                    }
                                });
                                widgets::tag(ui, &a.license, license_color(a.license_status()));
                                ui.label(mono(&e.source, theme::TEXT_2));
                                let fit = a.fit(&weights);
                                ui.label(mono(
                                    if weights.is_empty() {
                                        "–".to_owned()
                                    } else {
                                        format!("{fit:+.1}")
                                    },
                                    if fit > 0.0 {
                                        theme::OK
                                    } else if fit < 0.0 {
                                        theme::DANGER
                                    } else {
                                        theme::TEXT_MUTED
                                    },
                                ));
                                ui.end_row();
                            }
                        });
                });
        });
}

fn inspector(app: &mut App, ui: &mut egui::Ui) {
    let Some(entry) = app
        .catalog_view
        .selected
        .as_ref()
        .and_then(|id| app.catalog.entries.iter().find(|e| &e.asset.id == id))
        .cloned()
    else {
        ui.label(muted("Select an asset to see its metadata."));
        return;
    };
    let a = &entry.asset;
    ui.horizontal(|ui| {
        ui.label(strong(&a.name, 15.0));
        ui.add_space((ui.available_width() - 150.0).max(0.0));
        widgets::segmented(
            ui,
            &mut app.catalog_view.show_json,
            &[(false, "Form"), (true, "asset.json")],
        );
    });
    ui.add_space(6.0);

    if app.catalog_view.show_json {
        let json = std::fs::read_to_string(&entry.descriptor).unwrap_or_else(|e| e.to_string());
        egui::Frame::new()
            .fill(theme::BG_INSET)
            .corner_radius(6)
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.add(egui::Label::new(mono(json, theme::TEXT_2)).wrap());
            });
        return;
    }

    if let Some(s) = &a.summary {
        ui.label(text(s, 12.5, theme::TEXT_2));
    }
    heading(ui, "IDENTITY");
    widgets::kv(ui, "id", mono(&a.id, theme::TEXT));
    ui.horizontal(|ui| {
        ui.add_sized(
            [92.0, 16.0],
            egui::Label::new(mono("kind", theme::TEXT_MUTED)),
        );
        widgets::tag(ui, a.kind.key(), kind_color(a.kind));
    });
    ui.horizontal(|ui| {
        ui.add_sized(
            [92.0, 16.0],
            egui::Label::new(mono("license", theme::TEXT_MUTED)),
        );
        let st = a.license_status();
        widgets::tag(ui, &a.license, license_color(st));
        if st != LicenseStatus::Open {
            ui.label(muted(match st {
                LicenseStatus::Check => "check the terms",
                _ => "keep out of published videos",
            }));
        }
    });
    let src = match (&a.source.url, &a.source.author) {
        (Some(u), Some(au)) => format!("{} · {u} · {au}", a.source.kind),
        (Some(u), None) => format!("{} · {u}", a.source.kind),
        (None, Some(au)) => format!("{} · {au}", a.source.kind),
        (None, None) => a.source.kind.clone(),
    };
    widgets::kv(ui, "source", text(src, 12.0, theme::TEXT_2));
    widgets::kv(ui, "catalog", text(&entry.source, 12.0, theme::TEXT_2));
    widgets::kv(ui, "hash", mono(short_hash(&a.hash), theme::TEXT_MUTED));

    heading(ui, "FOR THE AI");
    ui.label(mono("tags", theme::TEXT_MUTED));
    ui.horizontal_wrapped(|ui| {
        for t in &a.tags {
            widgets::tag(ui, t, theme::TEXT_2);
        }
        if a.tags.is_empty() {
            ui.label(muted(
                "None yet. Add tags so search and the AI can find it.",
            ));
        }
    });
    for c in &a.capabilities {
        egui::Frame::new()
            .fill(theme::BG_RAISED)
            .corner_radius(6)
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&c.id).monospace().color(theme::CUE_LOOK));
                ui.label(text(&c.summary, 12.0, theme::TEXT_2));
                if !c.inputs.is_empty() {
                    let names: Vec<&str> = c.inputs.keys().map(String::as_str).collect();
                    widgets::kv(ui, "inputs", mono(names.join(", "), theme::TEXT));
                }
                if !c.requires.is_empty() {
                    widgets::kv(ui, "requires", mono(c.requires.join(", "), theme::WARN));
                }
                for (k, v) in &c.binds {
                    widgets::kv(ui, "binds", mono(format!("{k} ← {v}"), theme::OK));
                }
            });
        ui.add_space(4.0);
    }
    if !a.hosts.is_empty() {
        let hosts: Vec<String> = a
            .hosts
            .iter()
            .filter_map(|h| {
                Some(format!(
                    "{} ({})",
                    h.get("host")?.as_str()?,
                    h.get("format")?.as_str()?
                ))
            })
            .collect();
        widgets::kv(ui, "runs in", text(hosts.join(", "), 12.0, theme::TEXT_2));
    }

    heading(ui, "VISION FIT");
    for p in &app.visions {
        let fit = a.fit(&p.weight_map());
        ui.horizontal(|ui| {
            ui.label(text(p.title(), 12.0, theme::TEXT_2));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(mono(
                    format!("{fit:+.1}"),
                    if fit > 0.0 {
                        theme::OK
                    } else if fit < 0.0 {
                        theme::DANGER
                    } else {
                        theme::TEXT_MUTED
                    },
                ));
            });
        });
    }

    heading(ui, "FILE");
    widgets::kv(
        ui,
        "descriptor",
        text(entry.descriptor.display().to_string(), 11.5, theme::TEXT_2),
    );
    ui.add_space(8.0);
    if widgets::button(ui, "Show in folder").clicked()
        && let Some(dir) = entry.descriptor.parent()
    {
        crate::reveal(dir);
    }
}

fn heading(ui: &mut egui::Ui, s: &str) {
    ui.add_space(10.0);
    widgets::rule(ui);
    ui.add_space(6.0);
    ui.label(mono(s, theme::TEXT_MUTED));
}

fn short_hash(h: &str) -> String {
    match h.strip_prefix("sha256:") {
        Some(hex) if hex.len() > 12 => format!("sha256:{}…{}", &hex[..6], &hex[hex.len() - 4..]),
        _ => h.to_owned(),
    }
}
