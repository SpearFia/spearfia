//! Visual Script reuses the document editor; the side panel edits typed cue metadata.
use super::script_stage::ScriptStage;
use crate::{theme, widgets};
use egui::{Panel, RichText};
use spearfia_core::{
    links::Links,
    project::Project,
    visuals::{Anchor, Plan, Reference},
};
use spearfia_script::{Block, LineKind, format};
use std::{collections::BTreeMap, ops::Range};

pub struct VisualStage {
    pub editor: ScriptStage,
    selected: Option<String>,
    narration: bool,
    passage: Option<Range<usize>>,
    first_line: Option<usize>,
    status: Option<String>,
    export_kind: String,
    jump: Option<Range<usize>>,
}
impl VisualStage {
    pub fn new(project: &Project) -> Self {
        Self {
            editor: ScriptStage::load_visual(project),
            selected: None,
            narration: true,
            passage: None,
            first_line: None,
            status: None,
            export_kind: "cue-sheet".into(),
            jump: None,
        }
    }
    pub fn add_for_selection(
        &mut self,
        project: &Project,
        script: &str,
        range: Range<usize>,
        ctx: &egui::Context,
    ) {
        self.passage = Some(range.clone());
        let result = Plan::parse(self.editor.source())
            .and_then(|p| p.add("New visual cue", Anchor::new(script, range)?));
        match result {
            Ok((source, id)) => {
                self.apply(project, ctx, source);
                self.select(&id);
            }
            Err(e) => self.status = Some(e.to_string()),
        }
    }
    fn select(&mut self, id: &str) {
        self.selected = Some(id.into());
        if let Ok(plan) = Plan::parse(self.editor.source())
            && let Ok(cue) = plan.cue(id)
        {
            self.editor.focus_range(cue.span.start..cue.span.start);
        }
    }
    fn apply(&mut self, project: &Project, ctx: &egui::Context, source: String) {
        self.status = self
            .editor
            .flush(project)
            .and_then(|()| self.editor.change_document(ctx, project, source))
            .err();
    }
    fn edit(
        &mut self,
        project: &Project,
        ctx: &egui::Context,
        result: spearfia_core::error::Result<String>,
    ) {
        match result {
            Ok(source) => self.apply(project, ctx, source),
            Err(e) => self.status = Some(e.to_string()),
        }
    }
    pub fn show(
        &mut self,
        project: &Project,
        narration: &str,
        links: &Links,
        ui: &mut egui::Ui,
    ) -> Option<Range<usize>> {
        let parsed = spearfia_script::parse(narration);
        let plan = Plan::parse(self.editor.source());
        Panel::top("visual-tools").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Visual Script").strong().size(16.0));
                ui.checkbox(&mut self.narration, "Narration pane");
                if ui.button("New cue from selection").on_hover_text("Select narration in the pane; Shift-click to extend across lines.").clicked() {
                    if let Some(range) = self.passage.clone() { self.add_for_selection(project, narration, range, ui.ctx()); }
                    else { self.status = Some("Select a narration passage in the pane first, or select words in Script and choose Plan visuals.".into()); }
                }
                egui::ComboBox::from_id_salt("visual-export").selected_text(&self.export_kind).show_ui(ui, |ui| {
                    for kind in ["cue-sheet", "storyboard", "prompts", "handoff"] { ui.selectable_value(&mut self.export_kind, kind.into(), kind); }
                });
                for (save, label) in [(false, "Copy export"), (true, "Save export…")] {
                    if ui.button(label).clicked() {
                        let result = self.editor.flush(project).and_then(|()| {
                            if self.export_kind == "handoff" && project.read_script().map_err(|e| e.to_string())? != narration { return Err("Save the narration script before exporting a production handoff.".into()); }
                            if self.export_kind == "handoff" { project.export_visuals("handoff") }
                            else { Plan::parse(self.editor.source()).and_then(|p| p.export(&self.export_kind, narration)) }.map_err(|e| e.to_string())
                        });
                        match result {
                            Ok(text) if !save => { ui.ctx().copy_text(text); self.status = Some("Export copied.".into()); }
                            Ok(text) => {
                                if let Some(path) = rfd::FileDialog::new().set_file_name(format!("visual-{}.md", self.export_kind)).add_filter("Markdown", &["md"]).save_file() {
                                    self.status = Some(match project.save_visual_export(&path, &text) { Ok(()) => format!("Export saved: {}", path.display()), Err(e) => e.to_string() });
                                }
                            }
                            Err(e) => self.status = Some(e),
                        }
                    }
                }
            });
            if let Some(status) = &self.status { ui.label(widgets::text(status, 12.0, theme::WARN)); }
        });
        Panel::left("visual-cues")
            .default_size(294.0)
            .min_size(250.0)
            .max_size(430.0)
            .resizable(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("visual-cue-scroll")
                    .show(ui, |ui| match &plan {
                        Ok(plan) => self.cue_panel(project, narration, &parsed, plan, ui),
                        Err(e) => {
                            ui.label(widgets::text(e.to_string(), 12.0, theme::DANGER));
                            ui.label(
                                "The document is preserved. Repair the record in Markdown view.",
                            );
                        }
                    });
            });
        if self.narration {
            Panel::right("visual-narration")
                .default_size(340.0)
                .min_size(240.0)
                .max_size(650.0)
                .resizable(true)
                .show(ui, |ui| {
                    ui.label(RichText::new("Narration · read only").strong());
                    ui.label(widgets::muted(
                        "Click a line; Shift-click to extend. Edit narration in Script.",
                    ));
                    if let Some(range) = &self.passage
                        && ui.button("Edit selected passage in Script").clicked()
                    {
                        self.jump = Some(range.clone());
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("visual-narration-scroll")
                        .show(ui, |ui| {
                            for block in &parsed.blocks {
                                match block {
                                    Block::Heading { text, .. } => {
                                        ui.add_space(10.0);
                                        ui.label(RichText::new(text).strong());
                                    }
                                    Block::Line { kind, line, .. } if kind.is_spoken() => {
                                        let Some(range) = format::line_range(narration, *line)
                                        else {
                                            continue;
                                        };
                                        let selected = self.passage.as_ref().is_some_and(|r| {
                                            r.start <= range.start && r.end >= range.end
                                        });
                                        let text = narration.get(range.clone()).unwrap_or("");
                                        let covered = plan.as_ref().ok().is_some_and(|p| {
                                            p.cues.iter().any(|c| {
                                                c.meta.anchor.resolve(narration).range.is_some_and(
                                                    |r| {
                                                        r.start <= range.start && r.end >= range.end
                                                    },
                                                )
                                            })
                                        });
                                        let label = format!(
                                            "{} · {}\n{}",
                                            if *kind == LineKind::Live {
                                                "LIVE"
                                            } else {
                                                "VO"
                                            },
                                            if covered { "planned" } else { "needs visuals" },
                                            text
                                        );
                                        if ui
                                            .add(egui::Button::selectable(selected, label).wrap())
                                            .clicked()
                                        {
                                            let first = if ui.input(|i| i.modifiers.shift) {
                                                self.first_line.unwrap_or(*line)
                                            } else {
                                                self.first_line = Some(*line);
                                                *line
                                            };
                                            let start =
                                                format::line_range(narration, first.min(*line))
                                                    .map_or(range.start, |r| r.start);
                                            let end =
                                                format::line_range(narration, first.max(*line))
                                                    .map_or(range.end, |r| r.end);
                                            self.passage = Some(start..end);
                                        }
                                        ui.add_space(4.0);
                                    }
                                    _ => {}
                                }
                            }
                        });
                });
        }
        if let Some(super::script_stage::ScriptEvent::OpenNarration(id)) =
            self.editor.show(project, links, ui)
        {
            let result = Plan::parse(self.editor.source())
                .and_then(|p| p.cue(&id).map(|c| c.meta.anchor.resolve(narration)));
            match result {
                Ok(resolution) => {
                    self.jump = resolution.range;
                    if self.jump.is_none() {
                        self.status = Some(
                            "Resolve this narration attachment in the cue panel first.".into(),
                        );
                    }
                }
                Err(error) => self.status = Some(error.to_string()),
            }
        }
        self.jump.take()
    }
    fn cue_panel(
        &mut self,
        project: &Project,
        narration: &str,
        script: &spearfia_script::Script,
        plan: &Plan,
        ui: &mut egui::Ui,
    ) {
        let ranges: Vec<_> = plan
            .cues
            .iter()
            .filter_map(|c| c.meta.anchor.resolve(narration).range)
            .collect();
        let mut total = 0usize;
        let mut planned = 0usize;
        for (index, block) in script.blocks.iter().enumerate() {
            if let Block::Line { kind, .. } = block
                && kind.is_spoken()
            {
                for word in format::word_ranges(narration, script, index) {
                    total += 1;
                    if ranges
                        .iter()
                        .any(|r| word.start >= r.start && word.end <= r.end)
                    {
                        planned += 1;
                    }
                }
            }
        }
        ui.heading("Visual plan");
        ui.label(format!("Coverage: {planned} / {total} narration words"));
        ui.label(format!(
            "Assets ready: {} / {}",
            plan.cues
                .iter()
                .filter(|c| c.meta.asset_state == "ready")
                .count(),
            plan.cues.len()
        ));
        ui.label(format!(
            "Review accepted: {} / {}",
            plan.cues
                .iter()
                .filter(|c| c.meta.review == "accepted")
                .count(),
            plan.cues.len()
        ));
        ui.label(widgets::muted(
            "Holds and intentional LIVE footage count as planned treatments.",
        ));
        if ui.button("Next unresolved cue").clicked() {
            let start = plan
                .cues
                .iter()
                .position(|c| Some(&c.meta.id) == self.selected.as_ref())
                .map_or(0, |i| i.saturating_add(1));
            if let Some(c) = plan
                .cues
                .iter()
                .skip(start)
                .chain(plan.cues.iter().take(start))
                .find(|c| {
                    c.meta.anchor.resolve(narration).range.is_none()
                        || c.meta.asset_state != "ready"
                        || c.meta.review != "accepted"
                })
            {
                self.select(&c.meta.id);
            }
        }
        if ui.button("Next unplanned narration").clicked()
            && let Some(range) = script.blocks.iter().enumerate().find_map(|(index, b)| {
                let Block::Line { kind, line, .. } = b else {
                    return None;
                };
                if !kind.is_spoken() {
                    return None;
                }
                let missing = format::word_ranges(narration, script, index)
                    .iter()
                    .any(|word| {
                        !ranges
                            .iter()
                            .any(|r| word.start >= r.start && word.end <= r.end)
                    });
                missing
                    .then(|| format::line_range(narration, *line))
                    .flatten()
            })
        {
            self.passage = Some(range);
            self.narration = true;
        }
        ui.separator();
        for cue in &plan.cues {
            let resolution = cue.meta.anchor.resolve(narration);
            let text = format!(
                "{}\n{} · {} · {}",
                cue.title, resolution.state, cue.meta.asset_state, cue.meta.review
            );
            if ui
                .selectable_label(self.selected.as_deref() == Some(&cue.meta.id), text)
                .clicked()
            {
                self.select(&cue.meta.id);
                self.passage = resolution.range;
            }
        }
        if plan.cues.is_empty() {
            ui.label("Select narration and create a cue. Write directions on the page; use / to insert paragraph types.");
        }
        let Some(cue) = self.selected.as_ref().and_then(|id| plan.cue(id).ok()) else {
            return;
        };
        ui.separator();
        ui.label(RichText::new(&cue.title).strong());
        ui.label(widgets::muted(format!("Cue {}", cue.meta.id)));
        let resolution = cue.meta.anchor.resolve(narration);
        if ui
            .add(
                egui::Button::new(format!(
                    "Linked narration · {}\n{}",
                    cue.meta.anchor.kind, cue.meta.anchor.quote
                ))
                .wrap(),
            )
            .on_hover_text("Jump to the authoritative Script document")
            .clicked()
        {
            if let Some(range) = resolution.range.clone() {
                self.jump = Some(range);
            } else {
                self.status =
                    Some("This attachment needs resolving before it can jump to Script.".into());
            }
        }
        if let Some(range) = &resolution.range {
            let (seconds, recorded) = crate::vo_timing::passage_seconds(narration, range.clone());
            ui.label(widgets::muted(if recorded {
                format!("Linked narration: {} (recorded VO)", widgets::mmss(seconds))
            } else {
                format!(
                    "Linked narration: ~{} (150 wpm estimate)",
                    widgets::mmss(seconds)
                )
            }));
        } else {
            ui.label(widgets::text(
                format!(
                    "{} attachment — original passage retained",
                    resolution.state
                ),
                12.0,
                theme::WARN,
            ));
            ui.collapsing(
                "Candidate current passage · confirm before reattaching",
                |ui| {
                    ui.label(&resolution.candidate);
                },
            );
        }
        if ui
            .add_enabled(
                self.passage.is_some(),
                egui::Button::new("Attach to selected narration"),
            )
            .clicked()
            && let Some(range) = self.passage.clone()
        {
            let result = Anchor::new(narration, range).and_then(|a| plan.reanchor(&cue.meta.id, a));
            self.edit(project, ui.ctx(), result);
        }
        let mut meta = cue.meta.clone();
        let mut changed = false;
        changed |= select(
            ui,
            "Visual type",
            &mut meta.visual_type,
            &[
                "Scene",
                "Character scene",
                "Chart",
                "Diagram",
                "On-screen text",
                "On camera",
                "Cutaway",
                "Overlay",
                "Intentional hold",
            ],
        );
        changed |= select(
            ui,
            "Production method",
            &mut meta.method,
            &[
                "Reuse",
                "LIVE footage",
                "Generated artwork",
                "Generated video",
                "Character animation",
                "Remotion template",
                "Manual",
            ],
        );
        if cue.meta.anchor.kind.contains("LIVE") {
            ui.horizontal_wrapped(|ui| {
                for (label, kind) in [
                    ("Keep on camera", "On camera"),
                    ("Add overlay", "Overlay"),
                    ("Cover with cutaway", "Cutaway"),
                ] {
                    if ui.small_button(label).clicked() {
                        meta.visual_type = kind.into();
                        if kind == "On camera" {
                            meta.method = "LIVE footage".into();
                        }
                        changed = true;
                    }
                }
            });
        }
        changed |= select(
            ui,
            "Asset readiness",
            &mut meta.asset_state,
            &["missing", "blocked", "ready"],
        );
        changed |= select(
            ui,
            "Review",
            &mut meta.review,
            &["draft", "awaiting", "accepted", "changes"],
        );
        ui.label("Timing / narration trigger");
        changed |= ui.text_edit_singleline(&mut meta.timing).changed();
        ui.collapsing("Production details / preparation", |ui| {
            let fields: &[&str] = if ["Chart", "Diagram"].contains(&meta.visual_type.as_str()) {
                &["Data table", "Units", "Population", "Sources", "Approximation flags", "Axes / scales", "Relationships", "Reveal order"]
            } else if ["Character animation", "Remotion template"].contains(&meta.method.as_str()) {
                &["Template / rig / revision", "Characters", "Views needed", "Poses / actions", "Props", "Contact targets", "Template parameters", "Missing assets / preparation tasks"]
            } else if meta.method.starts_with("Generated") {
                &["Workflow / model", "Content inputs", "Identity inputs", "Style inputs", "Composition inputs", "Prompt input mapping", "Seed / settings", "Missing assets / preparation tasks"]
            } else { &["Exact asset / revision", "Shot / frame / in-out", "Usage instructions", "Missing assets / preparation tasks"] };
            for field in fields { ui.label(*field); changed |= ui.add(egui::TextEdit::multiline(meta.details.entry((*field).into()).or_default()).desired_rows(2).desired_width(f32::INFINITY)).changed(); }
            ui.label(widgets::muted("These are production instructions. This page does not run templates or generators."));
        });
        ui.collapsing(format!("References ({})", meta.references.len()), |ui| {
            let mut remove = None;
            for (index, reference) in meta.references.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    changed |= select(ui, "Role", &mut reference.role, &["asset", "content", "identity", "style", "composition", "data", "character", "prop"]);
                    ui.label("Path / catalog ID / source URL"); changed |= ui.text_edit_singleline(&mut reference.path).changed();
                    if ui.small_button("Browse…").clicked() && let Some(path) = rfd::FileDialog::new().pick_file() { reference.path = path.display().to_string(); changed = true; }
                    ui.label("Exact revision / shot / frame"); changed |= ui.text_edit_singleline(&mut reference.revision).changed();
                    changed |= select(ui, "Change scope", &mut reference.scope, &["use", "shared"]);
                    let affected: Vec<_> = plan.cues.iter().filter(|c| c.meta.references.iter().any(|r| r.path == reference.path && !r.path.is_empty())).map(|c| c.title.as_str()).collect();
                    if !affected.is_empty() { ui.label(widgets::muted(format!("Used by: {}", affected.join(", ")))); }
                    if reference.path.contains(['\\', '/']) && !reference.path.contains("://") && !reference.path.contains('@') && !project.resolve(&reference.path).exists() { ui.label(widgets::text("Reference file is missing", 11.5, theme::WARN)); }
                    if ui.small_button("Remove reference").clicked() { remove = Some(index); }
                    ui.separator();
                });
            }
            if let Some(index) = remove { meta.references.remove(index); changed = true; }
            if ui.add_enabled(meta.references.len() < 128, egui::Button::new("Add reference")).clicked() { meta.references.push(Reference { role: "asset".into(), path: String::new(), revision: String::new(), scope: "use".into(), extra: BTreeMap::new() }); changed = true; }
            ui.label(widgets::muted("Scope records intent. External shared files are edited in their own tool; references here do not overwrite them."));
        });
        if changed {
            self.edit(project, ui.ctx(), plan.update(&meta));
        }
        ui.separator();
        if ui.button("Split cue at document caret").clicked() {
            match plan.split(&cue.meta.id, self.editor.caret()) {
                Ok((source, id)) => {
                    self.apply(project, ui.ctx(), source);
                    self.select(&id);
                }
                Err(e) => self.status = Some(e.to_string()),
            }
        }
        if ui.button("Merge with next cue").clicked() {
            self.edit(project, ui.ctx(), plan.merge_next(&cue.meta.id, narration));
        }
        ui.collapsing("Shots within this cue", |ui| {
            if ui.button("Add shot").clicked() {
                self.edit(project, ui.ctx(), plan.add_shot(&cue.meta.id));
            }
            if let Ok(shots) = plan.shots(&cue.meta.id) {
                for (index, range) in shots.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(
                            plan.source
                                .get(range.clone())
                                .unwrap_or("")
                                .lines()
                                .next()
                                .unwrap_or("Shot"),
                        );
                        if ui.add_enabled(index > 0, egui::Button::new("↑")).clicked() {
                            self.edit(
                                project,
                                ui.ctx(),
                                plan.move_shot(&cue.meta.id, index, index.saturating_sub(1)),
                            );
                        }
                        if ui
                            .add_enabled(
                                index.saturating_add(1) < shots.len(),
                                egui::Button::new("↓"),
                            )
                            .clicked()
                        {
                            self.edit(
                                project,
                                ui.ctx(),
                                plan.move_shot(&cue.meta.id, index, index.saturating_add(1)),
                            );
                        }
                    });
                }
            }
        });
    }
}
fn select(ui: &mut egui::Ui, label: &str, value: &mut String, options: &[&str]) -> bool {
    let old = value.clone();
    ui.label(label);
    egui::ComboBox::from_id_salt(label)
        .selected_text(value.as_str())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for option in options {
                ui.selectable_value(value, (*option).into(), *option);
            }
        });
    old != *value
}
