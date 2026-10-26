//! Shared channel Markdown library and per-video approval/handoff.
use crate::app::App;
use crate::screens::workspace::Workspace;
use crate::theme;
use crate::widgets::{self, muted, panel, strong, text};
use egui::{Align, Layout, Margin, Panel};
use spearfia_core::channels::{self, Channel, Document};

#[derive(Default)]
pub struct ChannelView {
    selected: Option<String>,
    documents: Vec<Document>,
    document: usize,
    original: String,
    draft: String,
    new_name: String,
    status: Option<String>,
    closing: bool,
}

impl ChannelView {
    pub fn dirty(&self) -> bool {
        self.draft != self.original
    }

    fn select(&mut self, channel: &Channel) -> Result<(), String> {
        let docs = channel.documents().map_err(|e| e.to_string())?;
        self.selected = Some(channel.id.clone());
        self.documents = docs;
        self.select_document(0);
        self.status = None;
        Ok(())
    }

    fn select_document(&mut self, index: usize) {
        self.document = index;
        self.original = self
            .documents
            .get(index)
            .map(|d| d.text.clone())
            .unwrap_or_default();
        self.draft = self.original.clone();
    }
}

pub fn reload(app: &mut App) {
    let Some(config) = &app.config_dir else {
        app.channel_error = Some("The channel library needs an app config folder.".into());
        return;
    };
    match channels::list(config) {
        Ok(channels) => {
            app.channels = channels;
            app.channel_error = None;
        }
        Err(e) => app.channel_error = Some(e.to_string()),
    }
}

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    let mut chosen = None;
    let mut create = false;
    let mut refresh = false;
    let dirty = app.channel_view.dirty();
    Panel::left("channel-library")
        .exact_size(280.0)
        .frame(panel(theme::BG_PANEL).inner_margin(Margin::same(16)))
        .show(ui, |ui| {
            ui.heading("Channels");
            ui.label(muted(
                "Shared identity, staple assets and lessons for every video on a channel.",
            ));
            ui.add_space(10.0);
            ui.add(
                egui::TextEdit::singleline(&mut app.channel_view.new_name)
                    .hint_text("New channel name")
                    .desired_width(f32::INFINITY)
                    .char_limit(120),
            );
            ui.horizontal(|ui| {
                create = ui
                    .add_enabled(!dirty, egui::Button::new("+ Create channel"))
                    .clicked();
                refresh = ui
                    .add_enabled(!dirty, egui::Button::new("Reload"))
                    .clicked();
            });
            if dirty {
                ui.label(muted(
                    "Save or discard your draft before switching documents or channels.",
                ));
            }
            if let Some(error) = &app.channel_error {
                ui.label(text(error, 12.0, theme::DANGER));
            }
            ui.add_space(12.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                for channel in &app.channels {
                    if ui
                        .add_enabled(
                            !dirty,
                            egui::Button::new(&channel.title).selected(
                                app.channel_view.selected.as_deref() == Some(&channel.id),
                            ),
                        )
                        .clicked()
                    {
                        chosen = Some(channel.clone());
                    }
                }
                if app.channels.is_empty() {
                    ui.label(muted(
                        "Create your first channel to start its Markdown guides.",
                    ));
                }
            });
        });
    if create {
        let result = app
            .config_dir
            .as_deref()
            .ok_or_else(|| "No config folder is available.".to_owned())
            .and_then(|config| {
                channels::create(config, &app.channel_view.new_name).map_err(|e| e.to_string())
            });
        match result {
            Ok(channel) => {
                app.channel_view.new_name.clear();
                reload(app);
                chosen = Some(channel);
            }
            Err(e) => app.channel_view.status = Some(e),
        }
    }
    if refresh {
        reload(app);
        chosen = app
            .channels
            .iter()
            .find(|c| Some(&c.id) == app.channel_view.selected.as_ref())
            .cloned();
    }
    if let Some(channel) = chosen
        && let Err(e) = app.channel_view.select(&channel)
    {
        app.channel_view.status = Some(e);
    }
    let channel = app
        .channels
        .iter()
        .find(|c| Some(&c.id) == app.channel_view.selected.as_ref())
        .cloned();
    egui::CentralPanel::default().frame(panel(theme::BG_APP).inner_margin(Margin::same(20))).show(ui, |ui| {
        let Some(channel) = channel else {
            ui.heading("A home for each channel's knowledge");
            ui.label(muted("Channel guides are ordinary .md files. Keep channel analyses, recurring music/SFX/graphics, production settings and review lessons together. Choose a Channel when creating a video; use its Vision for the video's creative direction."));
            if let Some(status) = &app.channel_view.status { ui.label(text(status, 13.0, theme::DANGER)); }
            return;
        };
        editor(app, ui, &channel);
    });
}

fn editor(app: &mut App, ui: &mut egui::Ui, channel: &Channel) {
    ui.horizontal(|ui| {
        ui.heading(&channel.title);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.add_enabled(!app.channel_view.dirty(), egui::Button::new("New video for channel")).clicked() {
                app.start_new_project();
                if let crate::app::View::NewProject(form) = &mut app.view
                    && let Err(e) = form.choose_channel(&channel.id) { app.error = Some(e); }
            }
            if widgets::button(ui, "Open folder").clicked() { crate::reveal(&channel.root); }
            if ui.add_enabled(!app.channel_view.dirty(), egui::Button::new("Copy agent context")).clicked() {
                match channel.brief() {
                    Ok(brief) => { ui.ctx().copy_text(brief); app.channel_view.status = Some("Channel context copied. Drafting agents must still wait for script approval before production.".into()); }
                    Err(e) => app.channel_view.status = Some(e.to_string()),
                }
            }
        });
    });
    ui.label(muted(channel.root.display().to_string()));
    ui.horizontal_wrapped(|ui| {
        let mut index = None;
        for (i, doc) in app.channel_view.documents.iter().enumerate() {
            if ui
                .add_enabled(
                    !app.channel_view.dirty(),
                    egui::Button::new(&doc.name).selected(i == app.channel_view.document),
                )
                .clicked()
            {
                index = Some(i);
            }
        }
        if let Some(i) = index {
            app.channel_view.select_document(i);
        }
    });
    ui.horizontal(|ui| {
        if ui.add_enabled(app.channel_view.dirty(), egui::Button::new("Save document")).clicked() {
            let v = &mut app.channel_view;
            if let Some(doc) = v.documents.get(v.document) {
                match channel.save_document(&doc.name, &v.original, &v.draft) {
                    Ok(()) => {
                        v.original = v.draft.clone();
                        if let Some(doc) = v.documents.get_mut(v.document) { doc.text = v.draft.clone(); }
                        v.status = Some("Saved. Existing videos keep their channel brief until you explicitly refresh it.".into());
                    }
                    Err(e) => v.status = Some(e.to_string()),
                }
            }
        }
        if widgets::button(ui, "Discard / reload").clicked() {
            let index = app.channel_view.document;
            match app.channel_view.select(channel) {
                Ok(()) => app.channel_view.select_document(index),
                Err(e) => app.channel_view.status = Some(e),
            }
        }
        if ui.add_enabled(!app.channel_view.dirty(), egui::Button::new("Import Markdown…")).clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Markdown", &["md"]).pick_file()
        {
            let name = format!("{}.md", spearfia_core::project::slug(path.file_stem().and_then(|s| s.to_str()).unwrap_or("notes")));
            let result = channels::read_limited(&path, channels::MAX_DOCUMENT_BYTES)
                .and_then(|source| channel.add_document(&name, &source));
            match result {
                Ok(()) => {
                    if let Err(e) = app.channel_view.select(channel) { app.channel_view.status = Some(e); }
                    if let Some(i) = app.channel_view.documents.iter().position(|d| d.name == name) { app.channel_view.select_document(i); }
                }
                Err(e) => app.channel_view.status = Some(e.to_string()),
            }
        }
    });
    if let Some(status) = &app.channel_view.status {
        ui.label(text(status, 12.5, theme::TEXT_2));
    }
    ui.add_space(8.0);
    egui::ScrollArea::vertical()
        .id_salt("channel-markdown")
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut app.channel_view.draft)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(35)
                    .char_limit(channels::MAX_DOCUMENT_BYTES),
            );
        });
}

#[derive(Default)]
pub struct ProjectChannelView {
    pub open: bool,
    selected: Option<String>,
    brief: Option<String>,
    status: String,
    error: Option<String>,
}

impl ProjectChannelView {
    pub fn refresh(&mut self, project: &spearfia_core::project::Project, source: &str) {
        self.error = None;
        match project.channel_brief() {
            Ok(brief) => self.brief = brief,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        }
        match project.script_approval(Some(source)) {
            Ok(state) => self.status = state.label().into(),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}

pub fn project_window(ws: &mut Workspace, channels: &[Channel], ui: &mut egui::Ui) {
    if !ws.channel.open {
        return;
    }
    let mut open = true;
    egui::Window::new("Channel and agent handoff").open(&mut open).default_size([720.0, 620.0]).show(ui.ctx(), |ui| {
        let v = &mut ws.channel;
        ui.label(muted("A video keeps a channel brief snapshot. Refreshing the brief or editing the approved script requires another approval."));
        egui::ComboBox::from_id_salt("project-channel").selected_text(channels.iter().find(|c| Some(&c.id) == v.selected.as_ref()).map_or("Choose a channel", |c| c.title.as_str())).show_ui(ui, |ui| {
            for channel in channels { ui.selectable_value(&mut v.selected, Some(channel.id.clone()), &channel.title); }
        });
        let selected = channels.iter().find(|c| Some(&c.id) == v.selected.as_ref());
        if ui.add_enabled(selected.is_some(), egui::Button::new(if v.brief.is_some() { "Replace / refresh from selected channel" } else { "Attach channel" })).clicked()
            && let Some(channel) = selected
        {
            match channel.brief().and_then(|brief| ws.project.set_channel_brief(v.brief.as_deref(), &brief)) {
                Ok(()) => v.refresh(&ws.project, ws.script.source()), Err(e) => v.error = Some(e.to_string()),
            }
        }
        ui.separator();
        ui.label(strong(&v.status, 13.0));
        ui.horizontal(|ui| {
            if ui.button("Approve saved script for production").clicked() {
                match ws.project.approve_script(ws.script.source()) {
                    Ok(()) => v.refresh(&ws.project, ws.script.source()), Err(e) => v.error = Some(e.to_string()),
                }
            }
            if ui.button("Revoke approval").clicked() {
                match ws.project.revoke_script_approval() {
                    Ok(()) => v.refresh(&ws.project, ws.script.source()), Err(e) => v.error = Some(e.to_string()),
                }
            }
            if ui.button("Refresh status").clicked() { v.refresh(&ws.project, ws.script.source()); }
        });
        ui.horizontal(|ui| {
            for (production, label) in [(false, "Copy drafting brief"), (true, "Copy production handoff")] {
                if ui.button(label).clicked() {
                    match ws.project.agent_brief(production, Some(ws.script.source())) {
                        Ok(brief) => { ui.ctx().copy_text(brief); v.error = None; v.status = if production { "Production handoff copied." } else { "Drafting brief copied. Approval is still required before production." }.into(); }
                        Err(e) => { v.error = Some(e.to_string()); }
                    }
                }
            }
        });
        if let Some(e) = &v.error { ui.label(text(e, 12.5, theme::DANGER)); }
        if let Some(brief) = &v.brief {
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                let mut text = brief.as_str();
                ui.add(egui::TextEdit::multiline(&mut text).code_editor().desired_width(f32::INFINITY));
            });
        } else { ui.label(muted("Create a channel in the home screen's Channels tab, then attach it here.")); }
        ui.label(muted("This hands instructions to an agent. It does not launch generators or transfer a FilmCraft timeline automatically."));
    });
    ws.channel.open = open;
}

pub fn guard_close(app: &mut App, ctx: &egui::Context) {
    if ctx.input(|i| i.viewport().close_requested()) && app.channel_view.dirty() {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        app.channel_view.closing = true;
    }
    if !app.channel_view.closing {
        return;
    }
    egui::Window::new("Unsaved channel document")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Save or discard your channel draft before closing Spear Fia.");
            if let Some(error) = &app.channel_view.status {
                ui.label(text(error, 12.5, theme::DANGER));
            }
            ui.horizontal(|ui| {
                if ui.button("Save and close").clicked() {
                    let v = &mut app.channel_view;
                    let result = app
                        .channels
                        .iter()
                        .find(|c| Some(&c.id) == v.selected.as_ref())
                        .ok_or_else(|| {
                            "The channel is no longer available. Copy your draft before closing."
                                .to_owned()
                        })
                        .and_then(|channel| {
                            v.documents
                                .get(v.document)
                                .ok_or_else(|| "The document is no longer available.".to_owned())
                                .and_then(|doc| {
                                    channel
                                        .save_document(&doc.name, &v.original, &v.draft)
                                        .map_err(|e| e.to_string())
                                })
                        });
                    match result {
                        Ok(()) => {
                            v.original = v.draft.clone();
                            v.closing = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        Err(e) => v.status = Some(e),
                    }
                }
                if ui.button("Discard and close").clicked() {
                    app.channel_view.draft = app.channel_view.original.clone();
                    app.channel_view.closing = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Keep editing").clicked() {
                    app.channel_view.closing = false;
                }
                if ui.button("Copy draft").clicked() {
                    ctx.copy_text(app.channel_view.draft.clone());
                }
            });
        });
}
