//! Effects stage: every line that takes time, with the looks and effects that cover it.
//!
//! - Middle: the script's lines grouped by section. Bars on the left show what each look
//!   covers; chips on the right name the looks on the line (filled when written on that
//!   line, outlined when they come from a heading or an earlier line). On-screen lines with
//!   no look show suggestions.
//! - Right: the selected look (what it covers, its scope, and its settings, taken from the
//!   catalog asset's capabilities when there is one) or the selected line, and the catalog's
//!   effects and templates ranked by the project's vision. Click "Use" or drag one onto a
//!   line or a section heading.
//!
//! Every change is a text edit to `script.md` (`spearfia_script::looks`).

use crate::icons::{self, Icon};
use crate::screens::script_stage::ScriptStage;
use crate::screens::transitions_stage::{
    dashed_rect, fit_color, line_text, mmss_tenths, short, thumb, trim_num,
};
use crate::script_view::{self, Resolved};
use crate::theme;
use crate::widgets::{self, mmss, mono, muted, panel, strong, text};
use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Panel, Rect, Sense, Stroke, pos2,
    vec2,
};
use spearfia_catalog::{AssetKind, Capability, Catalog, Entry};
use spearfia_core::links::Links;
use spearfia_core::project::Project;
use spearfia_script::looks::{self, Look, Scope};
use spearfia_script::{Block, LineKind, Script, edit::next_id, timing};

/// Bars shown at most; more overlapping looks share the last one.
/// Bar columns kept free beside the cards; more are added when more looks overlap.
const LANES: usize = 4;
const LANE_W: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    WithLooks,
    Needs,
    Missing,
}

/// An effect dragged from the library.
#[derive(Debug, Clone)]
struct EffectDrag {
    label: String,
}

enum Edit {
    /// Add a look to the block on this source line.
    Add(usize, String),
    Remove(String),
    Scope(String, Scope),
    Attr(String, String, Option<String>),
}

pub struct EffectsStage {
    filter: Filter,
    /// Selected line or heading, by source line.
    line: Option<usize>,
    /// Selected look, by key (its id).
    look: Option<String>,
    custom: String,
    new_attr: String,
    error: Option<String>,
}

impl Default for EffectsStage {
    fn default() -> Self {
        Self::new()
    }
}

/// A stable name for a look: its id, or label and line.
fn key(l: &Look, parsed: &Script) -> String {
    l.cue
        .id
        .clone()
        .unwrap_or_else(|| format!("{}@{}", l.label(), looks::line_of(&parsed.blocks[l.block])))
}

/// The catalog's effect or template a look names.
fn asset_of<'a>(catalog: &'a Catalog, l: &Look) -> Option<&'a Entry> {
    let label = l.cue.label.as_deref()?;
    catalog
        .find(AssetKind::Effect, label)
        .or_else(|| catalog.find(AssetKind::Template, label))
}

/// The lane (bar column) of each look: the first lane free since its last use. Lanes are
/// never shared by overlapping looks; see [`lane_count`] for how many to draw.
fn lanes(all: &[Look], order: &[usize]) -> Vec<usize> {
    let pos = |b: usize| order.iter().position(|&t| t == b).unwrap_or(usize::MAX);
    let mut ends: Vec<usize> = Vec::new();
    let mut out = vec![0; all.len()];
    let mut idx: Vec<usize> = (0..all.len())
        .filter(|&i| !all[i].covers.is_empty())
        .collect();
    idx.sort_by_key(|&i| pos(all[i].covers[0]));
    for i in idx {
        let a = pos(all[i].covers[0]);
        let z = pos(*all[i].covers.last().unwrap_or(&all[i].covers[0]));
        let lane = match ends.iter().position(|&e| e < a) {
            Some(k) => k,
            None => {
                ends.push(0);
                ends.len() - 1
            }
        };
        ends[lane] = z;
        out[i] = lane;
    }
    out
}

/// Columns to leave for the bars: at least [`LANES`], more when more looks overlap.
fn lane_count(lane_of: &[usize]) -> usize {
    lane_of.iter().map(|l| l + 1).max().unwrap_or(0).max(LANES)
}

/// Suggestions for a line: catalog effects whose name or tags appear in the line, then the
/// best fits for the vision.
fn suggest<'a>(line: &str, library: &[(&'a Entry, f32)]) -> Vec<(&'a str, f32)> {
    let words: Vec<String> = line
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_owned)
        .collect();
    let mut scored: Vec<(&'a str, f32, usize)> = library
        .iter()
        .map(|(e, fit)| {
            let a = &e.asset;
            let mut keys: Vec<String> = a.tags.iter().map(|t| t.to_lowercase()).collect();
            keys.extend(
                a.short_name()
                    .to_lowercase()
                    .split(|c: char| !c.is_alphanumeric())
                    .map(str::to_owned),
            );
            let hits = keys
                .iter()
                .filter(|k| {
                    k.len() > 2 && words.iter().any(|w| w == *k || w.starts_with(k.as_str()))
                })
                .count();
            (a.short_name(), *fit, hits)
        })
        .collect();
    scored.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.total_cmp(&a.1)));
    scored.into_iter().take(3).map(|(n, f, _)| (n, f)).collect()
}

impl EffectsStage {
    pub fn new() -> Self {
        Self {
            filter: Filter::All,
            line: None,
            look: None,
            custom: String::new(),
            new_attr: String::new(),
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
        let all = looks::looks(&parsed, &tl);
        let timed = looks::timed(&parsed, &tl);
        let resolved = script_view::resolve(&parsed, links);
        let weights = project
            .vision_preset()
            .and_then(|v| spearfia_vision::preset(&v))
            .map(|p| p.weight_map())
            .unwrap_or_default();
        let mut library: Vec<(&Entry, f32)> = catalog
            .entries
            .iter()
            .filter(|e| matches!(e.asset.kind, AssetKind::Effect | AssetKind::Template))
            .map(|e| (e, e.asset.fit(&weights)))
            .collect();
        library.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then_with(|| a.0.asset.short_name().cmp(b.0.asset.short_name()))
        });
        let ranked = !weights.is_empty();
        let lane_of = lanes(&all, &timed);
        let n_lanes = lane_count(&lane_of);

        let line_no = |b: usize| looks::line_of(&parsed.blocks[b]);
        let block_at = |l: usize| parsed.blocks.iter().position(|b| looks::line_of(b) == l);
        let sel_look = self
            .look
            .as_ref()
            .and_then(|k| all.iter().position(|l| &key(l, &parsed) == k));
        // The selected block: the chosen line, else the selected look's, else the first line.
        let sel_block = self
            .line
            .and_then(block_at)
            .or_else(|| sel_look.map(|i| all[i].covers.first().copied().unwrap_or(all[i].block)))
            .or_else(|| timed.first().copied());

        let needs = |b: usize| {
            matches!(
                parsed.blocks[b],
                Block::Line {
                    kind: LineKind::OnScreen,
                    ..
                }
            ) && looks::over(&all, b).next().is_none()
        };
        let missing = |l: &Look| asset_of(catalog, l).is_none();

        let mut edit: Option<Edit> = None;

        Panel::right("effects-inspector")
            .exact_size(340.0)
            .frame(panel(theme::BG_PANEL).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        if let Some(i) = sel_look {
                            self.look_inspector(ui, &all[i], &parsed, &tl, catalog, &mut edit);
                        } else if let Some(b) = sel_block {
                            self.line_inspector(ui, b, &parsed, &tl, &all, &resolved, catalog);
                        }
                        ui.add_space(14.0);
                        widgets::rule(ui);
                        ui.add_space(10.0);
                        self.library(ui, &library, ranked, sel_block.map(line_no), &mut edit);
                    });
            });

        egui::CentralPanel::default()
            .frame(panel(theme::BG_APP).inner_margin(Margin::symmetric(22, 16)))
            .show(ui, |ui| {
                let counts = [
                    (Filter::All, timed.len()),
                    (
                        Filter::WithLooks,
                        timed
                            .iter()
                            .filter(|&&b| looks::over(&all, b).next().is_some())
                            .count(),
                    ),
                    (Filter::Needs, timed.iter().filter(|&&b| needs(b)).count()),
                    (Filter::Missing, all.iter().filter(|l| missing(l)).count()),
                ];
                self.header(ui, &counts, all.len());
                ui.add_space(10.0);
                let shows = |b: usize| match self.filter {
                    Filter::All => true,
                    Filter::WithLooks => looks::over(&all, b).next().is_some(),
                    Filter::Needs => needs(b),
                    Filter::Missing => looks::over(&all, b).any(missing),
                };
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        if timed.is_empty() {
                            ui.add_space(20.0);
                            ui.label(muted("No lines yet: write the script first."));
                        }
                        let mut any = false;
                        let mut heading: Option<usize> = None;
                        let mut heading_shown = None;
                        for (n, &b) in timed.iter().enumerate() {
                            // The section heading above this line.
                            let h = looks::section_heading(&parsed, b);
                            if h != heading {
                                heading = h;
                            }
                            if !shows(b) {
                                continue;
                            }
                            any = true;
                            if heading_shown != Some(heading)
                                && let Some(h) = heading
                            {
                                heading_shown = Some(heading);
                                let section_looks: Vec<(usize, &Look)> = all
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, l)| l.block == h)
                                    .collect();
                                let (clicked, ev, chip) = section_row(
                                    ui,
                                    &parsed,
                                    &tl,
                                    h,
                                    &section_looks,
                                    sel_block == Some(h),
                                    sel_look,
                                    catalog,
                                );
                                if clicked {
                                    self.line = Some(line_no(h));
                                    self.look = None;
                                }
                                if let Some(i) = chip {
                                    self.look = Some(key(&all[i], &parsed));
                                    self.line = None;
                                }
                                if ev.is_some() {
                                    edit = ev;
                                }
                            } else if heading_shown.is_none() && heading.is_none() {
                                heading_shown = Some(None);
                            }
                            let here: Vec<(usize, &Look)> = all
                                .iter()
                                .enumerate()
                                .filter(|(_, l)| l.covers.contains(&b))
                                .collect();
                            let suggestions = if needs(b) {
                                suggest(&line_text(&parsed, b), &library)
                            } else {
                                Vec::new()
                            };
                            let out = line_row(
                                ui,
                                RowArgs {
                                    n,
                                    block: b,
                                    parsed: &parsed,
                                    tl: &tl,
                                    resolved: &resolved,
                                    here: &here,
                                    lane_of: &lane_of,
                                    n_lanes,
                                    timed: &timed,
                                    selected: sel_block == Some(b) && sel_look.is_none(),
                                    sel_look,
                                    needs: needs(b),
                                    suggestions: &suggestions,
                                    catalog,
                                },
                            );
                            if out.clicked {
                                self.line = Some(line_no(b));
                                self.look = None;
                            }
                            if let Some(i) = out.chip {
                                self.look = Some(key(&all[i], &parsed));
                                self.line = None;
                            }
                            if out.edit.is_some() {
                                edit = out.edit;
                            }
                        }
                        if !any && !timed.is_empty() {
                            ui.add_space(20.0);
                            ui.label(muted(match self.filter {
                                Filter::Needs => "Every on-screen line has a look.",
                                Filter::Missing => "Every look is in the catalog.",
                                _ => "Nothing here.",
                            }));
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
        let all = looks::looks(&parsed, &tl);
        let find = |k: &str| all.iter().find(|l| key(l, &parsed) == k);
        let out = match e {
            Edit::Add(line, label) => {
                let id = next_id(&parsed, "l");
                parsed
                    .blocks
                    .iter()
                    .position(|b| looks::line_of(b) == line)
                    .and_then(|b| looks::add(&src, &parsed, b, &label))
                    .inspect(|_| {
                        self.look = Some(id);
                        self.line = None;
                    })
            }
            Edit::Remove(k) => find(&k)
                .and_then(|l| looks::remove(&src, &parsed, l))
                .inspect(|_| self.look = None),
            Edit::Scope(k, s) => find(&k).and_then(|l| looks::set_scope(&src, &parsed, l, s)),
            Edit::Attr(k, a, v) => {
                find(&k).and_then(|l| looks::set_attr(&src, l, &a, v.as_deref()))
            }
        };
        let Some(out) = out else {
            self.error = Some("Couldn't change that look.".into());
            return;
        };
        if out != src {
            match script.apply_edit(project, out) {
                Ok(()) => self.error = None,
                Err(e) => self.error = Some(e),
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, counts: &[(Filter, usize)], total: usize) {
        ui.horizontal(|ui| {
            ui.label(strong("Effects", 16.0));
            ui.label(muted(format!(
                "{total} look{} over the script's lines",
                if total == 1 { "" } else { "s" }
            )));
            if let Some(e) = &self.error {
                ui.label(text(e, 12.0, theme::DANGER));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                for &(f, n) in counts.iter().rev() {
                    let name = match f {
                        Filter::All => "All lines",
                        Filter::WithLooks => "With effects",
                        Filter::Needs => "Needs a look",
                        Filter::Missing => "Not in catalog",
                    };
                    let on = self.filter == f;
                    let color = if matches!(f, Filter::Needs | Filter::Missing) && n > 0 {
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
    fn line_inspector(
        &mut self,
        ui: &mut egui::Ui,
        b: usize,
        parsed: &Script,
        tl: &timing::Timeline,
        all: &[Look],
        resolved: &Resolved,
        catalog: &Catalog,
    ) {
        let heading = matches!(parsed.blocks[b], Block::Heading { .. });
        ui.horizontal(|ui| {
            ui.label(strong(if heading { "Section" } else { "Line" }, 13.5));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(mono(
                    format!("line {}", looks::line_of(&parsed.blocks[b]) + 1),
                    theme::TEXT_MUTED,
                ));
            });
        });
        ui.add_space(6.0);
        if let Block::Heading { text: t, .. } = &parsed.blocks[b] {
            ui.label(text(t, 14.0, theme::TEXT));
            if let Some(s) = tl.sections.iter().find(|s| s.block == b) {
                widgets::kv(
                    ui,
                    "runs",
                    text(
                        format!("{} – {}", mmss(s.span.start), mmss(s.span.end)),
                        12.5,
                        theme::TEXT,
                    ),
                );
            }
            ui.add_space(6.0);
            ui.label(muted(
                "Looks you add here cover the whole section (written on the heading).",
            ));
            return;
        }
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, w * 9.0 / 16.0 * 0.6), Sense::hover());
        thumb(ui.painter(), r, parsed, b, resolved);
        ui.add(
            egui::Label::new(text(short(&line_text(parsed, b), 160), 12.5, theme::TEXT_2)).wrap(),
        );
        ui.add_space(6.0);
        let span = tl.blocks[b];
        widgets::kv(
            ui,
            "runs",
            text(
                format!("{} – {}", mmss_tenths(span.start), mmss_tenths(span.end)),
                12.5,
                theme::TEXT,
            ),
        );
        let over: Vec<&Look> = looks::over(all, b).collect();
        if over.is_empty() {
            widgets::kv(ui, "looks", text("none", 12.5, theme::TEXT_2));
            ui.add_space(6.0);
            ui.label(muted("Pick an effect below, or drag one onto the line."));
        } else {
            for l in over {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [92.0, 16.0],
                        egui::Label::new(mono("look", theme::TEXT_MUTED)),
                    );
                    let color = if asset_of(catalog, l).is_some() {
                        theme::CUE_LOOK
                    } else {
                        theme::WARN
                    };
                    if widgets::chip(ui, &l.label(), color)
                        .interact(Sense::click())
                        .on_hover_text("Select this look")
                        .clicked()
                    {
                        self.look = Some(key(l, parsed));
                        self.line = None;
                    }
                    ui.label(mono(scope_text(l.scope), theme::TEXT_MUTED));
                });
            }
        }
    }

    fn look_inspector(
        &mut self,
        ui: &mut egui::Ui,
        l: &Look,
        parsed: &Script,
        tl: &timing::Timeline,
        catalog: &Catalog,
        edit: &mut Option<Edit>,
    ) {
        let k = key(l, parsed);
        let entry = asset_of(catalog, l);
        ui.horizontal(|ui| {
            widgets::chip(
                ui,
                &l.label(),
                if entry.is_some() {
                    theme::CUE_LOOK
                } else {
                    theme::WARN
                },
            );
            if let Some(id) = &l.cue.id {
                ui.label(mono(format!("#{id}"), theme::TEXT_MUTED));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("Back to line").clicked() {
                    self.line = l.covers.first().map(|&b| looks::line_of(&parsed.blocks[b]));
                    self.look = None;
                }
            });
        });
        ui.add_space(6.0);
        match entry {
            Some(e) => {
                ui.label(text(&e.asset.name, 13.0, theme::TEXT));
                if let Some(s) = &e.asset.summary {
                    ui.add(egui::Label::new(text(s, 11.5, theme::TEXT_MUTED)).wrap());
                }
            }
            None => {
                ui.label(text(
                    "Not in the catalog: FilmCraft gets the name only.",
                    11.5,
                    theme::WARN,
                ));
            }
        }
        ui.add_space(8.0);
        let section = l
            .covers
            .first()
            .and_then(|&b| tl.section_of(b))
            .map(|s| s.title.clone());
        let covers = match l.covers.len() {
            0 => "nothing (no lines below it)".to_owned(),
            1 => "1 line".to_owned(),
            n => format!("{n} lines"),
        };
        widgets::kv(
            ui,
            "covers",
            text(
                match &section {
                    Some(s) => format!("{covers} in {s}"),
                    None => covers,
                },
                12.5,
                theme::TEXT,
            ),
        );
        if !l.covers.is_empty() {
            widgets::kv(
                ui,
                "runs",
                text(
                    format!(
                        "{} – {}  ({}s)",
                        mmss_tenths(l.span.start),
                        mmss_tenths(l.span.end),
                        trim_num(l.span.len())
                    ),
                    12.5,
                    theme::TEXT,
                ),
            );
        }
        let written = match &parsed.blocks[l.block] {
            Block::Heading { text: t, .. } => format!("the heading {}", short(t, 34)),
            _ if l.scope == Scope::FromHere => "a line of its own".to_owned(),
            _ => format!("\"{}\"", short(&line_text(parsed, l.block), 34)),
        };
        widgets::kv(ui, "written on", text(written, 12.5, theme::TEXT_2));

        // Scope.
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [92.0, 16.0],
                egui::Label::new(mono("scope", theme::TEXT_MUTED)),
            );
            let n_lines = match l.scope {
                Scope::Lines(n) => n,
                _ => 1,
            };
            for (s, name) in [
                (Scope::Line, "Line"),
                (Scope::Lines(n_lines.max(2)), "Lines"),
                (Scope::Section, "Section"),
            ] {
                let on = std::mem::discriminant(&s) == std::mem::discriminant(&l.scope);
                if ui
                    .add(egui::Button::selectable(on, text(name, 12.0, theme::TEXT)))
                    .clicked()
                    && !on
                    && !l.covers.is_empty()
                {
                    *edit = Some(Edit::Scope(k.clone(), s));
                }
            }
        });
        if let Scope::Lines(n) = l.scope {
            ui.horizontal(|ui| {
                ui.add_space(96.0);
                let committed = widgets::drag_value(ui, ("lines", &k), n, |v| {
                    egui::DragValue::new(v).range(2..=50).suffix(" lines")
                });
                if let Some(v) = committed {
                    *edit = Some(Edit::Scope(k.clone(), Scope::Lines(v)));
                }
            });
        }
        if l.scope == Scope::FromHere {
            ui.horizontal(|ui| {
                ui.add_space(96.0);
                ui.label(muted("from its own line to the end of the section"));
            });
        }

        // Settings.
        ui.add_space(10.0);
        ui.label(strong("Settings", 13.0));
        ui.add_space(4.0);
        let caps: &[Capability] = entry.map_or(&[], |e| &e.asset.capabilities);
        let mut shown: Vec<&str> = vec!["lines"];
        if !caps.is_empty() {
            let cur_id = l.cue.attr("cap").unwrap_or(&caps[0].id).to_owned();
            let cap = caps.iter().find(|c| c.id == cur_id).unwrap_or(&caps[0]);
            shown.push("cap");
            if caps.len() > 1 {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [92.0, 16.0],
                        egui::Label::new(mono("use", theme::TEXT_MUTED)),
                    );
                    egui::ComboBox::from_id_salt(("cap", &k))
                        .selected_text(&cap.id)
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for c in caps {
                                if ui.selectable_label(c.id == cap.id, &c.id).clicked()
                                    && c.id != cap.id
                                {
                                    *edit = Some(Edit::Attr(
                                        k.clone(),
                                        "cap".into(),
                                        Some(c.id.clone()),
                                    ));
                                }
                            }
                        });
                });
            }
            ui.horizontal(|ui| {
                ui.add_space(96.0);
                ui.add(egui::Label::new(text(&cap.summary, 11.5, theme::TEXT_MUTED)).wrap());
            });
            for (name, spec) in &cap.inputs {
                shown.push(name.as_str());
                if let Some(src) = cap.binds.get(name) {
                    widgets::kv(
                        ui,
                        name,
                        text(format!("from analysis: {src}"), 12.0, theme::OK),
                    );
                    continue;
                }
                input_row(ui, &k, name, spec, l.cue.attr(name), edit);
            }
            if !cap.requires.is_empty() {
                widgets::kv(
                    ui,
                    "needs",
                    text(cap.requires.join(", "), 12.0, theme::TEXT_2),
                );
            }
        }
        // Attributes the catalog doesn't describe.
        for (a, v) in l.params() {
            if shown.contains(&a.as_str()) {
                continue;
            }
            ui.horizontal(|ui| {
                ui.add_sized([92.0, 16.0], egui::Label::new(mono(a, theme::TEXT_MUTED)));
                text_value(ui, &k, a, v, edit);
                if ui.small_button("×").on_hover_text("Remove").clicked() {
                    *edit = Some(Edit::Attr(k.clone(), a.clone(), None));
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add_space(96.0);
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.new_attr)
                    .hint_text("key=value")
                    .desired_width(150.0),
            );
            if r.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && let Some((a, v)) = self.new_attr.split_once('=')
                && !a.trim().is_empty()
            {
                *edit = Some(Edit::Attr(
                    k.clone(),
                    a.trim().to_owned(),
                    Some(v.trim().to_owned()),
                ));
                self.new_attr.clear();
            }
        });

        ui.add_space(10.0);
        if widgets::button(ui, "Remove look").clicked() {
            *edit = Some(Edit::Remove(k.clone()));
        }
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
        target: Option<usize>,
        edit: &mut Option<Edit>,
    ) {
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
            ui.label(muted(
                "No effects in the catalog yet. Add *.asset.json with \"kind\": \"effect\", or type a name below.",
            ));
        }
        for (e, fit) in library {
            let name = e.asset.short_name().to_owned();
            widgets::drag_card(
                ui,
                egui::Id::new(("effect-lib", &e.asset.id)),
                EffectDrag {
                    label: name.clone(),
                },
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
                                icons::paint(ui.painter(), r, Icon::Look, theme::CUE_LOOK);
                                ui.label(text(&name, 13.0, theme::TEXT));
                                ui.label(mono(
                                    if e.asset.kind == AssetKind::Template {
                                        "template"
                                    } else {
                                        "effect"
                                    },
                                    theme::TEXT_MUTED,
                                ));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if let Some(t) = target
                                        && ui.small_button("Use").clicked()
                                    {
                                        *edit = Some(Edit::Add(t, name.clone()));
                                    }
                                    if ranked {
                                        ui.label(mono(format!("{fit:+.1}"), fit_color(*fit)));
                                    }
                                });
                            });
                            if let Some(s) = &e.asset.summary {
                                ui.label(text(s, 11.5, theme::TEXT_MUTED));
                            }
                            if !e.asset.capabilities.is_empty() {
                                ui.label(mono(
                                    e.asset
                                        .capabilities
                                        .iter()
                                        .map(|c| c.id.as_str())
                                        .collect::<Vec<_>>()
                                        .join(" · "),
                                    theme::TEXT_2,
                                ));
                            }
                        });
                },
            )
            .response
            .on_hover_text("Drag onto a line or a section, or select one and click Use");
            ui.add_space(3.0);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.custom)
                    .hint_text("Other, e.g. film-grain")
                    .desired_width(170.0),
            );
            if let Some(t) = target
                && ui
                    .add_enabled(!self.custom.trim().is_empty(), egui::Button::new("Use"))
                    .clicked()
            {
                *edit = Some(Edit::Add(t, self.custom.trim().to_owned()));
            }
        });
    }
}

fn scope_text(s: Scope) -> String {
    match s {
        Scope::Line => "this line".into(),
        Scope::Lines(n) => format!("{n} lines"),
        Scope::Section => "whole section".into(),
        Scope::FromHere => "to end of section".into(),
    }
}

/// One capability input: a menu for enums, a number for floats, text otherwise.
fn input_row(
    ui: &mut egui::Ui,
    k: &str,
    name: &str,
    spec: &serde_json::Value,
    value: Option<&str>,
    edit: &mut Option<Edit>,
) {
    let kind = spec.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let default = spec.get("default").map(|d| match d {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => a
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(","),
        other => other.to_string(),
    });
    ui.horizontal(|ui| {
        ui.add_sized(
            [92.0, 16.0],
            egui::Label::new(mono(name, theme::TEXT_MUTED)),
        );
        let set = value.is_some();
        match kind {
            "enum" => {
                let opts: Vec<String> = spec
                    .get("options")
                    .and_then(|o| o.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                let cur = value
                    .map(str::to_owned)
                    .or(default.clone())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt(("input", k, name))
                    .selected_text(text(
                        &cur,
                        12.0,
                        if set { theme::TEXT } else { theme::TEXT_2 },
                    ))
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for o in &opts {
                            if ui.selectable_label(*o == cur, o).clicked() && *o != cur {
                                *edit = Some(Edit::Attr(
                                    k.to_owned(),
                                    name.to_owned(),
                                    Some(o.clone()),
                                ));
                            }
                        }
                    });
            }
            "float" | "int" | "number" => {
                let min = spec.get("min").and_then(|v| v.as_f64()).unwrap_or(f64::MIN);
                let max = spec.get("max").and_then(|v| v.as_f64()).unwrap_or(f64::MAX);
                let v: f64 = value
                    .and_then(|s| s.parse().ok())
                    .or_else(|| default.as_deref().and_then(|d| d.parse().ok()))
                    .unwrap_or(0.0);
                let committed = widgets::drag_value(ui, ("input", k, name), v, |v| {
                    egui::DragValue::new(v)
                        .range(min..=max)
                        .speed(((max - min) / 300.0).clamp(0.001, 1.0))
                        .max_decimals(if kind == "int" { 0 } else { 2 })
                });
                if let Some(v) = committed {
                    *edit = Some(Edit::Attr(
                        k.to_owned(),
                        name.to_owned(),
                        Some(trim_num(v as f32)),
                    ));
                }
            }
            _ => {
                let v = value
                    .map(str::to_owned)
                    .or(default.clone())
                    .unwrap_or_default();
                text_value(ui, k, name, &v, edit);
            }
        }
        if set {
            if ui
                .small_button("reset")
                .on_hover_text("Use the default")
                .clicked()
            {
                *edit = Some(Edit::Attr(k.to_owned(), name.to_owned(), None));
            }
        } else {
            ui.label(mono("default", theme::TEXT_MUTED));
        }
    });
}

/// A text field that writes on Enter or when it loses focus.
fn text_value(ui: &mut egui::Ui, k: &str, name: &str, value: &str, edit: &mut Option<Edit>) {
    let id = egui::Id::new(("look-attr", k, name));
    let mut buf: String = ui
        .data_mut(|d| d.get_temp(id))
        .unwrap_or_else(|| value.to_owned());
    let r = ui.add(
        egui::TextEdit::singleline(&mut buf)
            .id(id.with("edit"))
            .desired_width(130.0),
    );
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buf.clone()));
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    if r.lost_focus() && buf.trim() != value {
        *edit = Some(Edit::Attr(
            k.to_owned(),
            name.to_owned(),
            (!buf.trim().is_empty()).then(|| buf.trim().to_owned()),
        ));
    }
}

/// A look chip painted at `at`; returns its rect.
fn paint_chip(
    p: &egui::Painter,
    at: egui::Pos2,
    label: &str,
    color: Color32,
    own: bool,
    sel: bool,
) -> Rect {
    let g = p.layout_no_wrap(label.to_owned(), FontId::proportional(12.0), color);
    let r = Rect::from_min_size(at, g.size() + vec2(28.0, 8.0));
    p.rect(
        r,
        5.0,
        if own {
            theme::tint(color, 0.15)
        } else {
            Color32::TRANSPARENT
        },
        Stroke::new(
            if sel { 1.5 } else { 1.0 },
            if sel {
                theme::ACCENT
            } else {
                theme::tint(color, 0.5)
            },
        ),
        egui::StrokeKind::Inside,
    );
    icons::paint(
        p,
        Rect::from_center_size(pos2(r.left() + 11.0, r.center().y), vec2(11.0, 11.0)),
        Icon::Look,
        color,
    );
    p.galley(pos2(r.left() + 21.0, r.top() + 4.0), g, color);
    r
}

#[allow(clippy::too_many_arguments)]
fn section_row(
    ui: &mut egui::Ui,
    parsed: &Script,
    tl: &timing::Timeline,
    h: usize,
    section_looks: &[(usize, &Look)],
    selected: bool,
    sel_look: Option<usize>,
    catalog: &Catalog,
) -> (bool, Option<Edit>, Option<usize>) {
    let w = ui.available_width();
    ui.add_space(6.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
    ui.add_space(4.0);
    let p = ui.painter().clone();
    let dragging = egui::DragAndDrop::has_payload_of_type::<EffectDrag>(ui.ctx());
    let hovered_drop = dragging
        && ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|pt| r.contains(pt));
    if selected || hovered_drop || resp.hovered() {
        p.rect(
            r,
            5.0,
            if selected {
                theme::BG_RAISED
            } else {
                theme::BG_PANEL
            },
            if hovered_drop || selected {
                Stroke::new(if hovered_drop { 2.0 } else { 1.0 }, theme::ACCENT)
            } else {
                Stroke::NONE
            },
            egui::StrokeKind::Inside,
        );
    }
    let title = match &parsed.blocks[h] {
        Block::Heading { text, .. } => text.clone(),
        _ => String::new(),
    };
    let g = p.layout_no_wrap(title, FontId::proportional(14.0), theme::TEXT);
    let tw = g.size().x;
    p.galley(
        pos2(r.left() + 10.0, r.center().y - g.size().y / 2.0),
        g,
        theme::TEXT,
    );
    if let Some(s) = tl.sections.iter().find(|s| s.block == h) {
        p.text(
            pos2(r.left() + 22.0 + tw, r.center().y),
            Align2::LEFT_CENTER,
            format!("{} – {}", mmss(s.span.start), mmss(s.span.end)),
            FontId::monospace(10.5),
            theme::TEXT_MUTED,
        );
    }
    let mut chip_clicked = None;
    let mut x = r.right() - 8.0;
    for (i, l) in section_looks.iter().rev() {
        let color = if asset_of(catalog, l).is_some() {
            theme::CUE_LOOK
        } else {
            theme::WARN
        };
        let label = format!("{} · section", l.label());
        let g = p.layout_no_wrap(label.clone(), FontId::proportional(12.0), color);
        let cw = g.size().x + 28.0;
        x -= cw;
        let cr = paint_chip(
            &p,
            pos2(x, r.center().y - 10.0),
            &label,
            color,
            true,
            sel_look == Some(*i),
        );
        if ui
            .interact(cr, ui.id().with(("sec-chip", i)), Sense::click())
            .clicked()
        {
            chip_clicked = Some(*i);
        }
        x -= 6.0;
    }
    if section_looks.is_empty() && (resp.hovered() || selected) {
        p.text(
            pos2(r.right() - 10.0, r.center().y),
            Align2::RIGHT_CENTER,
            "drop an effect here for the whole section",
            FontId::monospace(10.0),
            theme::TEXT_MUTED,
        );
    }
    let mut ev = None;
    if hovered_drop
        && ui.input(|i| i.pointer.any_released())
        && let Some(d) = egui::DragAndDrop::take_payload::<EffectDrag>(ui.ctx())
    {
        ev = Some(Edit::Add(
            looks::line_of(&parsed.blocks[h]),
            d.label.clone(),
        ));
    }
    (resp.clicked() && chip_clicked.is_none(), ev, chip_clicked)
}

struct RowArgs<'a> {
    n: usize,
    block: usize,
    parsed: &'a Script,
    tl: &'a timing::Timeline,
    resolved: &'a Resolved,
    /// Looks covering this line, with their index in the full list.
    here: &'a [(usize, &'a Look)],
    lane_of: &'a [usize],
    /// Bar columns to leave left of the card.
    n_lanes: usize,
    timed: &'a [usize],
    selected: bool,
    sel_look: Option<usize>,
    needs: bool,
    suggestions: &'a [(&'a str, f32)],
    catalog: &'a Catalog,
}

struct RowOut {
    clicked: bool,
    chip: Option<usize>,
    edit: Option<Edit>,
}

fn line_row(ui: &mut egui::Ui, a: RowArgs) -> RowOut {
    let b = a.block;
    let w = ui.available_width();
    let h = if a.needs { 84.0 } else { 66.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    ui.add_space(6.0);
    let gap = 6.0 + ui.spacing().item_spacing.y * 2.0;
    let p = ui.painter().clone();
    let dragging = egui::DragAndDrop::has_payload_of_type::<EffectDrag>(ui.ctx());
    let hovered_drop = dragging
        && ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|pt| r.contains(pt));

    // Coverage bars, drawn through the gap below so a run of rows reads as one bar.
    let pos = a.timed.iter().position(|&t| t == b);
    for (i, l) in a.here {
        let lane = a.lane_of[*i];
        let x = r.left() + 2.0 + lane as f32 * LANE_W;
        let first = l.covers.first() == Some(&b);
        let last = l.covers.last() == Some(&b);
        let next_covered = pos
            .and_then(|k| a.timed.get(k + 1))
            .is_some_and(|nb| l.covers.contains(nb));
        let top = if first { r.top() + 6.0 } else { r.top() - gap };
        let bottom = if last || !next_covered {
            r.bottom() - 6.0
        } else {
            r.bottom()
        };
        let color = if a.sel_look == Some(*i) {
            theme::ACCENT
        } else if asset_of(a.catalog, l).is_some() {
            theme::tint(theme::CUE_LOOK, 0.7)
        } else {
            theme::tint(theme::WARN, 0.7)
        };
        p.rect_filled(
            Rect::from_x_y_ranges(x..=x + LANE_W - 2.0, top..=bottom),
            2.0,
            color,
        );
    }

    let card = Rect::from_min_max(
        pos2(r.left() + 4.0 + a.n_lanes as f32 * LANE_W, r.top()),
        r.max,
    );
    let stroke = if hovered_drop || a.selected {
        Stroke::new(if hovered_drop { 2.0 } else { 1.0 }, theme::ACCENT)
    } else {
        Stroke::new(1.0, theme::BORDER)
    };
    p.rect(
        card,
        CornerRadius::same(7),
        if a.selected {
            theme::BG_RAISED
        } else {
            theme::BG_PANEL
        },
        if a.needs && !a.selected && !hovered_drop {
            Stroke::NONE
        } else {
            stroke
        },
        egui::StrokeKind::Inside,
    );
    if a.needs && !a.selected && !hovered_drop {
        dashed_rect(
            &p,
            card.shrink(0.5),
            Stroke::new(1.0, theme::tint(theme::WARN, 0.6)),
        );
    }

    // Number and time.
    let span = a.tl.blocks[b];
    p.text(
        pos2(card.left() + 12.0, card.top() + 20.0),
        Align2::LEFT_CENTER,
        format!("{:02}", a.n + 1),
        FontId::monospace(12.0),
        theme::TEXT_2,
    );
    p.text(
        pos2(card.left() + 12.0, card.top() + 38.0),
        Align2::LEFT_CENTER,
        mmss(span.start),
        FontId::monospace(10.0),
        theme::TEXT_MUTED,
    );
    let th = Rect::from_min_size(pos2(card.left() + 50.0, card.top() + 8.0), vec2(88.0, 50.0));
    thumb(&p, th, a.parsed, b, a.resolved);

    let chips_w = (card.width() * 0.36).clamp(220.0, 420.0);
    let chips = Rect::from_min_max(
        pos2(card.right() - chips_w, card.top()),
        card.max - vec2(10.0, 0.0),
    );
    let tx = th.right() + 12.0;
    let tw = (chips.left() - tx - 12.0).max(60.0);
    let kind = match &a.parsed.blocks[b] {
        Block::Line { kind, .. } => *kind,
        _ => LineKind::Spoken,
    };
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = tw;
    job.wrap.max_rows = 2;
    let onscreen = kind == LineKind::OnScreen;
    job.append(
        &line_text(a.parsed, b),
        0.0,
        egui::TextFormat {
            font_id: FontId::new(
                12.5,
                egui::FontFamily::Name(
                    if onscreen {
                        theme::DOC_ITALIC
                    } else {
                        theme::DOC
                    }
                    .into(),
                ),
            ),
            color: if onscreen { theme::TEXT_2 } else { theme::TEXT },
            ..Default::default()
        },
    );
    let g = p.layout_job(job);
    p.galley(pos2(tx, card.top() + 12.0), g, theme::TEXT);

    // Chips: looks on this line.
    let mut out = RowOut {
        clicked: false,
        chip: None,
        edit: None,
    };
    let mut x = chips.left();
    let mut y = card.top() + 10.0;
    for (i, l) in a.here {
        let own = l.block == b;
        let color = if asset_of(a.catalog, l).is_some() {
            theme::CUE_LOOK
        } else {
            theme::WARN
        };
        let label = match (own, l.scope) {
            (true, Scope::Lines(n)) => format!("{} · {n} lines", l.label()),
            (true, _) => l.label(),
            (false, Scope::Section) => format!("{} · section", l.label()),
            (false, _) => format!("↳ {}", l.label()).replace('↳', "›"),
        };
        let gw = p
            .layout_no_wrap(label.clone(), FontId::proportional(12.0), color)
            .size()
            .x
            + 28.0;
        if x + gw > chips.right() - 8.0 && x > chips.left() {
            x = chips.left();
            y += 26.0;
        }
        let cr = paint_chip(&p, pos2(x, y), &label, color, own, a.sel_look == Some(*i));
        let hit = ui.interact(cr, ui.id().with(("look-chip", b, i)), Sense::click());
        if hit.clicked() {
            out.chip = Some(*i);
        }
        hit.on_hover_text(if own {
            "Written on this line. Click to edit."
        } else {
            "Comes from a heading or an earlier line. Click to edit."
        });
        x = cr.right() + 6.0;
    }
    if a.here.is_empty() && !a.needs {
        p.text(
            pos2(chips.left(), card.top() + 21.0),
            Align2::LEFT_CENTER,
            "no effect",
            FontId::monospace(10.5),
            theme::TEXT_MUTED,
        );
    }
    if a.needs {
        let g = p.layout_no_wrap(
            "add a look…".into(),
            FontId::proportional(12.0),
            theme::WARN,
        );
        let cr = Rect::from_min_size(
            pos2(chips.left(), card.top() + 10.0),
            g.size() + vec2(16.0, 8.0),
        );
        dashed_rect(&p, cr, Stroke::new(1.0, theme::tint(theme::WARN, 0.8)));
        p.galley(cr.min + vec2(8.0, 4.0), g, theme::WARN);
        p.text(
            pos2(cr.right() + 8.0, cr.center().y),
            Align2::LEFT_CENTER,
            "on-screen line, no look",
            FontId::monospace(10.0),
            theme::WARN,
        );
        let mut sx = chips.left();
        let sy = cr.bottom() + 8.0;
        for (name, fit) in a.suggestions {
            let t = format!("{name} {fit:+.1}");
            let g = p.layout_no_wrap(t, FontId::proportional(11.5), theme::TEXT_2);
            let tr = Rect::from_min_size(pos2(sx, sy), g.size() + vec2(14.0, 6.0));
            if tr.right() > chips.right() - 6.0 {
                break;
            }
            let hit = ui.interact(tr, ui.id().with(("fx-suggest", b, *name)), Sense::click());
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
                out.edit = Some(Edit::Add(
                    looks::line_of(&a.parsed.blocks[b]),
                    (*name).to_owned(),
                ));
            }
            sx = tr.right() + 6.0;
        }
    }

    if hovered_drop
        && ui.input(|i| i.pointer.any_released())
        && let Some(d) = egui::DragAndDrop::take_payload::<EffectDrag>(ui.ctx())
    {
        out.edit = Some(Edit::Add(
            looks::line_of(&a.parsed.blocks[b]),
            d.label.clone(),
        ));
    }
    out.clicked = resp.clicked() && out.chip.is_none();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_do_not_overlap() {
        let s = spearfia_script::parse(
            "## A :look[grain]{#l1}\none :look[zoom]{lines=2 #l2}\ntwo\nthree :look[pip]{#l3}\n",
        );
        let tl = timing::timeline(&s);
        let all = looks::looks(&s, &tl);
        let t = looks::timed(&s, &tl);
        let l = lanes(&all, &t);
        assert_eq!(l, vec![0, 1, 1]);
    }

    #[test]
    fn overflowing_lanes_stay_distinct() {
        let s = spearfia_script::parse(
            "## A :look[a]{#l1} :look[b]{#l2} :look[c]{#l3} :look[d]{#l4} :look[e]{#l5}\nspoken words\n",
        );
        let tl = timing::timeline(&s);
        let all = looks::looks(&s, &tl);
        let l = lanes(&all, &looks::timed(&s, &tl));
        assert_eq!(l, vec![0, 1, 2, 3, 4]);
        assert_eq!(lane_count(&l), 5);
        assert_eq!(lane_count(&[0, 1]), LANES);
    }
}
