//! Music stage: the script laid out over time, with a bar for each music cue showing what
//! it covers, the gaps with no music, stingers and transitions.
//!
//! - Left: music in the catalog, ranked by the project's vision. Drag a track onto a line
//!   to start it there, onto a music bar to swap the track, or onto a gap to fill it.
//! - Middle: an overview of the whole video, the timeline, and coverage per section.
//! - Right: the selected cue, and checks for the whole video.
//!
//! Timing is estimated from the script (see `spearfia_script::timing`) until clips are
//! linked and analysed. Every change is a text edit to `script.md`.

use crate::screens::script_stage::ScriptStage;
use crate::screens::timeline::{
    self, Geo, LABEL_W, View, cue_key, plain_text, ruler_ticks, script_card, truncate,
};
use crate::theme;
use crate::widgets::{self, mmss, mono, muted, panel, strong, text};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Panel, Pos2, Rect, Sense, Stroke,
    StrokeKind, pos2, vec2,
};
use spearfia_catalog::{AssetKind, Catalog, Entry, LicenseStatus};
use spearfia_core::project::Project;
use spearfia_script::timing::{self, MusicSpan, Span, Timeline};
use spearfia_script::{Block, Script, edit};
use std::collections::HashMap;

/// A music track dragged from the list.
#[derive(Debug, Clone)]
struct TrackDrag {
    /// What goes between the brackets: `:music[hype-drop]`.
    label: String,
}

/// Where a dragged track would land.
#[derive(Debug, Clone, PartialEq)]
enum DropTarget {
    /// Start the track at this block.
    Block(usize),
    /// Swap the track on this music cue (index into `Timeline::music`).
    Bar(usize),
}

const RULER_H: f32 = 22.0;
const SECTION_H: f32 = 24.0;
const SCRIPT_H: f32 = 118.0;
const STRIP_H: f32 = 18.0;
const MUSIC_H: f32 = 62.0;
const STINGER_H: f32 = 30.0;
const LANE_GAP: f32 = 6.0;

pub struct MusicStage {
    /// Selected cue, by id (or `@<byte offset>` for a cue without one).
    selected: Option<String>,
    view: View,
    error: Option<String>,
}

impl MusicStage {
    pub fn new() -> Self {
        Self {
            selected: None,
            view: View::default(),
            error: None,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        script: &mut ScriptStage,
        catalog: &Catalog,
    ) {
        let parsed = spearfia_script::parse(script.source());
        let tl = timing::timeline(&parsed);
        let weights = project
            .vision_preset()
            .and_then(|v| spearfia_vision::preset(&v))
            .map(|p| p.weight_map())
            .unwrap_or_default();
        let mut tracks: Vec<(&Entry, f32)> = catalog
            .entries
            .iter()
            .filter(|e| e.asset.kind == AssetKind::Music)
            .map(|e| (e, e.asset.fit(&weights)))
            .collect();
        tracks.sort_by(|a, b| b.1.total_cmp(&a.1));

        let mut edit: Option<Edit> = None;

        Panel::left("music-tracks")
            .exact_size(270.0)
            .frame(panel(theme::BG_PANEL).inner_margin(Margin::symmetric(10, 14)))
            .show(ui, |ui| track_list(ui, &tracks, !weights.is_empty(), &tl));

        Panel::right("music-inspector")
            .exact_size(320.0)
            .frame(panel(theme::BG_PANEL).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        self.inspector(ui, &parsed, &tl, catalog, &tracks, &mut edit)
                    });
            });

        egui::CentralPanel::default()
            .frame(panel(theme::BG_APP).inner_margin(Margin::same(18)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        self.header(ui, &parsed, &tl);
                        ui.add_space(10.0);
                        self.overview(ui, &tl);
                        ui.add_space(10.0);
                        if let Some(e) = self.timeline(ui, &parsed, &tl, catalog) {
                            edit = Some(e);
                        }
                        ui.add_space(12.0);
                        section_cards(ui, &tl, catalog);
                    });
            });

        if let Some(e) = edit {
            self.apply(project, script, &parsed, &tl, e);
        }
    }

    fn apply(
        &mut self,
        project: &Project,
        script: &mut ScriptStage,
        parsed: &Script,
        tl: &Timeline,
        e: Edit,
    ) {
        let src = script.source().to_owned();
        let (new_src, select) = match e {
            Edit::Drop(DropTarget::Block(b), label) => {
                let id = edit::next_id(parsed, "m");
                let cue = format!(":music[{}]{{#{id}}}", edit::clean_label(&label));
                (edit::add_cue(&src, parsed, b, &cue), Some(id))
            }
            Edit::Drop(DropTarget::Bar(i), label) | Edit::Swap(i, label) => {
                let Some(m) = tl.music.get(i) else { return };
                let mut d = m.cue.clone();
                d.label = Some(edit::clean_label(&label));
                (edit::replace_cue(&src, &m.cue, &d), Some(cue_key(&d)))
            }
            Edit::Remove(i) => {
                let Some(m) = tl.music.get(i) else { return };
                (edit::remove_cue(&src, &m.cue), None)
            }
        };
        let Some(new_src) = new_src else {
            self.error = Some("Couldn't place the cue there.".into());
            return;
        };
        match script.apply_edit(project, new_src) {
            Ok(()) => {
                self.error = None;
                self.selected = select;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, parsed: &Script, tl: &Timeline) {
        ui.horizontal(|ui| {
            ui.label(strong("What music covers what", 16.0));
            let title = parsed.title().unwrap_or("Script");
            ui.label(muted(format!(
                "{title} · {} · {} cue{} · {} gap{}",
                mmss(tl.total),
                tl.music.len(),
                plural(tl.music.len()),
                tl.gaps.len(),
                plural(tl.gaps.len()),
            )));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.view.zoom_buttons(ui);
                if let Some(e) = &self.error {
                    ui.label(text(e, 12.0, theme::DANGER));
                }
            });
        });
    }

    /// The whole video: music bars and gaps under the sections.
    fn overview(&mut self, ui: &mut egui::Ui, tl: &Timeline) {
        self.view.overview(ui, tl, |p, row, x| {
            for m in &tl.music {
                let r = Rect::from_x_y_ranges(x(m.span.start)..=x(m.span.end) - 1.0, row.y_range());
                p.rect_filled(r, 2.0, theme::tint(theme::CUE_MUSIC, 0.7));
            }
            for g in &tl.gaps {
                let r = Rect::from_x_y_ranges(x(g.start)..=x(g.end), row.y_range());
                p.rect_filled(r, 2.0, theme::tint(theme::DANGER, 0.55));
            }
        });
    }

    fn timeline(
        &mut self,
        ui: &mut egui::Ui,
        parsed: &Script,
        tl: &Timeline,
        catalog: &Catalog,
    ) -> Option<Edit> {
        let w = ui.available_width();
        let h = 14.0
            + RULER_H
            + SECTION_H
            + SCRIPT_H
            + STRIP_H
            + MUSIC_H
            + STINGER_H
            + 3.0 * LANE_GAP
            + 34.0;
        let (outer, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        let painter = ui.painter().clone();
        painter.rect(
            outer,
            CornerRadius::same(8),
            theme::BG_PANEL,
            Stroke::new(1.0, theme::BORDER),
            StrokeKind::Inside,
        );
        let inner = outer.shrink2(vec2(12.0, 7.0));
        let area = Rect::from_min_max(
            pos2(inner.left() + LABEL_W, inner.top()),
            inner.right_bottom(),
        );
        let (geo, bg) = self.view.pan(ui, area, tl.total);
        if bg.clicked() {
            self.selected = None;
        }

        // Lane rectangles.
        let mut y = inner.top();
        let mut lane = |h: f32, gap: f32| {
            let r = Rect::from_min_max(pos2(area.left(), y), pos2(area.right(), y + h));
            y += h + gap;
            r
        };
        let ruler = lane(RULER_H, 0.0);
        let sec = lane(SECTION_H, LANE_GAP);
        let script_lane = lane(SCRIPT_H, 0.0);
        let strip = lane(STRIP_H, 0.0);
        let music = lane(MUSIC_H, LANE_GAP);
        let stingers = lane(STINGER_H, LANE_GAP);
        let legend = lane(16.0, 0.0);

        timeline::lane_labels(
            &painter,
            inner.left(),
            &[
                (sec, "SECTION"),
                (script_lane, "SCRIPT"),
                (music, "MUSIC"),
                (stingers, "STINGERS"),
            ],
        );

        let p = painter.with_clip_rect(area);
        ruler_ticks(&p, ruler, &geo, tl.total);

        // Sections.
        for s in &tl.sections {
            let r = geo.rect(s.span, sec.y_range(), 2.0);
            if r.width() <= 0.0 {
                continue;
            }
            p.rect_filled(r, 3.0, theme::BG_HOVER);
            p.with_clip_rect(r.shrink(2.0).intersect(area)).text(
                r.left_center() + vec2(8.0, 0.0),
                Align2::LEFT_CENTER,
                s.title.to_uppercase(),
                FontId::monospace(10.5),
                theme::TEXT,
            );
        }

        // Script cards.
        let mut hovers: Vec<(Rect, DropTarget)> = Vec::new();
        for s in &tl.sections {
            let r = geo.rect(s.span, sec.y_range(), 2.0);
            hovers.push((r, DropTarget::Block(s.block)));
        }
        for (i, b) in parsed.blocks.iter().enumerate() {
            let Block::Line { kind, segments, .. } = b else {
                continue;
            };
            let span = tl.blocks[i];
            if span.is_empty() {
                continue;
            }
            let r = geo.rect(span, script_lane.y_range(), 3.0);
            if r.right() < area.left() || r.left() > area.right() || r.width() < 2.0 {
                continue;
            }
            script_card(&p, r, *kind, segments);
            let words = plain_text(segments);
            ui.interact(r.intersect(area), ui.id().with(("card", i)), Sense::hover())
                .on_hover_text(format!(
                    "{}–{}  {}",
                    mmss(span.start),
                    mmss(span.end),
                    words
                ));
            hovers.push((r, DropTarget::Block(i)));
        }

        // Transitions: a line through the script and music lanes, labelled in the strip.
        let mut label_end = f32::MIN;
        for t in &tl.transitions {
            let x = geo.x(t.at);
            if !(area.left()..=area.right()).contains(&x) {
                continue;
            }
            p.line_segment(
                [pos2(x, script_lane.top()), pos2(x, music.bottom())],
                Stroke::new(1.5, theme::CUE_TRANSITION),
            );
            if x + 3.0 < label_end {
                continue;
            }
            let label = t.cue.display();
            let g = p.layout_no_wrap(label, FontId::monospace(10.0), theme::CUE_TRANSITION);
            label_end = x + g.size().x + 16.0;
            let tag = Rect::from_min_size(
                pos2(x + 3.0, strip.center().y - g.size().y / 2.0 - 1.0),
                g.size() + vec2(8.0, 2.0),
            );
            p.rect_filled(tag, 3.0, theme::tint(theme::CUE_TRANSITION, 0.14));
            p.galley(tag.min + vec2(4.0, 1.0), g, theme::CUE_TRANSITION);
        }

        // Music bars and gaps.
        let gap_targets: Vec<(Rect, DropTarget)> = tl
            .gaps
            .iter()
            .filter_map(|g| {
                let r = geo.rect(*g, music.y_range(), 2.0);
                let block = first_block_at(parsed, tl, g.start)?;
                Some((r, DropTarget::Block(block)))
            })
            .collect();
        for g in &tl.gaps {
            let r = geo.rect(*g, music.y_range(), 2.0);
            if r.width() < 1.0 {
                continue;
            }
            p.rect(
                r,
                CornerRadius::same(4),
                theme::tint(theme::DANGER, 0.08),
                Stroke::new(1.0, theme::tint(theme::DANGER, 0.5)),
                StrokeKind::Inside,
            );
            if r.width() > 28.0 {
                p.text(
                    r.left_top() + vec2(6.0, 5.0),
                    Align2::LEFT_TOP,
                    format!("{:.0}s no music", g.len()),
                    FontId::monospace(10.0),
                    theme::DANGER,
                );
            }
        }
        let vo: Vec<Span> = parsed
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| matches!(b, Block::Line { kind, .. } if kind.is_spoken()))
            .map(|(i, _)| tl.blocks[i])
            .filter(|s| !s.is_empty())
            .collect();
        let mut bar_targets: Vec<(Rect, DropTarget)> = Vec::new();
        let mut clicked: Option<String> = None;
        for (i, m) in tl.music.iter().enumerate() {
            let r = geo.rect(m.span, music.y_range(), 2.0);
            if r.width() < 1.0 {
                continue;
            }
            let key = cue_key(&m.cue);
            let selected = self.selected.as_deref() == Some(key.as_str());
            let entry = m.track().and_then(|l| catalog.find(AssetKind::Music, l));
            music_bar(&p, r, m, entry, &vo, &geo, selected);
            let resp = ui.interact(r.intersect(area), ui.id().with(("bar", i)), Sense::click());
            if resp.clicked() {
                clicked = Some(key);
            }
            resp.on_hover_text(format!(
                "{} · {}–{} ({})",
                bar_title(m),
                mmss(m.span.start),
                mmss(m.span.end),
                mmss(m.span.len())
            ));
            bar_targets.push((r, DropTarget::Bar(i)));
        }
        if let Some(k) = clicked {
            self.selected = Some(k);
        }

        // Stingers.
        let mut last_label_end = f32::MIN;
        for s in &tl.stingers {
            let x = geo.x(s.at);
            if !(area.left() - 8.0..=area.right()).contains(&x) {
                continue;
            }
            let c = pos2(x, stingers.center().y);
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + vec2(0.0, -6.0),
                    c + vec2(6.0, 0.0),
                    c + vec2(0.0, 6.0),
                    c + vec2(-6.0, 0.0),
                ],
                theme::CUE_SFX,
                Stroke::NONE,
            ));
            if x + 10.0 > last_label_end {
                let rr = p.text(
                    c + vec2(10.0, 0.0),
                    Align2::LEFT_CENTER,
                    s.cue.display(),
                    FontId::monospace(10.5),
                    theme::CUE_SFX,
                );
                last_label_end = rr.right() + 8.0;
            }
        }

        // Legend.
        timeline::legend(
            &painter,
            legend,
            &[
                (theme::tint(theme::CUE_MUSIC, 0.6), "music cue"),
                (theme::tint(theme::CUE_MUSIC, 0.25), "ducked under VO"),
                (theme::tint(theme::DANGER, 0.55), "no music"),
                (theme::CUE_TRANSITION, "transition"),
                (theme::CUE_SFX, "stinger (sfx)"),
            ],
            "scroll to pan · Ctrl+scroll to zoom",
        );

        // Dropping a track.
        let dragging = egui::DragAndDrop::has_payload_of_type::<TrackDrag>(ui.ctx());
        if !dragging {
            return None;
        }
        let pointer = ui.ctx().pointer_hover_pos()?;
        if !area.contains(pointer) {
            return None;
        }
        // Bars win over gaps, which win over lines.
        let target = bar_targets
            .iter()
            .chain(gap_targets.iter())
            .chain(hovers.iter().rev())
            .find(|(r, _)| r.contains(pointer))
            .cloned();
        let (rect, target) = target?;
        let hint = match &target {
            DropTarget::Bar(_) => "swap the track on this cue",
            DropTarget::Block(b) if matches!(parsed.blocks[*b], Block::Heading { .. }) => {
                "start this track with the section"
            }
            DropTarget::Block(_) => "start this track on this line",
        };
        let p = painter.with_clip_rect(area.expand(2.0));
        p.rect_stroke(
            rect.expand(2.0),
            5.0,
            Stroke::new(2.0, theme::ACCENT),
            StrokeKind::Outside,
        );
        let g = p.layout_no_wrap(hint.into(), FontId::proportional(12.0), theme::ACCENT_ON);
        let tag = Rect::from_min_size(
            pointer + vec2(14.0, -g.size().y - 26.0),
            g.size() + vec2(12.0, 6.0),
        );
        painter.rect_filled(tag, 4.0, theme::ACCENT);
        painter.galley(tag.min + vec2(6.0, 3.0), g, theme::ACCENT_ON);

        if ui.input(|i| i.pointer.any_released())
            && let Some(d) = egui::DragAndDrop::take_payload::<TrackDrag>(ui.ctx())
        {
            return Some(Edit::Drop(target, d.label.clone()));
        }
        None
    }

    fn inspector(
        &mut self,
        ui: &mut egui::Ui,
        parsed: &Script,
        tl: &Timeline,
        catalog: &Catalog,
        tracks: &[(&Entry, f32)],
        edit: &mut Option<Edit>,
    ) {
        let selected = self.selected.as_deref().and_then(|k| {
            tl.music
                .iter()
                .enumerate()
                .find(|(_, m)| cue_key(&m.cue) == k)
        });
        match selected {
            Some((i, m)) => self.cue_details(ui, i, m, parsed, tl, catalog, tracks, edit),
            None => {
                ui.label(strong("Music cues", 13.5));
                ui.add_space(4.0);
                ui.label(muted(
                    "Click a bar to see its track. Drag a track from the left onto a line \
                     to start it there, onto a bar to swap it, or onto a red gap to fill it.",
                ));
                ui.add_space(4.0);
                ui.label(muted(
                    "In the script a cue is :music[track]; it plays until the next music cue \
                     or :music[stop].",
                ));
            }
        }
        ui.add_space(14.0);
        widgets::rule(ui);
        ui.add_space(10.0);
        ui.label(strong("Checks", 13.5));
        ui.add_space(6.0);
        let issues = checks(tl, catalog);
        if issues.is_empty() {
            ui.label(text("No problems found.", 12.5, theme::OK));
        }
        for c in issues {
            let r = egui::Frame::new()
                .fill(theme::BG_RAISED)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        ui.label(text("!", 13.0, c.color).strong());
                        ui.vertical(|ui| {
                            ui.label(text(&c.title, 12.5, theme::TEXT));
                            ui.label(mono(&c.detail, theme::TEXT_MUTED));
                        });
                    });
                })
                .response
                .interact(Sense::click());
            if r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.clicked() {
                self.view.show_time(c.at);
                if let Some(k) = c.select {
                    self.selected = Some(k);
                }
            }
            ui.add_space(5.0);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn cue_details(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        m: &MusicSpan,
        parsed: &Script,
        tl: &Timeline,
        catalog: &Catalog,
        tracks: &[(&Entry, f32)],
        edit: &mut Option<Edit>,
    ) {
        let entry = m.track().and_then(|l| catalog.find(AssetKind::Music, l));
        ui.horizontal(|ui| {
            widgets::chip(ui, &bar_title(m), theme::CUE_MUSIC);
            if let Some(id) = &m.cue.id {
                ui.label(mono(format!("#{id}"), theme::TEXT_MUTED));
            }
        });
        ui.add_space(8.0);
        if let Some(e) = entry {
            let a = &e.asset;
            let lic = match a.license_status() {
                LicenseStatus::Open => theme::OK,
                LicenseStatus::Check => theme::WARN,
                LicenseStatus::Unknown => theme::DANGER,
            };
            ui.horizontal(|ui| {
                ui.label(mono(&a.id, theme::TEXT_2));
                widgets::tag(ui, &a.license, lic);
            });
            if let Some(s) = &a.summary {
                ui.label(text(s, 12.0, theme::TEXT_2));
            }
            ui.add_space(6.0);
        } else if let Some(l) = m.track() {
            ui.label(text(
                format!("\"{l}\" isn't in your catalog."),
                12.5,
                theme::WARN,
            ));
            ui.add_space(6.0);
        }

        let section = tl
            .section_of(m.block)
            .map(|s| s.title.clone())
            .unwrap_or_else(|| "Before the first section".into());
        let last_section = tl
            .sections
            .iter()
            .rev()
            .find(|s| s.span.start < m.span.end)
            .map(|s| s.title.clone());
        let covers = match last_section {
            Some(l) if l != section => format!("{section} to {l}"),
            _ => section,
        };
        widgets::kv(ui, "covers", text(covers, 12.5, theme::TEXT));
        widgets::kv(
            ui,
            "time",
            text(
                format!(
                    "{}–{} ({})",
                    mmss(m.span.start),
                    mmss(m.span.end),
                    mmss(m.span.len())
                ),
                12.5,
                theme::TEXT,
            ),
        );
        let audio = entry.and_then(|e| e.asset.audio.as_ref());
        if let Some(bpm) = audio.and_then(|a| a.bpm) {
            widgets::kv(
                ui,
                "tempo",
                text(format!("{bpm:.0} bpm"), 12.5, theme::TEXT),
            );
        }
        if let Some(len) = audio.and_then(|a| a.seconds) {
            let short = len + 0.5 < m.span.len();
            widgets::kv(
                ui,
                "length",
                text(
                    if short {
                        format!("{} — {} short", mmss(len), mmss(m.span.len() - len))
                    } else {
                        mmss(len)
                    },
                    12.5,
                    if short { theme::WARN } else { theme::TEXT },
                ),
            );
        }
        for (k, v) in &m.cue.attrs {
            widgets::kv(ui, k, text(v, 12.5, theme::TEXT));
        }
        if let Some(a) = audio
            && !a.markers.is_empty()
        {
            let list = a
                .markers
                .iter()
                .map(|mk| format!("{} at {}", mk.name, mmss(mk.at)))
                .collect::<Vec<_>>()
                .join(", ");
            widgets::kv(ui, "markers", text(list, 12.5, theme::TEXT));
        }
        let line = parsed.blocks.get(m.block).map(|b| match b {
            Block::Heading { text: t, .. } => format!("## {t}"),
            Block::Line { segments, .. } => plain_text(segments),
            Block::Divider { cue, .. } => cue.display(),
        });
        if let Some(l) = line {
            widgets::kv(ui, "starts at", text(truncate(&l, 80), 12.0, theme::TEXT_2));
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if widgets::button(ui, "Remove cue").clicked() {
                *edit = Some(Edit::Remove(index));
            }
            if widgets::button(ui, "Show").clicked() {
                self.view.start = (m.span.start - 2.0).max(0.0);
            }
        });

        // Suggestions: catalog tracks matching the cue's mood, or the best fits.
        let mood = m.cue.attr("mood").map(str::to_lowercase);
        let mut picks: Vec<&(&Entry, f32)> = tracks
            .iter()
            .filter(|(e, _)| m.track().is_none_or(|l| !e.asset.answers_to(l)))
            .filter(|(e, _)| {
                mood.as_deref()
                    .is_none_or(|md| e.asset.tags.iter().any(|t| t.eq_ignore_ascii_case(md)))
            })
            .collect();
        picks.truncate(4);
        if !picks.is_empty() {
            ui.add_space(12.0);
            ui.label(strong(
                match &mood {
                    Some(md) => format!("Tracks tagged \"{md}\""),
                    None => "Swap for".to_owned(),
                },
                12.5,
            ));
            ui.add_space(4.0);
            for (e, fit) in picks {
                ui.horizontal(|ui| {
                    ui.label(text(&e.asset.name, 12.5, theme::TEXT));
                    if *fit != 0.0 {
                        ui.label(mono(format!("{fit:+.1}"), fit_color(*fit)));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::button(ui, "Use").clicked() {
                            *edit = Some(Edit::Swap(index, e.asset.short_name().to_owned()));
                        }
                    });
                });
            }
        }
    }
}

impl Default for MusicStage {
    fn default() -> Self {
        Self::new()
    }
}

enum Edit {
    Drop(DropTarget, String),
    Swap(usize, String),
    Remove(usize),
}

fn music_bar(
    p: &egui::Painter,
    r: Rect,
    m: &MusicSpan,
    entry: Option<&Entry>,
    vo: &[Span],
    geo: &Geo,
    selected: bool,
) {
    let color = if m.track().is_none() || entry.is_none() {
        theme::WARN
    } else {
        theme::CUE_MUSIC
    };
    p.rect(
        r,
        CornerRadius::same(4),
        theme::tint(color, 0.12),
        Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected {
                theme::ACCENT
            } else {
                theme::tint(color, 0.8)
            },
        ),
        StrokeKind::Inside,
    );
    // Level: full where nothing is spoken, ducked under VO.
    let c = p.with_clip_rect(r.shrink(1.0).intersect(p.clip_rect()));
    let body = Rect::from_min_max(
        pos2(r.left(), r.top() + 20.0),
        r.right_bottom() - vec2(0.0, 1.0),
    );
    c.rect_filled(
        Rect::from_min_max(pos2(body.left(), body.top() + 4.0), body.right_bottom()),
        0.0,
        theme::tint(color, 0.45),
    );
    for s in vo {
        if s.overlap(m.span) <= 0.0 {
            continue;
        }
        let d = Rect::from_x_y_ranges(
            geo.x(s.start.max(m.span.start))..=geo.x(s.end.min(m.span.end)),
            body.top()..=body.bottom(),
        );
        c.rect_filled(d, 0.0, theme::tint(theme::BG_PANEL, 0.9));
        c.rect_filled(
            Rect::from_min_max(
                pos2(d.left(), body.top() + body.height() * 0.62),
                d.right_bottom(),
            ),
            0.0,
            theme::tint(color, 0.2),
        );
    }
    // Where the track runs out, if it's shorter than the stretch it covers.
    if let Some(len) = entry.and_then(|e| e.asset.audio.as_ref()?.seconds)
        && len + 0.5 < m.span.len()
    {
        let x = geo.x(m.span.start + len);
        c.extend(egui::Shape::dashed_line(
            &[pos2(x, r.top() + 2.0), pos2(x, r.bottom() - 2.0)],
            Stroke::new(1.5, theme::WARN),
            4.0,
            3.0,
        ));
        c.text(
            pos2(x + 4.0, r.top() + 4.0),
            Align2::LEFT_TOP,
            "track ends",
            FontId::monospace(10.0),
            theme::WARN,
        );
    }
    // Keep the name in view when the bar starts off the left edge.
    let lx = r
        .left()
        .max(p.clip_rect().left())
        .min(r.right() - 24.0)
        .max(r.left());
    note_icon(&c, pos2(lx + 10.0, r.top() + 10.0), color);
    c.text(
        pos2(lx + 19.0, r.top() + 10.0),
        Align2::LEFT_CENTER,
        bar_title(m),
        FontId::proportional(12.0),
        theme::TEXT,
    );
}

/// A small ♪ drawn with the painter, so no icon font is needed.
fn note_icon(p: &egui::Painter, c: Pos2, color: Color32) {
    p.line_segment(
        [c + vec2(1.5, 3.0), c + vec2(1.5, -5.0)],
        Stroke::new(1.3, color),
    );
    p.line_segment(
        [c + vec2(1.5, -5.0), c + vec2(4.5, -3.0)],
        Stroke::new(1.3, color),
    );
    p.circle_filled(c + vec2(-0.5, 3.0), 2.2, color);
}

fn bar_title(m: &MusicSpan) -> String {
    match (m.track(), m.cue.attr("mood")) {
        (Some(t), _) => t.to_owned(),
        (None, Some(mood)) => format!("mood: {mood} · no track yet"),
        (None, None) => "no track yet".to_owned(),
    }
}

fn track_list(ui: &mut egui::Ui, tracks: &[(&Entry, f32)], ranked: bool, tl: &Timeline) {
    ui.horizontal(|ui| {
        ui.label(strong("Music", 13.5));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(mono(
                if ranked { "ranked by vision" } else { "A–Z" },
                theme::TEXT_MUTED,
            ));
        });
    });
    ui.add_space(8.0);
    if tracks.is_empty() {
        ui.label(muted(
            "No music in your catalog yet. Add *.asset.json descriptors with \"kind\": \
             \"music\" to a catalog source (Catalog tab), then drag tracks onto the timeline.",
        ));
        ui.add_space(6.0);
        ui.label(muted(
            "Cues already in the script still show, by mood, until they name a track.",
        ));
        return;
    }
    let in_use: HashMap<String, usize> = tracks
        .iter()
        .map(|(e, _)| {
            let n = tl
                .music
                .iter()
                .filter(|m| m.track().is_some_and(|l| e.asset.answers_to(l)))
                .count();
            (e.asset.id.clone(), n)
        })
        .collect();
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for (e, fit) in tracks {
                let a = &e.asset;
                let id = egui::Id::new(("track", &a.id));
                let payload = TrackDrag {
                    label: a.short_name().to_owned(),
                };
                let uses = in_use.get(&a.id).copied().unwrap_or(0);
                widgets::drag_card(ui, id, payload, |ui| {
                    egui::Frame::new()
                        .fill(if uses > 0 {
                            theme::BG_RAISED
                        } else {
                            Color32::TRANSPARENT
                        })
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(8, 7))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                note_icon(
                                    ui.painter(),
                                    ui.cursor().min + vec2(5.0, 9.0),
                                    theme::CUE_MUSIC,
                                );
                                ui.add_space(14.0);
                                ui.label(text(&a.name, 13.0, theme::TEXT));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ranked {
                                        ui.label(mono(format!("{fit:+.1}"), fit_color(*fit)));
                                    }
                                    if uses > 0 {
                                        ui.label(mono(
                                            if uses == 1 {
                                                "in use".to_owned()
                                            } else {
                                                format!("in use ×{uses}")
                                            },
                                            theme::CUE_MUSIC,
                                        ));
                                    }
                                });
                            });
                            let mut meta: Vec<String> = Vec::new();
                            if let Some(au) = &a.audio {
                                if let Some(b) = au.bpm {
                                    meta.push(format!("{b:.0} bpm"));
                                }
                                if let Some(s) = au.seconds {
                                    meta.push(mmss(s));
                                }
                            }
                            meta.extend(a.tags.iter().take(2).cloned());
                            ui.add(
                                egui::Label::new(mono(meta.join(" · "), theme::TEXT_MUTED))
                                    .truncate(),
                            );
                        });
                })
                .response
                .on_hover_text(a.summary.as_deref().unwrap_or("Drag onto the timeline"));
                ui.add_space(2.0);
            }
        });
}

fn section_cards(ui: &mut egui::Ui, tl: &Timeline, catalog: &Catalog) {
    if tl.sections.is_empty() {
        return;
    }
    egui::Frame::new()
        .fill(theme::BG_PANEL)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(strong("Music by section", 13.5));
            ui.add_space(8.0);
            let per_row = 3usize;
            let gap = 8.0;
            let w =
                ((ui.available_width() - gap * (per_row as f32 - 1.0)) / per_row as f32).floor();
            for row in tl.sections.chunks(per_row) {
                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for s in row {
                        section_card(ui, w, s, tl, catalog);
                    }
                });
                ui.add_space(gap);
            }
        });
}

fn section_card(
    ui: &mut egui::Ui,
    w: f32,
    s: &spearfia_script::timing::SectionTime,
    tl: &Timeline,
    catalog: &Catalog,
) {
    let pct = if s.span.len() > 0.0 {
        tl.music_over(s.span) / s.span.len()
    } else {
        0.0
    };
    let color = if pct >= 0.9 {
        theme::OK
    } else if pct >= 0.5 {
        theme::WARN
    } else {
        theme::DANGER
    };
    let names: Vec<String> = tl.music_in(s.span).map(bar_title).collect();
    let missing = tl.music_in(s.span).any(|m| {
        m.track()
            .is_none_or(|l| catalog.find(AssetKind::Music, l).is_none())
    });
    ui.allocate_ui_with_layout(vec2(w, 70.0), Layout::top_down(Align::Min), |ui| {
        egui::Frame::new()
            .fill(theme::BG_RAISED)
            .stroke(Stroke::new(
                1.0,
                if pct < 0.9 {
                    theme::tint(color, 0.5)
                } else {
                    theme::BORDER
                },
            ))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(11, 9))
            .show(ui, |ui| {
                ui.set_width(w - 22.0);
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(text(&s.title, 12.5, theme::TEXT)).truncate());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(mono(format!("{:.0}%", pct * 100.0), color));
                    });
                });
                let what = if names.is_empty() {
                    "no music yet".to_owned()
                } else {
                    names.join(", ")
                };
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(text(
                            what,
                            12.0,
                            if names.is_empty() || missing {
                                theme::WARN
                            } else {
                                theme::CUE_MUSIC
                            },
                        ))
                        .truncate(),
                    );
                    ui.label(mono(
                        format!("{}–{}", mmss(s.span.start), mmss(s.span.end)),
                        theme::TEXT_MUTED,
                    ));
                });
                ui.add_space(4.0);
                let (bar, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 3.0), Sense::hover());
                ui.painter().rect_filled(bar, 2.0, theme::BG_HOVER);
                let mut fill = bar;
                fill.set_width(bar.width() * pct.clamp(0.0, 1.0));
                ui.painter().rect_filled(fill, 2.0, theme::CUE_MUSIC);
            });
    });
}

struct Check {
    title: String,
    detail: String,
    color: Color32,
    /// Where to jump the view.
    at: f32,
    select: Option<String>,
}

fn checks(tl: &Timeline, catalog: &Catalog) -> Vec<Check> {
    let mut out = Vec::new();
    for g in &tl.gaps {
        let sec = tl
            .sections
            .iter()
            .rev()
            .find(|s| s.span.start <= g.start)
            .map(|s| s.title.as_str())
            .unwrap_or("the start");
        out.push(Check {
            title: format!("{:.0}s with no music", g.len()),
            detail: format!("{}–{} · {sec}", mmss(g.start), mmss(g.end)),
            color: theme::DANGER,
            at: g.start,
            select: None,
        });
    }
    for m in &tl.music {
        let key = Some(cue_key(&m.cue));
        let when = format!("{}–{}", mmss(m.span.start), mmss(m.span.end));
        let Some(label) = m.track() else {
            out.push(Check {
                title: "Cue has no track yet".into(),
                detail: format!("{} · {when}", bar_title(m)),
                color: theme::WARN,
                at: m.span.start,
                select: key,
            });
            continue;
        };
        let Some(e) = catalog.find(AssetKind::Music, label) else {
            out.push(Check {
                title: format!("\"{label}\" isn't in the catalog"),
                detail: when,
                color: theme::WARN,
                at: m.span.start,
                select: key,
            });
            continue;
        };
        if let Some(len) = e.asset.audio.as_ref().and_then(|a| a.seconds)
            && len + 0.5 < m.span.len()
        {
            out.push(Check {
                title: format!("{label} runs out {} early", mmss(m.span.len() - len)),
                detail: format!("track {} · covers {}", mmss(len), mmss(m.span.len())),
                color: theme::WARN,
                at: m.span.start + len,
                select: key.clone(),
            });
        }
        if e.asset.license_status() != LicenseStatus::Open {
            out.push(Check {
                title: format!("Check the license of {label}"),
                detail: if e.asset.license.is_empty() {
                    "no license given".into()
                } else {
                    e.asset.license.clone()
                },
                color: theme::WARN,
                at: m.span.start,
                select: key,
            });
        }
    }
    out
}

/// The first spoken or on-screen line that starts at or after `t`.
fn first_block_at(parsed: &Script, tl: &Timeline, t: f32) -> Option<usize> {
    parsed
        .blocks
        .iter()
        .enumerate()
        .find(|(i, b)| {
            matches!(b, Block::Line { .. }) && !tl.blocks[*i].is_empty() && tl.blocks[*i].end > t
        })
        .map(|(i, _)| i)
}

fn fit_color(fit: f32) -> Color32 {
    if fit > 0.0 {
        theme::OK
    } else if fit < 0.0 {
        theme::DANGER
    } else {
        theme::TEXT_MUTED
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_and_untracked_cues_are_checked() {
        let s = spearfia_script::parse(
            "## A :music{mood=hype #m1}\none two three four five\n## B\nsix seven eight nine ten\n",
        );
        let tl = timing::timeline(&s);
        let c = checks(&tl, &Catalog::default());
        // Section B has no music of its own, but m1 keeps playing into it.
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].title, "Cue has no track yet");
        assert_eq!(c[0].select.as_deref(), Some("m1"));
    }

    #[test]
    fn drops_into_a_gap_land_on_the_next_line() {
        let s = spearfia_script::parse("## A\n[ON SCREEN: x]\none two three\n");
        let tl = timing::timeline(&s);
        assert_eq!(first_block_at(&s, &tl, 0.0), Some(1));
        assert_eq!(first_block_at(&s, &tl, 2.0), Some(2));
        assert_eq!(first_block_at(&s, &tl, 99.0), None);
    }
}
