//! Sound effects stage: every `:sfx` cue on the timeline over the script, on the word, cut
//! or section it is written on.
//!
//! - Left: sound effects in the catalog, ranked by the project's vision. Drag one onto the
//!   timeline: onto a word (it lands before the nearest word boundary), onto a cut, or onto
//!   a section to play as it starts.
//! - Middle: the shared timeline (`timeline.rs`) with the script's words, the cuts, music
//!   with its hit points (drops), and the sound effects. Drag a cue to move it.
//! - Right: the selected cue (where it sits, nudge a word left or right, swap, remove) and
//!   checks: sounds missing from the catalog, two sounds at once, sounds near a music hit,
//!   long sounds under VO, licenses.
//!
//! Timing is estimated from the script until clips are linked and analysed. Every change is
//! a text edit to `script.md` (`spearfia_script::sfx`).

use crate::screens::script_stage::ScriptStage;
use crate::screens::timeline::{
    self, Geo, LABEL_W, View, card_frame, cue_key, plain_text, ruler_ticks, script_card, truncate,
};
use crate::screens::transitions_stage::{fit_color, mmss_tenths};
use crate::theme;
use crate::widgets::{self, mmss, mono, muted, panel, strong, text};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Panel, Pos2, Rect, Sense, Stroke,
    StrokeKind, pos2, vec2,
};
use spearfia_catalog::{AssetKind, Catalog, Entry, LicenseStatus};
use spearfia_core::project::Project;
use spearfia_script::cuts;
use spearfia_script::sfx::{self, Place, Sfx, Target, Word};
use spearfia_script::timing::{self, Timeline};
use spearfia_script::{Block, LineKind, Script};

const RULER_H: f32 = 22.0;
const SECTION_H: f32 = 24.0;
const SCRIPT_H: f32 = 96.0;
const CUTS_H: f32 = 20.0;
const MUSIC_H: f32 = 34.0;
const SFX_H: f32 = 76.0;
const LANE_GAP: f32 = 6.0;
/// Two sounds closer than this play at once.
const CLASH_SECONDS: f32 = 0.3;
/// A sound this close to a music hit competes with it.
const HIT_SECONDS: f32 = 0.5;
/// Pixels either side of a cut that count as dropping on it.
const CUT_SNAP_PX: f32 = 7.0;

/// A sound effect dragged from the library, or a cue being moved.
#[derive(Debug, Clone)]
struct SfxDrag {
    label: String,
    /// Key of the cue being moved, if this is a move.
    moving: Option<String>,
}

enum Edit {
    Add(Target, String),
    Move(String, Target),
    Rename(String, String),
    Remove(String),
}

pub struct SfxStage {
    selected: Option<String>,
    view: View,
    rename: String,
    error: Option<String>,
}

impl Default for SfxStage {
    fn default() -> Self {
        Self::new()
    }
}

/// The catalog sound a cue names: by name or id, else by a tag equal to the whole label
/// (`boom` → `boom-impact`). The flag is true for a name or id match.
fn resolve<'a>(catalog: &'a Catalog, label: &str) -> Option<(&'a Entry, bool)> {
    if let Some(e) = catalog.find(AssetKind::Sfx, label) {
        return Some((e, true));
    }
    catalog
        .tagged(AssetKind::Sfx, label)
        .into_iter()
        .next()
        .map(|e| (e, false))
}

/// How many sounds the label could mean, counted the way [`resolve`] matches it.
fn candidates(catalog: &Catalog, label: &str) -> usize {
    match catalog.find_all(AssetKind::Sfx, label).len() {
        0 => catalog.tagged(AssetKind::Sfx, label).len(),
        n => n,
    }
}

/// A music hit (a track marker such as `drop`) placed on the timeline.
struct Hit {
    at: f32,
    track: String,
    name: String,
}

fn hits(tl: &Timeline, catalog: &Catalog) -> Vec<Hit> {
    let mut out = Vec::new();
    for m in &tl.music {
        let Some(e) = m.track().and_then(|l| catalog.find(AssetKind::Music, l)) else {
            continue;
        };
        for mk in e.asset.audio.iter().flat_map(|a| &a.markers) {
            let at = m.span.start + mk.at;
            if at < m.span.end {
                out.push(Hit {
                    at,
                    track: e.asset.short_name().to_owned(),
                    name: mk.name.clone(),
                });
            }
        }
    }
    out
}

impl SfxStage {
    pub fn new() -> Self {
        Self {
            selected: None,
            view: View::default(),
            rename: String::new(),
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
        let all = sfx::sfx(&parsed, &tl);
        let words = sfx::words(&parsed, &tl);
        let music_hits = hits(&tl, catalog);
        let weights = project
            .vision_preset()
            .and_then(|v| spearfia_vision::preset(&v))
            .map(|p| p.weight_map())
            .unwrap_or_default();
        let mut library: Vec<(&Entry, f32)> = catalog
            .entries
            .iter()
            .filter(|e| e.asset.kind == AssetKind::Sfx)
            .map(|e| (e, e.asset.fit(&weights)))
            .collect();
        library.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then_with(|| a.0.asset.short_name().cmp(b.0.asset.short_name()))
        });
        let issues = checks(&parsed, &all, &words, &music_hits, catalog);

        let mut edit: Option<Edit> = None;

        Panel::left("sfx-library")
            .exact_size(270.0)
            .frame(panel(theme::BG_PANEL).inner_margin(Margin::symmetric(10, 14)))
            .show(ui, |ui| {
                sound_list(ui, &library, !weights.is_empty(), &all, catalog)
            });

        Panel::right("sfx-inspector")
            .exact_size(320.0)
            .frame(panel(theme::BG_PANEL).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        let sel = self
                            .selected
                            .as_deref()
                            .and_then(|k| all.iter().find(|s| cue_key(&s.cue) == k));
                        match sel {
                            Some(s) => {
                                self.details(ui, s, &parsed, &tl, catalog, &library, &mut edit)
                            }
                            None => {
                                ui.label(strong("Sound effects", 13.5));
                                ui.add_space(4.0);
                                ui.label(muted(
                                    "Drag a sound from the left onto a word, a cut or a section. \
                                     Click a diamond to see it; drag it to move it.",
                                ));
                                ui.add_space(4.0);
                                ui.label(muted(
                                    "In the script a cue is :sfx[name], written before the word \
                                     it lands on.",
                                ));
                            }
                        }
                        ui.add_space(14.0);
                        widgets::rule(ui);
                        ui.add_space(10.0);
                        self.checks_list(ui, &issues);
                    });
            });

        egui::CentralPanel::default()
            .frame(panel(theme::BG_APP).inner_margin(Margin::same(18)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        self.header(ui, &tl, &all);
                        ui.add_space(10.0);
                        self.view.overview(ui, &tl, |p, row, x| {
                            for m in &tl.music {
                                let r = Rect::from_x_y_ranges(
                                    x(m.span.start)..=x(m.span.end) - 1.0,
                                    row.top() + 2.0..=row.bottom() - 2.0,
                                );
                                p.rect_filled(r, 2.0, theme::tint(theme::CUE_MUSIC, 0.35));
                            }
                            for s in &all {
                                diamond(p, pos2(x(s.at), row.center().y), 4.0, theme::CUE_SFX);
                            }
                        });
                        ui.add_space(10.0);
                        if let Some(e) =
                            self.timeline(ui, &parsed, &tl, &all, &words, &music_hits, catalog)
                        {
                            edit = Some(e);
                        }
                    });
            });

        if let Some(e) = edit {
            self.apply(project, script, e);
        }
    }

    fn apply(&mut self, project: &Project, script: &mut ScriptStage, e: Edit) {
        let src = script.source().to_owned();
        let parsed = spearfia_script::parse(&src);
        let tl = timing::timeline(&parsed);
        let all = sfx::sfx(&parsed, &tl);
        let find = |k: &str| all.iter().find(|s| cue_key(&s.cue) == k);
        let (out, select) = match e {
            Edit::Add(t, label) => {
                let id = spearfia_script::edit::next_id(&parsed, "s");
                (sfx::add(&src, &parsed, t, &label), Some(id))
            }
            Edit::Move(k, t) => (find(&k).and_then(|s| sfx::move_to(&src, s, t)), Some(k)),
            Edit::Rename(k, l) => (find(&k).and_then(|s| sfx::set_label(&src, s, &l)), Some(k)),
            Edit::Remove(k) => (find(&k).and_then(|s| sfx::remove(&src, s)), None),
        };
        let Some(out) = out else {
            self.error = Some("Couldn't place the sound there.".into());
            return;
        };
        if out != src {
            match script.apply_edit(project, out) {
                Ok(()) => {
                    self.error = None;
                    self.selected = select;
                    self.rename.clear();
                }
                Err(e) => self.error = Some(e),
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, tl: &Timeline, all: &[Sfx]) {
        let on = |f: fn(&Place) -> bool| all.iter().filter(|s| f(&s.place)).count();
        ui.horizontal(|ui| {
            ui.label(strong("Sound effects", 16.0));
            ui.label(muted(format!(
                "{} · {} cue{} · {} on words · {} on cuts · {} with sections",
                mmss(tl.total),
                all.len(),
                if all.len() == 1 { "" } else { "s" },
                on(|p| matches!(p, Place::Word { .. } | Place::LineEnd)),
                on(|p| matches!(p, Place::Cut | Place::OnScreen)),
                on(|p| matches!(p, Place::Section)),
            )));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.view.zoom_buttons(ui);
                if let Some(e) = &self.error {
                    ui.label(text(e, 12.0, theme::DANGER));
                }
            });
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn timeline(
        &mut self,
        ui: &mut egui::Ui,
        parsed: &Script,
        tl: &Timeline,
        all: &[Sfx],
        words: &[Word],
        music_hits: &[Hit],
        catalog: &Catalog,
    ) -> Option<Edit> {
        let w = ui.available_width();
        let h = 14.0
            + RULER_H
            + SECTION_H
            + SCRIPT_H
            + CUTS_H
            + MUSIC_H
            + SFX_H
            + 4.0 * LANE_GAP
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

        let mut y = inner.top();
        let mut lane = |h: f32, gap: f32| {
            let r = Rect::from_min_max(pos2(area.left(), y), pos2(area.right(), y + h));
            y += h + gap;
            r
        };
        let ruler = lane(RULER_H, 0.0);
        let sec = lane(SECTION_H, LANE_GAP);
        let script_lane = lane(SCRIPT_H, 0.0);
        let cuts_lane = lane(CUTS_H, LANE_GAP);
        let music = lane(MUSIC_H, LANE_GAP);
        let sfx_lane = lane(SFX_H, LANE_GAP);
        let legend = lane(16.0, 0.0);
        timeline::lane_labels(
            &painter,
            inner.left(),
            &[
                (sec, "SECTION"),
                (script_lane, "SCRIPT"),
                (cuts_lane, "CUTS"),
                (music, "MUSIC"),
                (sfx_lane, "SFX"),
            ],
        );

        let p = painter.with_clip_rect(area);
        ruler_ticks(&p, ruler, &geo, tl.total);

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

        // Script cards; words at their times when there is room.
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
            let line_words: Vec<&Word> = words.iter().filter(|w| w.block == i).collect();
            // Word mode once words are a few pixels apart: a tick per word and the words
            // that fit without overlapping.
            let per_word = line_words
                .first()
                .map_or(0.0, |w| geo.x(w.end) - geo.x(w.start));
            if per_word >= 8.0 {
                let ty = card_frame(&p, r, *kind);
                let c = p.with_clip_rect(r.shrink(2.0).intersect(area));
                // Two rows of words, so more of them fit.
                let mut row_end = [f32::MIN; 2];
                for w in line_words {
                    let x = geo.x(w.start);
                    c.line_segment(
                        [pos2(x, r.bottom() - 10.0), pos2(x, r.bottom() - 3.0)],
                        Stroke::new(1.0, theme::tint(theme::BORDER_STRONG, 0.9)),
                    );
                    let Some(row) = (0..2).find(|&k| x >= row_end[k]) else {
                        continue;
                    };
                    let word = w
                        .text
                        .trim_matches(|ch| matches!(ch, '*' | '+' | '~' | '`'));
                    let tr = c.text(
                        pos2(x + 2.0, ty + 2.0 + row as f32 * 16.0),
                        Align2::LEFT_TOP,
                        word,
                        FontId::proportional(11.5),
                        if row == 0 { theme::TEXT } else { theme::TEXT_2 },
                    );
                    row_end[row] = tr.right() + 3.0;
                }
            } else {
                script_card(&p, r, *kind, segments);
            }
            ui.interact(
                r.intersect(area),
                ui.id().with(("sfx-card", i)),
                Sense::hover(),
            )
            .on_hover_text(format!(
                "{}–{}  {}",
                mmss(span.start),
                mmss(span.end),
                plain_text(segments)
            ));
        }

        // Cuts: a line through the script, labelled with its transition.
        let all_cuts = cuts::cuts(parsed, tl);
        let mut label_end = f32::MIN;
        for c in &all_cuts {
            let x = geo.x(c.at);
            if !(area.left()..=area.right()).contains(&x) {
                continue;
            }
            let color = if c.cue.is_some() {
                theme::CUE_TRANSITION
            } else {
                theme::tint(theme::TEXT_MUTED, 0.8)
            };
            p.line_segment(
                [pos2(x, script_lane.top()), pos2(x, cuts_lane.bottom())],
                Stroke::new(if c.cue.is_some() { 1.5 } else { 1.0 }, color),
            );
            if x + 3.0 < label_end {
                continue;
            }
            let g = p.layout_no_wrap(c.label(), FontId::monospace(10.0), color);
            label_end = x + g.size().x + 14.0;
            let tag = Rect::from_min_size(
                pos2(x + 3.0, cuts_lane.center().y - g.size().y / 2.0 - 1.0),
                g.size() + vec2(8.0, 2.0),
            );
            p.rect_filled(tag, 3.0, theme::tint(color, 0.14));
            p.galley(tag.min + vec2(4.0, 1.0), g, color);
        }

        // Music, with its hit points.
        for m in &tl.music {
            let r = geo.rect(m.span, music.y_range(), 2.0);
            if r.width() < 1.0 {
                continue;
            }
            p.rect(
                r,
                CornerRadius::same(4),
                theme::tint(theme::CUE_MUSIC, 0.12),
                Stroke::new(1.0, theme::tint(theme::CUE_MUSIC, 0.6)),
                StrokeKind::Inside,
            );
            let lx = r
                .left()
                .max(area.left())
                .min(r.right() - 20.0)
                .max(r.left());
            p.with_clip_rect(r.shrink(1.0).intersect(area)).text(
                pos2(lx + 6.0, r.top() + 9.0),
                Align2::LEFT_CENTER,
                m.track().unwrap_or("music"),
                FontId::proportional(11.5),
                theme::TEXT_2,
            );
        }
        for hit in music_hits {
            let x = geo.x(hit.at);
            if !(area.left()..=area.right()).contains(&x) {
                continue;
            }
            p.line_segment(
                [pos2(x, music.top() + 2.0), pos2(x, sfx_lane.bottom())],
                Stroke::new(1.0, theme::tint(theme::CUE_MUSIC, 0.7)),
            );
            p.text(
                pos2(x + 3.0, music.bottom() - 3.0),
                Align2::LEFT_BOTTOM,
                &hit.name,
                FontId::monospace(10.0),
                theme::CUE_MUSIC,
            );
        }

        // Sound effects: diamonds, labels in up to three rows.
        let mut row_end = [f32::MIN; 3];
        let mut clicked: Option<String> = None;
        for s in all {
            let x = geo.x(s.at);
            if !(area.left() - 8.0..=area.right() + 8.0).contains(&x) {
                continue;
            }
            let key = cue_key(&s.cue);
            let selected = self.selected.as_deref() == Some(key.as_str());
            let known = s.cue.label.as_deref().and_then(|l| resolve(catalog, l));
            let color = if known.is_some() {
                theme::CUE_SFX
            } else {
                theme::WARN
            };
            let row = (0..3).find(|&k| x > row_end[k]).unwrap_or(2);
            let cy = sfx_lane.top() + 12.0 + row as f32 * 22.0;
            let c = pos2(x, cy);
            p.line_segment(
                [pos2(x, sfx_lane.top()), pos2(x, cy)],
                Stroke::new(1.0, theme::tint(color, 0.5)),
            );
            if selected {
                p.circle_stroke(c, 10.0, Stroke::new(1.5, theme::ACCENT));
            }
            diamond(&p, c, 6.0, color);
            let tr = p.text(
                c + vec2(10.0, 0.0),
                Align2::LEFT_CENTER,
                s.cue.display(),
                FontId::monospace(10.5),
                if selected { theme::ACCENT } else { color },
            );
            row_end[row] = tr.right() + 6.0;
            let hit_r = Rect::from_min_max(pos2(x - 8.0, cy - 9.0), pos2(tr.right(), cy + 9.0));
            let id = egui::Id::new(("sfx-cue", &key));
            let resp = ui.interact(hit_r.intersect(area), id, Sense::click_and_drag());
            if resp.clicked() {
                clicked = Some(key.clone());
            }
            if resp.drag_started() {
                egui::DragAndDrop::set_payload(
                    ui.ctx(),
                    SfxDrag {
                        label: s.label(),
                        moving: Some(key.clone()),
                    },
                );
                clicked = Some(key.clone());
            }
            resp.on_hover_text(format!(
                "{} at {} · {}",
                s.label(),
                mmss_tenths(s.at),
                place_text(parsed, s)
            ))
            .on_hover_cursor(egui::CursorIcon::Grab);
        }
        if let Some(k) = clicked {
            self.selected = Some(k);
        }

        timeline::legend(
            &painter,
            legend,
            &[
                (theme::CUE_SFX, "sound effect"),
                (theme::WARN, "not in catalog"),
                (theme::CUE_TRANSITION, "transition"),
                (theme::tint(theme::TEXT_MUTED, 0.8), "plain cut"),
                (theme::CUE_MUSIC, "music hit"),
            ],
            "scroll to pan · Ctrl+scroll to zoom",
        );

        // Dropping a sound (or moving a cue).
        let payload = egui::DragAndDrop::payload::<SfxDrag>(ui.ctx())?;
        let pointer = ui.ctx().pointer_hover_pos()?;
        if !area.contains(pointer) {
            return None;
        }
        let (target, at, hint) = target_at(parsed, tl, &geo, &all_cuts, words, sec, pointer)?;
        let x = geo.x(at);
        let pa = painter.with_clip_rect(area.expand(2.0));
        pa.line_segment(
            [pos2(x, sec.top()), pos2(x, sfx_lane.bottom())],
            Stroke::new(2.0, theme::ACCENT),
        );
        let label = if payload.moving.is_some() {
            format!("move here: {hint}")
        } else {
            hint
        };
        let g = pa.layout_no_wrap(label, FontId::proportional(12.0), theme::ACCENT_ON);
        // Below the pointer: the dragged card floats above it.
        let tag = Rect::from_min_size(pointer + vec2(14.0, 22.0), g.size() + vec2(12.0, 6.0));
        painter.rect_filled(tag, 4.0, theme::ACCENT);
        painter.galley(tag.min + vec2(6.0, 3.0), g, theme::ACCENT_ON);

        if ui.input(|i| i.pointer.any_released())
            && let Some(d) = egui::DragAndDrop::take_payload::<SfxDrag>(ui.ctx())
        {
            return Some(match &d.moving {
                Some(k) => Edit::Move(k.clone(), target),
                None => Edit::Add(target, d.label.clone()),
            });
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn details(
        &mut self,
        ui: &mut egui::Ui,
        s: &Sfx,
        parsed: &Script,
        tl: &Timeline,
        catalog: &Catalog,
        library: &[(&Entry, f32)],
        edit: &mut Option<Edit>,
    ) {
        let key = cue_key(&s.cue);
        let found = s.cue.label.as_deref().and_then(|l| resolve(catalog, l));
        ui.horizontal(|ui| {
            widgets::chip(
                ui,
                &s.cue.display(),
                if found.is_some() {
                    theme::CUE_SFX
                } else {
                    theme::WARN
                },
            );
            if let Some(id) = &s.cue.id {
                ui.label(mono(format!("#{id}"), theme::TEXT_MUTED));
            }
        });
        ui.add_space(8.0);
        match found {
            Some((e, exact)) => {
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
                if !exact {
                    ui.label(text(
                        format!("matched by tag; write :sfx[{}] to be exact", a.short_name()),
                        11.5,
                        theme::TEXT_MUTED,
                    ));
                }
                let others = s
                    .cue
                    .label
                    .as_deref()
                    .map_or(0, |l| candidates(catalog, l))
                    .saturating_sub(1);
                if others > 0 {
                    ui.label(text(
                        format!(
                            "{others} other sound{} also match{}; write :sfx[{}] to pick this one",
                            if others == 1 { "" } else { "s" },
                            if others == 1 { "es" } else { "" },
                            a.id,
                        ),
                        11.5,
                        theme::WARN,
                    ));
                }
                if let Some(sm) = &a.summary {
                    ui.label(text(sm, 12.0, theme::TEXT_2));
                }
                if let Some(len) = a.audio.as_ref().and_then(|x| x.seconds) {
                    widgets::kv(ui, "length", text(format!("{len:.1}s"), 12.5, theme::TEXT));
                }
            }
            None => {
                ui.label(text(
                    format!("\"{}\" isn't in your catalog.", s.label()),
                    12.5,
                    theme::WARN,
                ));
            }
        }
        ui.add_space(6.0);
        widgets::kv(ui, "at", text(mmss_tenths(s.at), 12.5, theme::TEXT));
        widgets::kv(ui, "placed", text(place_text(parsed, s), 12.5, theme::TEXT));
        if let Some(sec) = tl.section_of(s.block) {
            widgets::kv(ui, "section", text(&sec.title, 12.5, theme::TEXT_2));
        }
        let under: Vec<&str> = tl
            .music
            .iter()
            .filter(|m| m.span.start <= s.at && s.at < m.span.end)
            .filter_map(|m| m.track())
            .collect();
        widgets::kv(
            ui,
            "music",
            text(
                if under.is_empty() {
                    "none".to_owned()
                } else {
                    under.join(", ")
                },
                12.5,
                theme::TEXT_2,
            ),
        );
        for (k, v) in &s.cue.attrs {
            widgets::kv(ui, k, text(v, 12.5, theme::TEXT));
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let left = sfx::nudge(parsed, s, -1);
            let right = sfx::nudge(parsed, s, 1);
            if ui
                .add_enabled(left.is_some(), egui::Button::new("‹ word"))
                .on_hover_text("One word earlier")
                .clicked()
                && let Some(t) = left
            {
                *edit = Some(Edit::Move(key.clone(), t));
            }
            if ui
                .add_enabled(right.is_some(), egui::Button::new("word ›"))
                .on_hover_text("One word later")
                .clicked()
                && let Some(t) = right
            {
                *edit = Some(Edit::Move(key.clone(), t));
            }
            if widgets::button(ui, "Show").clicked() {
                self.view.show_time(s.at);
            }
            if widgets::button(ui, "Remove").clicked() {
                *edit = Some(Edit::Remove(key.clone()));
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.rename)
                    .hint_text("Rename, e.g. impact")
                    .desired_width(170.0),
            );
            if ui
                .add_enabled(!self.rename.trim().is_empty(), egui::Button::new("Rename"))
                .clicked()
            {
                *edit = Some(Edit::Rename(key.clone(), self.rename.trim().to_owned()));
            }
        });

        // Swap for: sounds sharing a tag with this one, else the best fits.
        let tags: Vec<String> = found
            .map(|(e, _)| e.asset.tags.iter().map(|t| t.to_lowercase()).collect())
            .unwrap_or_default();
        let mut picks: Vec<&(&Entry, f32)> = library
            .iter()
            .filter(|(e, _)| found.is_none_or(|(f, _)| f.asset.id != e.asset.id))
            .filter(|(e, _)| {
                tags.is_empty()
                    || e.asset
                        .tags
                        .iter()
                        .any(|t| tags.contains(&t.to_lowercase()))
            })
            .collect();
        if picks.is_empty() {
            picks = library
                .iter()
                .filter(|(e, _)| found.is_none_or(|(f, _)| f.asset.id != e.asset.id))
                .collect();
        }
        picks.truncate(4);
        if !picks.is_empty() {
            ui.add_space(12.0);
            ui.label(strong("Swap for", 12.5));
            ui.add_space(4.0);
            for (e, fit) in picks {
                ui.horizontal(|ui| {
                    ui.label(text(e.asset.short_name(), 12.5, theme::TEXT));
                    if *fit != 0.0 {
                        ui.label(mono(format!("{fit:+.1}"), fit_color(*fit)));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::button(ui, "Use").clicked() {
                            *edit =
                                Some(Edit::Rename(key.clone(), e.asset.short_name().to_owned()));
                        }
                    });
                });
            }
        }
    }

    fn checks_list(&mut self, ui: &mut egui::Ui, issues: &[Check]) {
        ui.label(strong("Checks", 13.5));
        ui.add_space(6.0);
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
                if let Some(k) = &c.select {
                    self.selected = Some(k.clone());
                }
            }
            ui.add_space(5.0);
        }
    }
}

fn diamond(p: &egui::Painter, c: Pos2, r: f32, color: Color32) {
    p.add(egui::Shape::convex_polygon(
        vec![
            c + vec2(0.0, -r),
            c + vec2(r, 0.0),
            c + vec2(0.0, r),
            c + vec2(-r, 0.0),
        ],
        color,
        Stroke::NONE,
    ));
}

fn line_no(parsed: &Script, b: usize) -> usize {
    match &parsed.blocks[b] {
        Block::Heading { line, .. } | Block::Line { line, .. } | Block::Divider { line, .. } => {
            *line
        }
    }
}

fn line_label(parsed: &Script, b: usize) -> String {
    match &parsed.blocks[b] {
        Block::Line { segments, .. } => {
            let t = truncate(plain_text(segments).trim(), 34);
            if t.starts_with(['"', '“', '[']) {
                t
            } else {
                format!("\"{t}\"")
            }
        }
        Block::Heading { text, .. } => text.clone(),
        Block::Divider { .. } => String::new(),
    }
}

/// Where a cue sits, in words.
fn place_text(parsed: &Script, s: &Sfx) -> String {
    match &s.place {
        Place::Word { word, .. } => format!("on \"{word}\" in {}", line_label(parsed, s.block)),
        Place::Cut => format!("on the cut into {}", line_label(parsed, s.block)),
        Place::LineEnd => format!("at the end of {}", line_label(parsed, s.block)),
        Place::OnScreen => format!("as {} starts", line_label(parsed, s.block)),
        Place::Section => format!("as {} starts", line_label(parsed, s.block)),
    }
}

/// Where a drop at `pointer` lands: on a section, on a cut, or at the nearest word boundary
/// of the line under it. Returns the target, its time and a hint.
fn target_at(
    parsed: &Script,
    tl: &Timeline,
    geo: &Geo,
    all_cuts: &[cuts::Cut],
    words: &[Word],
    sec: Rect,
    pointer: Pos2,
) -> Option<(Target, f32, String)> {
    let t = geo.t(pointer.x);
    if sec.y_range().contains(pointer.y) {
        let s = tl
            .sections
            .iter()
            .find(|s| s.span.start <= t && t < s.span.end)?;
        return Some((
            Target::Start {
                line: line_no(parsed, s.block),
            },
            s.span.start,
            format!("as {} starts", s.title),
        ));
    }
    // The nearest cut within reach; on a tie, the earlier one.
    if let Some(c) = all_cuts
        .iter()
        .map(|c| (c, (geo.x(c.at) - pointer.x).abs()))
        .filter(|(_, d)| *d <= CUT_SNAP_PX)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(c, _)| c)
    {
        return Some((
            Target::Start {
                line: line_no(parsed, c.in_block),
            },
            c.at,
            format!("on the cut into {}", line_label(parsed, c.in_block)),
        ));
    }
    // The timed line under the pointer (or the last one before it).
    let b = (0..parsed.blocks.len())
        .filter(|&i| matches!(parsed.blocks[i], Block::Line { .. }) && !tl.blocks[i].is_empty())
        .rfind(|&i| tl.blocks[i].start <= t)?;
    let span = tl.blocks[b];
    let line = line_no(parsed, b);
    let kind = match &parsed.blocks[b] {
        Block::Line { kind, .. } => *kind,
        _ => LineKind::Note,
    };
    if !kind.is_spoken() {
        return Some((
            Target::Start { line },
            span.start,
            format!("as {} starts", line_label(parsed, b)),
        ));
    }
    let ws: Vec<&Word> = words.iter().filter(|w| w.block == b).collect();
    let n = ws.len().max(1);
    let index = (((t - span.start) / span.len().max(0.001)) * n as f32)
        .round()
        .clamp(0.0, n as f32) as usize;
    let at = span.start + span.len() * index as f32 / n as f32;
    let hint = match ws.get(index) {
        Some(_) if index == 0 => format!("on the cut into {}", line_label(parsed, b)),
        Some(w) => format!("on \"{}\"", w.text),
        None => format!("at the end of {}", line_label(parsed, b)),
    };
    Some((Target::Word { line, index }, at, hint))
}

struct Check {
    title: String,
    detail: String,
    color: Color32,
    at: f32,
    select: Option<String>,
}

fn checks(
    parsed: &Script,
    all: &[Sfx],
    words: &[Word],
    music_hits: &[Hit],
    catalog: &Catalog,
) -> Vec<Check> {
    let mut out = Vec::new();
    for (i, s) in all.iter().enumerate() {
        let key = Some(cue_key(&s.cue));
        let when = mmss_tenths(s.at);
        let found = s.cue.label.as_deref().and_then(|l| resolve(catalog, l));
        match found {
            None => out.push(Check {
                title: format!("\"{}\" isn't in the catalog", s.label()),
                detail: format!("{when} · {}", place_text(parsed, s)),
                color: theme::WARN,
                at: s.at,
                select: key.clone(),
            }),
            Some((e, _)) => {
                if e.asset.license_status() != LicenseStatus::Open {
                    out.push(Check {
                        title: format!("Check the license of {}", e.asset.short_name()),
                        detail: if e.asset.license.is_empty() {
                            "no license given".into()
                        } else {
                            e.asset.license.clone()
                        },
                        color: theme::WARN,
                        at: s.at,
                        select: key.clone(),
                    });
                }
                // A long sound under the voice.
                if let Some(len) = e.asset.audio.as_ref().and_then(|a| a.seconds)
                    && len >= 1.0
                {
                    let under: f32 = words
                        .iter()
                        .map(|w| (w.end.min(s.at + len) - w.start.max(s.at)).max(0.0))
                        .sum();
                    if under >= 0.5 {
                        out.push(Check {
                            title: format!("{} ({len:.1}s) plays under VO", s.label()),
                            detail: format!("{when} · {under:.1}s of speech under it"),
                            color: theme::WARN,
                            at: s.at,
                            select: key.clone(),
                        });
                    }
                }
            }
        }
        if let Some(o) = all[i + 1..]
            .iter()
            .find(|o| (o.at - s.at).abs() < CLASH_SECONDS)
        {
            out.push(Check {
                title: format!("{} and {} play at once", s.label(), o.label()),
                detail: format!("{when} · {:.1}s apart", (o.at - s.at).abs()),
                color: theme::WARN,
                at: s.at,
                select: key.clone(),
            });
        }
        if let Some(h) = music_hits
            .iter()
            .find(|h| (h.at - s.at).abs() < HIT_SECONDS)
        {
            out.push(Check {
                title: format!("{} lands on the {} of {}", s.label(), h.name, h.track),
                detail: format!(
                    "{when} · {:.1}s from the hit: line them up or move one",
                    (h.at - s.at).abs()
                ),
                color: theme::WARN,
                at: s.at,
                select: key,
            });
        }
    }
    out
}

fn sound_list(
    ui: &mut egui::Ui,
    library: &[(&Entry, f32)],
    ranked: bool,
    all: &[Sfx],
    catalog: &Catalog,
) {
    ui.horizontal(|ui| {
        ui.label(strong("Sounds", 13.5));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(mono(
                if ranked { "ranked by vision" } else { "A–Z" },
                theme::TEXT_MUTED,
            ));
        });
    });
    ui.add_space(8.0);
    if library.is_empty() {
        ui.label(muted(
            "No sound effects in your catalog yet. Add *.asset.json descriptors with \"kind\": \
             \"sfx\" to a catalog source (Catalog tab), then drag them onto the timeline.",
        ));
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for (e, fit) in library {
                let a = &e.asset;
                let uses = all
                    .iter()
                    .filter(|s| {
                        s.cue
                            .label
                            .as_deref()
                            .and_then(|l| resolve(catalog, l))
                            .is_some_and(|(f, _)| f.asset.id == a.id)
                    })
                    .count();
                widgets::drag_card(
                    ui,
                    egui::Id::new(("sfx-lib", &a.id)),
                    SfxDrag {
                        label: a.short_name().to_owned(),
                        moving: None,
                    },
                    |ui| {
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
                                    let (r, _) =
                                        ui.allocate_exact_size(vec2(12.0, 14.0), Sense::hover());
                                    diamond(ui.painter(), r.center(), 5.0, theme::CUE_SFX);
                                    ui.label(text(a.short_name(), 13.0, theme::TEXT));
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
                                                theme::CUE_SFX,
                                            ));
                                        }
                                    });
                                });
                                let mut meta: Vec<String> = Vec::new();
                                if let Some(len) = a.audio.as_ref().and_then(|x| x.seconds) {
                                    meta.push(format!("{len:.1}s"));
                                }
                                meta.extend(a.tags.iter().take(3).cloned());
                                ui.add(
                                    egui::Label::new(mono(meta.join(" · "), theme::TEXT_MUTED))
                                        .truncate(),
                                );
                            });
                    },
                )
                .response
                .on_hover_text(a.summary.as_deref().unwrap_or("Drag onto the timeline"));
                ui.add_space(2.0);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_snap_to_word_boundaries_and_cuts() {
        let s = spearfia_script::parse("## A\n[ON SCREEN: x]\none two three four\n");
        let tl = timing::timeline(&s);
        let words = sfx::words(&s, &tl);
        let cs = cuts::cuts(&s, &tl);
        let geo = Geo {
            x0: 0.0,
            px: 100.0,
            start: 0.0,
        };
        let sec = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 20.0));
        let line = tl.blocks[2];
        // Just past the second word's start: before word 1 ("two").
        let x = geo.x(line.start + line.len() * 0.27);
        let (t, _, hint) = target_at(&s, &tl, &geo, &cs, &words, sec, pos2(x, 50.0)).unwrap();
        assert_eq!(t, Target::Word { line: 2, index: 1 });
        assert_eq!(hint, "on \"two\"");
        // On the cut between the on-screen line and the spoken one.
        let x = geo.x(line.start) + 3.0;
        let (t, at, _) = target_at(&s, &tl, &geo, &cs, &words, sec, pos2(x, 50.0)).unwrap();
        assert_eq!(t, Target::Start { line: 2 });
        assert!((at - line.start).abs() < 1e-4);
        // In the section lane.
        let (t, _, _) = target_at(&s, &tl, &geo, &cs, &words, sec, pos2(50.0, 10.0)).unwrap();
        assert_eq!(t, Target::Start { line: 0 });
    }

    #[test]
    fn drops_pick_the_nearest_cut() {
        // Four one-word lines at 10 px/s: cuts 0.4 s (4 px) apart, all within reach.
        let s = spearfia_script::parse("one\ntwo\nthree\nfour\n");
        let tl = timing::timeline(&s);
        let words = sfx::words(&s, &tl);
        let cs = cuts::cuts(&s, &tl);
        let geo = Geo {
            x0: 0.0,
            px: 10.0,
            start: 0.0,
        };
        let sec = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 20.0));
        let (t, at, _) =
            target_at(&s, &tl, &geo, &cs, &words, sec, pos2(geo.x(1.2), 50.0)).unwrap();
        assert!((at - 1.2).abs() < 1e-4, "{at}");
        assert_eq!(t, Target::Start { line: 3 });
        // Halfway between two cuts: the earlier one.
        let (_, at, _) =
            target_at(&s, &tl, &geo, &cs, &words, sec, pos2(geo.x(1.0), 50.0)).unwrap();
        assert!((at - 0.8).abs() < 1e-4, "{at}");
    }

    #[test]
    fn labels_match_whole_tags_only() {
        let json = |id: &str, tags: &str| {
            format!(
                r#"{{"id": "{id}", "name": "{id}", "kind": "sfx", "license": "CC0-1.0",
                "source": {{ "type": "local" }}, "hash": "sha256:00", "tags": {tags}}}"#
            )
        };
        let entry = |id: &str, tags: &str| Entry {
            asset: serde_json::from_str(&json(id, tags)).unwrap(),
            descriptor: Default::default(),
            source: "t".into(),
        };
        let c = Catalog {
            entries: vec![
                entry("spearfia/boom-impact@0.1.0", r#"["boom"]"#),
                entry("spearfia/big-boom@0.1.0", r#"["boom", "big boom"]"#),
            ],
            errors: vec![],
        };
        assert!(resolve(&c, "boom nonexistent").is_none());
        let (e, exact) = resolve(&c, "big boom").unwrap();
        assert!(exact, "the name wins over a tag");
        assert_eq!(e.asset.short_name(), "big-boom");
        let (e, exact) = resolve(&c, "BOOM").unwrap();
        assert!(!exact);
        assert_eq!(e.asset.short_name(), "boom-impact");
        assert_eq!(candidates(&c, "boom"), 2);
    }
}
