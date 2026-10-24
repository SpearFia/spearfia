//! Browse vision templates and import AI-generated, validated templates.
use crate::app::App;
use crate::theme;
use crate::widgets::{self, mono, muted, strong, text};
use spearfia_core::visions::{self, Vision};

pub const EXAMPLE: &str = r#"{
  "$schema": "https://spearfia.dev/schemas/vision/v1.json",
  "name": "educational-explainer",
  "title": "Educational explainer",
  "goal": "Make a complex idea understandable and memorable.",
  "pacing": {
    "avg_shot_seconds": 4,
    "max_silence_seconds": 0.5,
    "cut_style": "mixed"
  },
  "script_rules": ["Introduce one concept at a time.", "Explain unfamiliar terms with a concrete example."],
  "weights": {"diagram": 0.9, "animation": 0.8, "meme": -0.5},
  "avoid": ["Unexplained jargon"],
  "references": []
}"#;

pub fn creation_prompt() -> String {
    format!(
        r#"Create a reusable vision template for Spear Fia from my requirements and any channel-analysis Markdown I attach. I will describe the channel/style and what I want after this prompt. For example, I may attach a Kurzgesagt analysis and request a science-explainer vision. Use the supplied analysis as evidence; label assumptions instead of inventing observations. Describe reusable storytelling/editing principles in your own words.

A vision is a creative brief for what the audience should feel or understand, how a script is written, the pace of edits, and which catalog assets suit the project. It is a template, not a script, media asset, or executable effect. Positive/negative weights bias catalog search using actual asset names/tags. They do not create assets or automatically apply effects. Rules/avoid/references are guidance retained in the project; automatic enforcement is not implemented.

Return ONLY one JSON object, without Markdown fences or commentary. It uses the existing Spear Fia vision schema (JSON is also valid YAML). Produce a complete standalone template; do not use extends. Keep it under 256 KiB.

Fields:
- name: unique lowercase ID, 1–80 characters of a-z, 0-9 and hyphens; no leading/trailing hyphen (e.g. kurzgesagt-inspired).
- title: human-readable display name.
- goal: what the viewer should feel, learn or do.
- pacing.avg_shot_seconds: a finite number greater than 0; an editing target, not spoken-word timing.
- pacing.max_silence_seconds: a finite number >= 0.
- pacing.cut_style: exactly jump, invisible, rhythmic, or mixed.
- script_rules: array of specific, actionable writing/editing instructions.
- weights: object mapping catalog tags or asset names to finite numbers from -1 (avoid) through 0 (neutral) to 1 (prefer). If I supply catalog tags, use them; otherwise use descriptive tags and say in notes that they may need matching assets.
- avoid: array of things to avoid.
- references: array of {{"url":"absolute URL", "note":"why it matters"}}; use only URLs I supplied, otherwise []. Do not invent URLs.
- notes: optional text for assumptions, tradeoffs or how to adapt the source analysis.

Example of the format (adapt all creative choices to my request):
{EXAMPLE}

My channel analysis and desired changes follow:
"#
    )
}

#[derive(Debug, Default)]
pub struct VisionView {
    pub selected: Option<String>,
    adding: bool,
    source: String,
    status: Option<String>,
}

pub fn list(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Vision");
    ui.label(muted(
        "Reusable creative briefs for your scripts, pacing and catalog choices.",
    ));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, "+ Add vision").clicked() {
            app.catalog_view.visions.adding = true;
            app.catalog_view.visions.status = None;
        }
        if widgets::button(ui, "Copy AI prompt").clicked() {
            ui.ctx().copy_text(creation_prompt());
            app.catalog_view.visions.status = Some("Prompt copied. Attach your channel-analysis .md to your AI, add your requirements, then paste its JSON here.".into());
        }
    });
    ui.label(muted("Have a channel analysis? Copy the prompt and give it to your AI with that Markdown file and the changes you want."));
    if let Some(status) = &app.catalog_view.visions.status {
        ui.label(text(status, 12.5, theme::CUE_VISION));
    }
    for error in &app.vision_errors {
        ui.label(text(error, 12.0, theme::DANGER));
    }
    ui.add_space(10.0);
    egui::ScrollArea::vertical()
        .id_salt("vision-templates")
        .show(ui, |ui| {
            for v in &app.visions {
                let selected = app.catalog_view.visions.selected.as_deref() == Some(v.name());
                let response = widgets::card(
                    if selected {
                        theme::BG_HOVER
                    } else {
                        theme::BG_PANEL
                    },
                    theme::BORDER,
                )
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(strong(v.title(), 14.0));
                        widgets::tag(
                            ui,
                            if spearfia_vision::preset(v.name()).is_some() {
                                "Built-in"
                            } else {
                                "Custom"
                            },
                            theme::CUE_VISION,
                        );
                    });
                    ui.label(muted(v.goal()));
                    ui.label(mono(
                        format!(
                            "{} · {:.1}s shots · {} cuts",
                            v.name(),
                            v.avg_shot_seconds(),
                            v.cut_style()
                        ),
                        theme::TEXT_MUTED,
                    ));
                })
                .response
                .interact(egui::Sense::click());
                if response.clicked() {
                    app.catalog_view.visions.selected = Some(v.name().into());
                    app.catalog_view.visions.adding = false;
                }
                ui.add_space(6.0);
            }
        });
}

pub fn inspector(app: &mut App, ui: &mut egui::Ui) {
    if app.catalog_view.visions.adding {
        ui.label(strong("Add a vision", 15.0));
        ui.label(muted("Paste the JSON returned by your AI, or import its .json file. Each vision needs a unique name."));
        ui.horizontal(|ui| {
            if widgets::button(ui, "Import JSON…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Vision JSON", &["json"])
                    .pick_file()
            {
                match visions::read(&path).and_then(|v| v.to_source()) {
                    Ok(source) => {
                        app.catalog_view.visions.source = source;
                        app.catalog_view.visions.status = None;
                    }
                    Err(e) => app.catalog_view.visions.status = Some(e.to_string()),
                }
            }
            if widgets::button(ui, "Use example").clicked() {
                app.catalog_view.visions.source = EXAMPLE.into();
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("vision-json-editor")
            .max_height(360.0)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut app.catalog_view.visions.source)
                        .code_editor()
                        .desired_rows(22)
                        .desired_width(f32::INFINITY)
                        .char_limit(visions::MAX_BYTES),
                );
            });
        if widgets::primary_button(ui, "Save vision").clicked() {
            let result = save(app);
            match result {
                Ok(v) => {
                    app.catalog_view.visions.selected = Some(v.name().into());
                    app.visions.push(v);
                    app.catalog_view.visions.source.clear();
                    app.catalog_view.visions.adding = false;
                    app.catalog_view.visions.status = Some(
                        "Vision saved. It is now available in New Project and in catalog ranking."
                            .into(),
                    );
                }
                Err(e) => app.catalog_view.visions.status = Some(e.to_string()),
            }
        }
        if widgets::button(ui, "Cancel").clicked() {
            app.catalog_view.visions.adding = false;
        }
        return;
    }
    let selected = app
        .catalog_view
        .visions
        .selected
        .as_deref()
        .and_then(|name| app.visions.iter().find(|v| v.name() == name));
    let Some(v) = selected else {
        ui.label(muted(
            "Select a vision to inspect its brief and format, or add one of your own.",
        ));
        return;
    };
    ui.label(strong(v.title(), 15.0));
    ui.label(muted(v.goal()));
    ui.add_space(8.0);
    ui.label(muted("Choose this vision when creating a project. Its weights guide catalog ranking; its rules and references are preserved with the project."));
    match v.to_source() {
        Ok(source) => {
            if widgets::button(ui, "Copy vision JSON").clicked() {
                ui.ctx().copy_text(source.clone());
            }
            ui.add(egui::Label::new(mono(source, theme::TEXT_2)).wrap());
        }
        Err(e) => {
            ui.label(text(e.to_string(), 12.0, theme::DANGER));
        }
    }
}

fn save(app: &App) -> Result<Vision, spearfia_core::CoreError> {
    let v = Vision::parse(&app.catalog_view.visions.source)?;
    if app.visions.iter().any(|old| old.name() == v.name()) {
        return Err(spearfia_core::CoreError::Invalid(
            "That vision name is already in use. Choose another name.".into(),
        ));
    }
    let directory = app.config_dir.as_deref().ok_or_else(|| {
        spearfia_core::CoreError::Invalid("The app's configuration folder is unavailable.".into())
    })?;
    visions::add(directory, &app.catalog_view.visions.source)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_example_is_importable() {
        let v = Vision::parse(EXAMPLE).unwrap();
        assert_eq!(v.weight_map()["diagram"], 0.9);
        assert!(creation_prompt().contains(EXAMPLE));
        assert!(creation_prompt().contains("channel-analysis Markdown"));
    }
}
