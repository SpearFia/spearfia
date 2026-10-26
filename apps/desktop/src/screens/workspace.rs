//! An open project: top bar, the workflow bar along the bottom (like Resolve's page bar),
//! and the current stage in the middle.

use crate::app::{App, View};
use crate::screens::{
    effects_stage::EffectsStage,
    home,
    media_stage::MediaStage,
    music_stage::MusicStage,
    script_stage::{ScriptEvent, ScriptStage},
    sfx_stage::SfxStage,
    transitions_stage::TransitionsStage,
};
use crate::theme;
use crate::widgets::{self, muted, panel, strong, text};
use egui::{Align, Align2, CornerRadius, FontId, Layout, Margin, Panel, Sense, Stroke};
use spearfia_core::links::Links;
use spearfia_core::project::Project;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Script,
    VisualScript,
    Media,
    Transitions,
    Effects,
    Music,
    SoundEffects,
    Sync,
}

impl Stage {
    pub const ALL: [Self; 8] = [
        Self::Script,
        Self::VisualScript,
        Self::Media,
        Self::Transitions,
        Self::Effects,
        Self::Music,
        Self::SoundEffects,
        Self::Sync,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Script => "Script",
            Self::VisualScript => "Visual Script",
            Self::Media => "Media",
            Self::Effects => "Effects",
            Self::Music => "Music",
            Self::Transitions => "Transitions",
            Self::SoundEffects => "Sound effects",
            Self::Sync => "Sync",
        }
    }

    /// What the stage will do, shown until it is built.
    fn plan(self) -> &'static str {
        match self {
            Self::Script | Self::VisualScript => "",
            Self::Media => {
                "Link the project folders (A-roll, B-roll, Screen, Graphics), browse clips in a grid, \
                or switch to the script view: highlight words, keep the highlight, and drop a clip on it."
            }
            Self::Effects => {
                "Each line's looks and effects, a preview rendered by FilmCraft, and the settings \
                of the selected effect, bound to analysis such as face tracking."
            }
            Self::Music => {
                "The script laid out over time with music bars showing what each track covers, \
                ducking under VO, gaps, and sync points like drop ↔ hard cut."
            }
            Self::Transitions => {
                "Every cut between lines and sections: the outgoing and incoming shot, the \
                transition, and where it came from (script divider, AI suggestion or default cut)."
            }
            Self::SoundEffects => {
                "Every :sfx cue on the timeline over the script: stingers on words, whooshes on \
                transitions and impacts on cuts, with sounds from the catalog to drag in and \
                checks for clashes with music hits and VO."
            }
            Self::Sync => {
                "Push the script to FilmCraft as a timeline plan, and pull FilmCraft edits back \
                with a three-way merge by cue ID."
            }
        }
    }
}

pub struct Workspace {
    pub project: Project,
    pub stage: Stage,
    pub script: ScriptStage,
    pub visuals: super::visual_stage::VisualStage,
    pub document_error: Option<String>,
    pub media: MediaStage,
    pub music: MusicStage,
    pub transitions: TransitionsStage,
    pub effects: EffectsStage,
    pub sfx: SfxStage,
    pub links: Links,
    pub channel: crate::screens::channels::ProjectChannelView,
    /// Shown in the top bar if `links.json` can't be read.
    pub links_error: Option<String>,
}

impl Workspace {
    pub fn new(project: Project) -> Self {
        let script = ScriptStage::load(&project);
        let media = MediaStage::new(&project);
        let visuals = super::visual_stage::VisualStage::new(&project);
        let (links, links_error) = match Links::load(&project) {
            Ok(l) => (l, None),
            Err(e) => (Links::default(), Some(e.to_string())),
        };
        Self {
            project,
            stage: Stage::Script,
            script,
            visuals,
            document_error: None,
            media,
            music: MusicStage::new(),
            transitions: TransitionsStage::new(),
            effects: EffectsStage::new(),
            sfx: SfxStage::new(),
            links,
            channel: crate::screens::channels::ProjectChannelView::default(),
            links_error,
        }
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let fc_found = app.filmcraft.is_some();
    let mut go_home = false;
    {
        let catalog = &app.catalog;
        let View::Workspace(ws) = &mut app.view else {
            return;
        };
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
        let now = ui.input(|i| i.time);
        ws.script.refresh_external(&ws.project, now, ui.ctx());
        ws.visuals
            .editor
            .refresh_external(&ws.project, now, ui.ctx());
        Panel::top("ws-top")
            .exact_size(44.0)
            .show_separator_line(false)
            .frame(panel(theme::BG_PANEL).inner_margin(Margin::symmetric(16, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let r = ui.scope(home::brand).response.interact(Sense::click());
                    if r.on_hover_text("Back to projects").clicked() {
                        go_home = true;
                    }
                    let f = &ws.project.file;
                    if let Some(s) = &f.series {
                        ui.label(muted("/"));
                        ui.label(text(s, 13.0, theme::TEXT));
                    }
                    ui.label(muted("/"));
                    ui.label(text(&f.name, 13.0, theme::TEXT_2));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (c, l) = match (&f.filmcraft, fc_found) {
                            (None, _) => (theme::TEXT_MUTED, "FilmCraft not linked".to_owned()),
                            (Some(_), false) => (theme::WARN, "FilmCraft not found".to_owned()),
                            (Some(l), true) if !l.created => {
                                (theme::OK, "FilmCraft · not pushed yet".to_owned())
                            }
                            (Some(_), true) => (theme::OK, "FilmCraft · linked".to_owned()),
                        };
                        widgets::chip(ui, &l, c);
                        if let Some(e) = &ws.links_error {
                            widgets::chip(ui, "links.json unreadable", theme::DANGER)
                                .on_hover_text(e);
                        }
                        if widgets::button(ui, "Channel / handoff").clicked() {
                            ws.channel.open = true;
                            ws.channel.refresh(&ws.project, ws.script.source());
                        }
                        if let Some(v) = ws.project.vision_preset() {
                            widgets::chip(ui, &format!("Vision: {v}"), theme::CUE_VISION);
                        }
                    });
                });
            });

        let previous_stage = ws.stage;
        Panel::bottom("ws-nav")
            .exact_size(60.0)
            .show_separator_line(false)
            .frame(panel(theme::BG_PANEL))
            .show(ui, |ui| nav(ui, &mut ws.stage));

        if (go_home || ws.stage != previous_stage)
            && let Err(error) = ws
                .script
                .flush(&ws.project)
                .and_then(|()| ws.visuals.editor.flush(&ws.project))
        {
            ws.document_error = Some(error);
            ws.stage = previous_stage;
            go_home = false;
        }
        if let Some(error) = &ws.document_error {
            Panel::top("document-save-error").show(ui, |ui| {
                ui.label(widgets::text(error, 12.0, theme::DANGER));
            });
        }
        match ws.stage {
            Stage::Script => {
                Panel::top("script-plan-visuals").show(ui, |ui| {
                    if ui
                        .add_enabled(
                            ws.script.selected_range().is_some(),
                            egui::Button::new("Plan visuals for selected narration"),
                        )
                        .clicked()
                        && let Some(range) = ws.script.selected_range()
                    {
                        ws.visuals.add_for_selection(
                            &ws.project,
                            ws.script.source(),
                            range,
                            ui.ctx(),
                        );
                        ws.stage = Stage::VisualScript;
                    }
                });
                if let Some(ev) = ws.script.show(&ws.project, &ws.links, ui) {
                    match ev {
                        ScriptEvent::LinkClip(sel) => {
                            ws.media.focus_selection(sel);
                            ws.stage = Stage::Media;
                        }
                        ScriptEvent::OpenNarration(_) => {}
                    }
                }
            }
            Stage::VisualScript => {
                if let Some(range) = ws
                    .visuals
                    .show(&ws.project, ws.script.source(), &ws.links, ui)
                {
                    ws.script.focus_range(range);
                    ws.stage = Stage::Script;
                }
            }
            Stage::Media => ws.media.show(
                ui,
                &mut ws.project,
                ws.script.script(),
                ws.script.source(),
                &mut ws.links,
            ),
            Stage::Music => ws.music.show(ui, &ws.project, &mut ws.script, catalog),
            Stage::Transitions => {
                ws.transitions
                    .show(ui, &ws.project, &mut ws.script, &ws.links, catalog)
            }
            Stage::Effects => ws
                .effects
                .show(ui, &ws.project, &mut ws.script, &ws.links, catalog),
            Stage::SoundEffects => ws.sfx.show(ui, &ws.project, &mut ws.script, catalog),
            other => placeholder(ui, other),
        }
        crate::screens::channels::project_window(ws, &app.channels, ui);
    }
    if go_home {
        app.cards.clear();
        app.go_home();
    }
}

/// Workflow bar: one button per stage with › between them.
fn nav(ui: &mut egui::Ui, stage: &mut Stage) {
    let full = ui.max_rect();
    ui.painter().line_segment(
        [full.left_top(), full.right_top()],
        Stroke::new(1.0, theme::BORDER),
    );
    let gap = 10.0;
    let item_w = ((full.width() - gap * 7.0) / 8.0).clamp(60.0, 108.0);
    let total = Stage::ALL.len() as f32 * item_w + (Stage::ALL.len() - 1) as f32 * gap;
    let mut x = full.center().x - total / 2.0;
    for (i, s) in Stage::ALL.into_iter().enumerate() {
        let rect =
            egui::Rect::from_min_size(egui::pos2(x, full.top()), egui::vec2(item_w, full.height()));
        let r = ui.interact(rect, ui.id().with(("stage", i)), Sense::click());
        let active = *stage == s;
        let p = ui.painter();
        if active {
            p.rect_filled(rect, CornerRadius::ZERO, theme::BG_HOVER);
            p.line_segment(
                [rect.left_top(), rect.right_top()],
                Stroke::new(2.0, theme::ACCENT),
            );
        } else if r.hovered() {
            p.rect_filled(rect, CornerRadius::ZERO, theme::tint(theme::BG_HOVER, 0.6));
        }
        let color = if active {
            theme::ACCENT
        } else {
            theme::TEXT_MUTED
        };
        stage_icon(p, rect.center() - egui::vec2(0.0, 9.0), s, color);
        p.text(
            rect.center() + egui::vec2(0.0, 14.0),
            Align2::CENTER_CENTER,
            s.label(),
            FontId::proportional(12.0),
            if active { theme::TEXT } else { theme::TEXT_2 },
        );
        if r.clicked() {
            *stage = s;
        }
        if i + 1 < Stage::ALL.len() {
            p.text(
                egui::pos2(x + item_w + gap / 2.0, rect.center().y),
                Align2::CENTER_CENTER,
                "›",
                FontId::proportional(16.0),
                theme::TEXT_MUTED,
            );
        }
        x += item_w + gap;
    }
}

/// Simple line icons drawn with the painter, so no icon font is needed.
fn stage_icon(p: &egui::Painter, c: egui::Pos2, s: Stage, color: egui::Color32) {
    let st = Stroke::new(1.5, color);
    let r = |w: f32, h: f32| egui::Rect::from_center_size(c, egui::vec2(w, h));
    match s {
        Stage::Script | Stage::VisualScript => {
            p.rect_stroke(r(13.0, 16.0), 2.0, st, egui::StrokeKind::Middle);
            for dy in [-3.0, 0.5, 4.0] {
                p.line_segment([c + egui::vec2(-3.5, dy), c + egui::vec2(3.5, dy)], st);
            }
        }
        Stage::Media => {
            p.rect_stroke(r(18.0, 13.0), 2.0, st, egui::StrokeKind::Middle);
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(-2.5, -3.5),
                    c + egui::vec2(4.0, 0.0),
                    c + egui::vec2(-2.5, 3.5),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Stage::Effects => {
            p.rect_stroke(r(18.0, 13.0), 2.0, st, egui::StrokeKind::Middle);
            p.circle_stroke(c + egui::vec2(-3.5, 2.0), 3.0, st);
        }
        Stage::Music => {
            p.line_segment([c + egui::vec2(-3.0, 5.0), c + egui::vec2(-3.0, -6.0)], st);
            p.line_segment([c + egui::vec2(5.0, 3.0), c + egui::vec2(5.0, -8.0)], st);
            p.line_segment([c + egui::vec2(-3.0, -6.0), c + egui::vec2(5.0, -8.0)], st);
            p.circle_filled(c + egui::vec2(-5.0, 5.0), 2.5, color);
            p.circle_filled(c + egui::vec2(3.0, 3.0), 2.5, color);
        }
        Stage::Transitions => {
            for sign in [-1.0f32, 1.0] {
                p.add(egui::Shape::closed_line(
                    vec![
                        c + egui::vec2(8.0 * sign, -6.0),
                        c,
                        c + egui::vec2(8.0 * sign, 6.0),
                    ],
                    st,
                ));
            }
        }
        Stage::SoundEffects => {
            // Speaker with two sound waves.
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(-8.0, -3.0),
                    c + egui::vec2(-4.5, -3.0),
                    c + egui::vec2(0.0, -7.0),
                    c + egui::vec2(0.0, 7.0),
                    c + egui::vec2(-4.5, 3.0),
                    c + egui::vec2(-8.0, 3.0),
                ],
                egui::Color32::TRANSPARENT,
                st,
            ));
            for (r, a) in [(4.0f32, 0.9f32), (7.5, 0.9)] {
                let pts: Vec<egui::Pos2> = (0..=8)
                    .map(|i| {
                        let t = -a + 2.0 * a * i as f32 / 8.0;
                        c + egui::vec2(2.0 + r * t.cos(), r * t.sin())
                    })
                    .collect();
                p.add(egui::Shape::line(pts, st));
            }
        }
        Stage::Sync => {
            p.circle_stroke(c, 7.0, st);
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(5.0, -9.0),
                    c + egui::vec2(9.0, -5.0),
                    c + egui::vec2(4.0, -4.0),
                ],
                color,
                Stroke::NONE,
            ));
        }
    }
}

fn placeholder(ui: &mut egui::Ui, stage: Stage) {
    egui::CentralPanel::default()
        .frame(panel(theme::BG_APP).inner_margin(Margin::same(40)))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                widgets::card(theme::BG_PANEL, theme::BORDER).show(ui, |ui| {
                    ui.set_max_width(560.0);
                    ui.label(strong(stage.label(), 18.0));
                    ui.add_space(6.0);
                    ui.label(text(stage.plan(), 13.0, theme::TEXT_2));
                    ui.add_space(10.0);
                    widgets::chip(
                        ui,
                        "Next up · designed in Figma (Workflow v1)",
                        theme::ACCENT,
                    );
                });
            });
        });
}
