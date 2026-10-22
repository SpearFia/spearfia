//! Transitions stage: every cut between script lines, with the shot before and after it,
//! the transition used and where it came from (a divider line, a cue on the outgoing line,
//! or nothing: a plain cut).
//!
//! - Middle: the cuts, filterable (all, section changes, within sections, unset). A
//!   section change with no transition is "unset" and shows suggestions.
//! - Right: the selected cut (before / after, source, timing, duration) and the catalog's
//!   transitions ranked by the project's vision. Click "Use" or drag one onto a cut.
//!
//! Every change is a text edit to `script.md`.

use crate::icons::{self, Icon};
use crate::screens::script_stage::ScriptStage;
use crate::script_view::{self, Resolved};
use crate::theme;
use crate::widgets::{self, mmss, mono, muted, panel, strong, text};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Panel, Rect, Sense, Stroke, pos2,
    vec2,
};
use spearfia_catalog::{AssetKind, Catalog, Entry};
use spearfia_core::links::{LinkKind, Links};
use spearfia_core::project::Project;
use spearfia_script::cuts::{self, Cut, Source};
use spearfia_script::{Block, LineKind, Script, Segment, timing};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Sections,
    Within,
    Unset,
}

/// A transition dragged from the library.
#[derive(Debug, Clone)]
struct TransitionDrag {
    label: String,
}

enum Edit {
    Set(usize, String),
    Clear(usize),
    Duration(usize, Option<f32>),
    /// Use this transition at every unset section change.
    ApplyToUnset(String),
}

pub struct TransitionsStage {
    filter: Filter,
    /// Selected cut, by the source line of its outgoing line (stable across edits above it
    /// only roughly, which is fine for a selection).
    selected: Option<usize>,
    custom: String,
    error: Option<String>,
}

impl Default for TransitionsStage {
    fn default() -> Self {
        Self::new()
    }
}

impl TransitionsStage {
    pub fn new() -> Self {
        Self {
            filter: Filter::All,
            selected: None,
            custom: String::new(),
            error: None,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        project: &Project,
        script: &mut ScriptStage,
        links: &Links,
        catalog: &Catalog,
    ) {
        let parsed = spearfia_script::parse(script.source());
        let tl = timing::timeline(&parsed);
        let all = cuts::cuts(&parsed, &tl);
        let resolved = script_view::resolve(&parsed, links);
        let weights = project
            .vision_preset()
            .and_then(|v| spearfia_vision::preset(&v))
            .map(|p| p.weight_map())
            .unwrap_or_default();
        let mut library: Vec<(&Entry, f32)> = catalog
            .entries
            .iter()
            .filter(|e| e.asset.kind == AssetKind::Transition)
            .map(|e| (e, e.asset.fit(&weights)))
            .collect();
        library.sort_by(|a, b| b.1.total_cmp(&a.1));
        let ranked = !weights.is_empty();

        let line_of = |b: usize| match &parsed.blocks[b] {
            Block::Line { line, .. } => *line,
            _ => 0,
        };
        let sel_idx = self
            .selected
            .and_then(|l| all.iter().position(|c| line_of(c.out_block) == l))
            .or_else(|| (!all.is_empty()).then_some(0));

        let mut edit: Option<Edit> = None;

        Panel::right("transitions-inspector")
            .exact_size(330.0)
            .frame(panel(theme::BG_PANEL).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        if let Some(i) = sel_idx {
                            self.inspector(ui, i, &all[i], &parsed, &tl, &resolved, &mut edit);
                            ui.add_space(14.0);
                            widgets::rule(ui);
                            ui.add_space(10.0);
                        }
                        self.library(
                            ui,
                            &library,
                            ranked,
                            sel_idx.map(|i| &all[i]),
                            &all,
                            &mut edit,
                        );
                    });
            });

        egui::CentralPanel::default()
            .frame(panel(theme::BG_APP).inner_margin(Margin::symmetric(22, 16)))
            .show(ui, |ui| {
                self.header(ui, &all);
                ui.add_space(10.0);
                let shown: Vec<usize> = (0..all.len())
                    .filter(|&i| match self.filter {
                        Filter::All => true,
                        Filter::Sections => all[i].section_change,
                        Filter::Within => !all[i].section_change,
                        Filter::Unset => all[i].unset(),
                    })
                    .collect();
                column_titles(ui);
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    if shown.is_empty() {
                        ui.add_space(20.0);
                        ui.label(muted(match self.filter {
                            Filter::Unset => "Every section change has a transition.",
                            _ if all.is_empty() => "No cuts yet: the script needs at least two lines that take time.",
                            _ => "Nothing here.",
                        }));
                    }
                    for i in shown {
                        let c = &all[i];
                        let suggestions: Vec<(&str, f32)> = if c.unset() {
                            library.iter().take(3).map(|(e, f)| (e.asset.short_name(), *f)).collect()
                        } else {
                            Vec::new()
                        };
                        let (clicked, ev) = row(ui, i, c, &parsed, &tl, &resolved, sel_idx == Some(i), &suggestions);
                        if clicked {
                            self.selected = Some(line_of(c.out_block));
                        }
                        if ev.is_some() {
                            self.selected = Some(line_of(c.out_block));
                            edit = ev;
                        }
                    }
                    ui.add_space(30.0);
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
        let all = cuts::cuts(&parsed, &tl);
        let out = match e {
            Edit::Set(i, label) => all.get(i).and_then(|c| cuts::set(&src, &parsed, c, &label)),
            Edit::Clear(i) => all.get(i).and_then(|c| cuts::clear(&src, &parsed, c)),
            Edit::Duration(i, d) => all.get(i).and_then(|c| cuts::set_duration(&src, c, d)),
            Edit::ApplyToUnset(label) => {
                // One at a time from the end, so earlier cuts' positions don't move.
                let mut s = src.clone();
                let n = all.len();
                for k in (0..n).rev() {
                    let p = spearfia_script::parse(&s);
                    let cs = cuts::cuts(&p, &timing::timeline(&p));
                    if let Some(c) = cs.get(k).filter(|c| c.unset())
                        && let Some(next) = cuts::set(&s, &p, c, &label)
                    {
                        s = next;
                    }
                }
                Some(s)
            }
        };
        let Some(out) = out else {
            self.error = Some("Couldn't change that transition.".into());
            return;
        };
        if out != src {
            match script.apply_edit(project, out) {
                Ok(()) => self.error = None,
                Err(e) => self.error = Some(e),
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, all: &[Cut]) {
        ui.horizontal(|ui| {
            ui.label(strong("Transitions", 16.0));
            ui.label(muted("every cut between script lines and sections"));
            if let Some(e) = &self.error {
                ui.label(text(e, 12.0, theme::DANGER));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let count = |f: Filter| {
                    all.iter()
                        .filter(|c| match f {
                            Filter::All => true,
                            Filter::Sections => c.section_change,
                            Filter::Within => !c.section_change,
                            Filter::Unset => c.unset(),
                        })
                        .count()
                };
                for (f, name) in [
                    (Filter::Unset, "Unset"),
                    (Filter::Within, "Within sections"),
                    (Filter::Sections, "Section changes"),
                    (Filter::All, "All"),
                ] {
                    let n = count(f);
                    let on = self.filter == f;
                    let color = if f == Filter::Unset && n > 0 {
                        theme::WARN
                    } else if on {
                        theme::TEXT
                    } else {
                        theme::TEXT_2
                    };
                    let r = egui::Frame::new()
                        .fill(if on {
                            theme::BG_HOVER
                        } else {
                            Color32::TRANSPARENT
                        })
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.label(text(format!("{name} · {n}"), 12.5, color))
                        })
                        .response
                        .interact(Sense::click());
                    if r.clicked() {
                        self.filter = f;
                    }
                }
            });
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn inspector(
        &mut self,
        ui: &mut egui::Ui,
        i: usize,
        c: &Cut,
        parsed: &Script,
        tl: &timing::Timeline,
        resolved: &Resolved,
        edit: &mut Option<Edit>,
    ) {
        ui.horizontal(|ui| {
            transition_chip(ui, c);
            if let Some(id) = c.cue.as_ref().and_then(|d| d.id.as_ref()) {
                ui.label(mono(format!("#{id}"), theme::TEXT_MUTED));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(mono(format!("cut {:02}", i + 1), theme::TEXT_MUTED));
            });
        });
        ui.add_space(8.0);
        // Before | after.
        let w = (ui.available_width() - 8.0) / 2.0;
        ui.horizontal(|ui| {
            for (b, label) in [(c.out_block, "BEFORE"), (c.in_block, "AFTER")] {
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.label(mono(label, theme::TEXT_MUTED));
                    let (r, _) = ui.allocate_exact_size(vec2(w, w * 9.0 / 16.0), Sense::hover());
                    thumb(ui.painter(), r, parsed, b, resolved);
                    ui.add(
                        egui::Label::new(text(
                            short(&line_text(parsed, b), 70),
                            11.5,
                            theme::TEXT_2,
                        ))
                        .wrap(),
                    );
                });
            }
        });
        ui.add_space(8.0);
        let from = match &c.source {
            Source::Divider(_) => format!(
                "script divider{}",
                c.out_section
                    .as_deref()
                    .map(|s| format!(" in {s}"))
                    .unwrap_or_default()
            ),
            Source::LineCue => "cue at the end of the outgoing line".into(),
            Source::Default => "nothing written: a plain cut".into(),
        };
        widgets::kv(ui, "from", text(from, 12.5, theme::TEXT));
        widgets::kv(ui, "at", text(mmss_tenths(c.at), 12.5, theme::TEXT));
        if c.section_change {
            widgets::kv(
                ui,
                "sections",
                text(
                    format!(
                        "{} › {}",
                        c.out_section.as_deref().unwrap_or("start"),
                        c.in_section.as_deref().unwrap_or("?")
                    ),
                    12.5,
                    theme::TEXT,
                ),
            );
        }
        let music: Vec<String> = tl
            .music_in(timing::Span {
                start: c.at - 0.01,
                end: c.at + 0.01,
            })
            .map(|m| m.track().unwrap_or("untitled music").to_owned())
            .collect();
        widgets::kv(
            ui,
            "music",
            text(
                if music.is_empty() {
                    "none under this cut".into()
                } else {
                    music.join(", ")
                },
                12.5,
                theme::TEXT,
            ),
        );
        if c.cue.is_some() {
            let d = c.duration().unwrap_or(0.0);
            ui.horizontal(|ui| {
                ui.add_sized(
                    [92.0, 16.0],
                    egui::Label::new(mono("duration", theme::TEXT_MUTED)),
                );
                let committed = widgets::drag_value(ui, ("duration", i), d, |v| {
                    egui::DragValue::new(v)
                        .range(0.0..=5.0)
                        .speed(0.02)
                        .fixed_decimals(2)
                        .suffix(" s")
                });
                if let Some(d) = committed {
                    *edit = Some(Edit::Duration(i, (d > 0.0).then_some(d)));
                }
                if c.duration().is_some() && ui.small_button("clear").clicked() {
                    *edit = Some(Edit::Duration(i, None));
                }
            });
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if c.cue.is_some() && widgets::button(ui, "Make it a plain cut").clicked() {
                *edit = Some(Edit::Clear(i));
            }
        });
        ui.add_space(4.0);
        ui.label(muted(
            "Previews render in FilmCraft once the timeline is pushed from Sync.",
        ));
    }

    fn library(
        &mut self,
        ui: &mut egui::Ui,
        library: &[(&Entry, f32)],
        ranked: bool,
        selected: Option<&Cut>,
        all: &[Cut],
        edit: &mut Option<Edit>,
    ) {
        let sel_i = selected.and_then(|s| all.iter().position(|c| std::ptr::eq(c, s)));
        ui.horizontal(|ui| {
            ui.label(strong("Library", 13.5));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(mono(
                    if ranked { "ranked by vision" } else { "A–Z" },
                    theme::TEXT_MUTED,
                ));
            });
        });
        ui.add_space(6.0);
        if library.is_empty() {
            ui.label(muted("No transitions in the catalog yet. Add *.asset.json with \"kind\": \"transition\", or type a name below."));
        }
        for (e, fit) in library {
            let name = e.asset.short_name().to_owned();
            let payload = TransitionDrag {
                label: name.clone(),
            };
            widgets::drag_card(
                ui,
                egui::Id::new(("transition-lib", &e.asset.id)),
                payload,
                |ui| {
                    egui::Frame::new()
                        .fill(theme::BG_RAISED)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let (r, _) =
                                    ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                                icons::paint(
                                    ui.painter(),
                                    r,
                                    Icon::Transition,
                                    theme::CUE_TRANSITION,
                                );
                                ui.label(text(&name, 13.0, theme::TEXT));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if let Some(i) = sel_i
                                        && ui.small_button("Use").clicked()
                                    {
                                        *edit = Some(Edit::Set(i, name.clone()));
                                    }
                                    if ranked {
                                        ui.label(mono(format!("{fit:+.1}"), fit_color(*fit)));
                                    }
                                });
                            });
                            if let Some(s) = &e.asset.summary {
                                ui.label(text(s, 11.5, theme::TEXT_MUTED));
                            }
                        });
                },
            )
            .response
            .on_hover_text("Drag onto a cut, or select a cut and click Use");
            ui.add_space(3.0);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.custom)
                    .hint_text("Other, e.g. dissolve")
                    .desired_width(170.0),
            );
            if let Some(i) = sel_i
                && ui
                    .add_enabled(!self.custom.trim().is_empty(), egui::Button::new("Use"))
                    .clicked()
            {
                *edit = Some(Edit::Set(i, self.custom.trim().to_owned()));
            }
        });
        let unset = all.iter().filter(|c| c.unset()).count();
        if let Some(c) = selected
            && c.cue.is_some()
            && unset > 0
        {
            ui.add_space(10.0);
            if widgets::button(
                ui,
                &format!(
                    "Apply {} to all {unset} unset section change{}",
                    c.label(),
                    if unset == 1 { "" } else { "s" }
                ),
            )
            .clicked()
            {
                *edit = Some(Edit::ApplyToUnset(c.label()));
            }
        }
    }
}

fn column_titles(ui: &mut egui::Ui) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 18.0), Sense::hover());
    let cols = Cols::new(r);
    let p = ui.painter();
    for (x, t) in [
        (r.left() + 12.0, "#"),
        (cols.out.left(), "OUTGOING"),
        (cols.mid.center().x, "TRANSITION"),
        (cols.inc.left(), "INCOMING"),
    ] {
        let align = if t == "TRANSITION" {
            Align2::CENTER_CENTER
        } else {
            Align2::LEFT_CENTER
        };
        p.text(
            pos2(x, r.center().y),
            align,
            t,
            FontId::monospace(10.5),
            theme::TEXT_MUTED,
        );
    }
}

/// Column rectangles of a row.
struct Cols {
    num: Rect,
    out: Rect,
    mid: Rect,
    inc: Rect,
}

impl Cols {
    fn new(r: Rect) -> Self {
        let num_w = 44.0;
        let mid_w = (r.width() * 0.3).clamp(220.0, 320.0);
        let side = (r.width() - num_w - mid_w) / 2.0;
        let x0 = r.left();
        Self {
            num: Rect::from_min_size(pos2(x0, r.top()), vec2(num_w, r.height())),
            out: Rect::from_min_size(pos2(x0 + num_w, r.top()), vec2(side, r.height())),
            mid: Rect::from_min_size(pos2(x0 + num_w + side, r.top()), vec2(mid_w, r.height())),
            inc: Rect::from_min_size(
                pos2(x0 + num_w + side + mid_w, r.top()),
                vec2(side, r.height()),
            ),
        }
    }
}

/// One cut. Returns (row clicked, edit from a suggestion click or a drop).
#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut egui::Ui,
    i: usize,
    c: &Cut,
    parsed: &Script,
    tl: &timing::Timeline,
    resolved: &Resolved,
    selected: bool,
    suggestions: &[(&str, f32)],
) -> (bool, Option<Edit>) {
    let w = ui.available_width();
    let h = if c.unset() { 92.0 } else { 78.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    ui.add_space(8.0);
    let p = ui.painter().clone();
    let dragging = egui::DragAndDrop::has_payload_of_type::<TransitionDrag>(ui.ctx());
    let hovered_drop = dragging
        && ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|pt| r.contains(pt));
    let stroke = if hovered_drop || selected {
        Stroke::new(if hovered_drop { 2.0 } else { 1.0 }, theme::ACCENT)
    } else {
        Stroke::new(1.0, theme::BORDER)
    };
    p.rect(
        r,
        CornerRadius::same(7),
        if selected {
            theme::BG_RAISED
        } else {
            theme::BG_PANEL
        },
        if c.unset() && !selected && !hovered_drop {
            Stroke::NONE
        } else {
            stroke
        },
        egui::StrokeKind::Inside,
    );
    if c.unset() && !selected && !hovered_drop {
        dashed_rect(
            &p,
            r.shrink(0.5),
            Stroke::new(1.0, theme::tint(theme::WARN, 0.7)),
        );
    }
    let cols = Cols::new(r);
    p.text(
        cols.num.left_center() + vec2(14.0, 0.0),
        Align2::LEFT_CENTER,
        format!("{:02}", i + 1),
        FontId::monospace(12.0),
        theme::TEXT_2,
    );

    // Outgoing and incoming.
    for (col, b) in [(cols.out, c.out_block), (cols.inc, c.in_block)] {
        let th = Rect::from_min_size(pos2(col.left(), col.center().y - 27.0), vec2(96.0, 54.0));
        thumb(&p, th, parsed, b, resolved);
        let tx = th.right() + 12.0;
        let tw = (col.right() - tx - 14.0).max(40.0);
        let span = tl.blocks[b];
        let section = tl
            .section_of(b)
            .map(|s| s.title.to_uppercase())
            .unwrap_or_default();
        p.text(
            pos2(tx, col.center().y - 20.0),
            Align2::LEFT_CENTER,
            format!("{} · {}", short(&section, 26), mmss(span.start)),
            FontId::monospace(10.5),
            theme::TEXT_MUTED,
        );
        let kind = match &parsed.blocks[b] {
            Block::Line { kind, .. } => *kind,
            _ => LineKind::Spoken,
        };
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = tw;
        job.wrap.max_rows = 2;
        job.append(
            &line_text(parsed, b),
            0.0,
            egui::TextFormat {
                font_id: FontId::new(
                    12.5,
                    egui::FontFamily::Name(
                        if kind == LineKind::OnScreen {
                            theme::DOC_ITALIC
                        } else {
                            theme::DOC
                        }
                        .into(),
                    ),
                ),
                color: if kind == LineKind::OnScreen {
                    theme::TEXT_2
                } else {
                    theme::TEXT
                },
                ..Default::default()
            },
        );
        let g = p.layout_job(job);
        p.galley(pos2(tx, col.center().y - 10.0), g, theme::TEXT);
    }

    // The transition.
    let mut ev = None;
    let mid = cols.mid;
    let mut y = mid.center().y - if c.unset() { 30.0 } else { 16.0 };
    if c.section_change {
        p.text(
            pos2(mid.center().x, y - 4.0),
            Align2::CENTER_CENTER,
            format!(
                "{} › {}",
                short(c.out_section.as_deref().unwrap_or("start"), 18),
                short(c.in_section.as_deref().unwrap_or("?"), 18)
            ),
            FontId::monospace(10.0),
            theme::TEXT_MUTED,
        );
        y += 10.0;
    }
    let label = match (c.unset(), c.duration()) {
        (true, _) => "choose…".to_owned(),
        (false, Some(d)) => format!("{} {}s", c.label(), trim_num(d)),
        (false, None) => c.label(),
    };
    let color = if c.unset() {
        theme::WARN
    } else if c.cue.is_some() {
        theme::CUE_TRANSITION
    } else {
        theme::TEXT_2
    };
    let g = p.layout_no_wrap(label, FontId::proportional(12.5), color);
    let chip = Rect::from_center_size(
        pos2(mid.center().x, y + 10.0),
        g.size() + vec2(if c.unset() { 16.0 } else { 34.0 }, 10.0),
    );
    if c.unset() {
        dashed_rect(&p, chip, Stroke::new(1.0, theme::tint(theme::WARN, 0.8)));
        p.galley(
            pos2(chip.center().x - g.size().x / 2.0, chip.top() + 5.0),
            g,
            color,
        );
    } else {
        p.rect(
            chip,
            5.0,
            theme::tint(color, if c.cue.is_some() { 0.14 } else { 0.06 }),
            Stroke::new(1.0, theme::tint(color, 0.5)),
            egui::StrokeKind::Inside,
        );
        icons::paint(
            &p,
            Rect::from_center_size(pos2(chip.left() + 13.0, chip.center().y), vec2(12.0, 12.0)),
            Icon::Transition,
            color,
        );
        p.galley(pos2(chip.left() + 24.0, chip.top() + 5.0), g, color);
    }
    let source = match &c.source {
        Source::Divider(_) => "script divider",
        Source::LineCue => "cue on the line",
        Source::Default if c.unset() => "suggested for this vision",
        Source::Default => "default cut",
    };
    p.text(
        pos2(mid.center().x, chip.bottom() + 11.0),
        Align2::CENTER_CENTER,
        source,
        FontId::monospace(10.0),
        if c.unset() {
            theme::WARN
        } else {
            theme::TEXT_MUTED
        },
    );

    // Suggestions on unset cuts: click to use.
    if c.unset() && !suggestions.is_empty() {
        let mut items: Vec<(String, Option<&str>)> = suggestions
            .iter()
            .map(|(n, f)| (format!("{n} {f:+.1}"), Some(*n)))
            .collect();
        items.push(("cut".into(), None));
        let gs: Vec<_> = items
            .iter()
            .map(|(t, _)| p.layout_no_wrap(t.clone(), FontId::proportional(11.5), theme::TEXT_2))
            .collect();
        let total: f32 =
            gs.iter().map(|g| g.size().x + 14.0).sum::<f32>() + 6.0 * (gs.len() as f32 - 1.0);
        let mut x = mid.center().x - total / 2.0;
        let ty = chip.bottom() + 22.0;
        for ((_, name), g) in items.iter().zip(gs) {
            let tr = Rect::from_min_size(pos2(x, ty), g.size() + vec2(14.0, 6.0));
            let hit = ui.interact(tr, ui.id().with(("suggest", i, x as i32)), Sense::click());
            p.rect(
                tr,
                4.0,
                if hit.hovered() {
                    theme::BG_HOVER
                } else {
                    Color32::TRANSPARENT
                },
                Stroke::new(1.0, theme::BORDER_STRONG),
                egui::StrokeKind::Inside,
            );
            p.galley(tr.min + vec2(7.0, 3.0), g, theme::TEXT_2);
            if hit.clicked() {
                ev = Some(match name {
                    Some(n) => Edit::Set(i, (*n).to_owned()),
                    None => Edit::Set(i, "cut".to_owned()),
                });
            }
            x = tr.right() + 6.0;
        }
    }

    // Dropping a transition from the library.
    if hovered_drop
        && ui.input(|inp| inp.pointer.any_released())
        && let Some(d) = egui::DragAndDrop::take_payload::<TransitionDrag>(ui.ctx())
    {
        ev = Some(Edit::Set(i, d.label.clone()));
    }
    (resp.clicked(), ev)
}

fn transition_chip(ui: &mut egui::Ui, c: &Cut) {
    let label = match c.duration() {
        Some(d) => format!("{} {}s", c.label(), trim_num(d)),
        None => c.label(),
    };
    widgets::chip(
        ui,
        &label,
        if c.cue.is_some() {
            theme::CUE_TRANSITION
        } else {
            theme::TEXT_2
        },
    );
}

/// A shot placeholder: the linked clip's name in its role color, or "no clip".
pub(crate) fn thumb(
    p: &egui::Painter,
    r: Rect,
    parsed: &Script,
    block: usize,
    resolved: &Resolved,
) {
    let picture = resolved
        .spans
        .get(&block)
        .and_then(|v| v.iter().find(|s| s.kind == LinkKind::Picture));
    let audio = resolved
        .spans
        .get(&block)
        .and_then(|v| v.iter().find(|s| s.kind == LinkKind::Audio));
    match picture {
        Some(s) => {
            let c = script_view::role_color(s.role);
            p.rect(
                r,
                4.0,
                theme::tint(c, 0.18),
                Stroke::new(1.0, theme::tint(c, 0.4)),
                egui::StrokeKind::Inside,
            );
            let cc = r.center() - vec2(0.0, 6.0);
            p.add(egui::Shape::convex_polygon(
                vec![
                    cc + vec2(-5.0, -7.0),
                    cc + vec2(8.0, 0.0),
                    cc + vec2(-5.0, 7.0),
                ],
                theme::tint(c, 0.9),
                Stroke::NONE,
            ));
            let name = short(&s.label, ((r.width() - 8.0) / 6.0) as usize);
            p.text(
                pos2(r.center().x, r.bottom() - 9.0),
                Align2::CENTER_CENTER,
                name,
                FontId::monospace(9.5),
                c,
            );
        }
        None => {
            let onscreen = matches!(
                parsed.blocks.get(block),
                Some(Block::Line {
                    kind: LineKind::OnScreen,
                    ..
                })
            );
            p.rect(
                r,
                4.0,
                theme::BG_INSET,
                Stroke::new(
                    1.0,
                    if onscreen {
                        theme::tint(theme::DANGER, 0.5)
                    } else {
                        theme::BORDER
                    },
                ),
                egui::StrokeKind::Inside,
            );
            p.text(
                r.center(),
                Align2::CENTER_CENTER,
                if audio.is_some() {
                    "VO only"
                } else {
                    "no clip"
                },
                FontId::monospace(10.0),
                if onscreen {
                    theme::tint(theme::DANGER, 0.8)
                } else {
                    theme::TEXT_MUTED
                },
            );
        }
    }
}

pub(crate) fn dashed_rect(p: &egui::Painter, r: Rect, st: Stroke) {
    let pts = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    for w in pts.windows(2) {
        p.extend(egui::Shape::dashed_line(&[w[0], w[1]], st, 5.0, 4.0));
    }
}

/// A line's text without cues and formatting syntax.
pub(crate) fn line_text(parsed: &Script, block: usize) -> String {
    match parsed.blocks.get(block) {
        Some(Block::Line { segments, .. }) => segments
            .iter()
            .filter_map(|s| match s {
                Segment::Text(t) => Some(t.as_str()),
                Segment::Cue(_) => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
            .replace("**", "")
            .replace("++", "")
            .replace("~~", ""),
        _ => String::new(),
    }
}

pub(crate) fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        format!(
            "{}…",
            s.chars().take(n.saturating_sub(1)).collect::<String>()
        )
    }
}

pub(crate) fn trim_num(v: f32) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

pub(crate) fn mmss_tenths(t: f32) -> String {
    let m = (t / 60.0).floor() as u32;
    format!("{m}:{:04.1}", t - m as f32 * 60.0)
}

pub(crate) fn fit_color(fit: f32) -> Color32 {
    if fit > 0.0 {
        theme::OK
    } else if fit < 0.0 {
        theme::DANGER
    } else {
        theme::TEXT_MUTED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_times_and_text() {
        assert_eq!(mmss_tenths(2.6), "0:02.6");
        assert_eq!(mmss_tenths(75.25), "1:15.2");
        assert_eq!(trim_num(0.30), "0.3");
        let s = spearfia_script::parse("**bold** words :sfx[x]{#s1}\n");
        assert_eq!(line_text(&s, 0), "bold words");
        assert_eq!(short("abcdef", 4), "abc…");
    }
}
