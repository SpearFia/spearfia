// SPDX-License-Identifier: MIT OR Apache-2.0
//! Creator controls for the shared production coordinator.
use crate::{theme, widgets};
use spearfia_core::{channels::Approval, project::Project};
use spearfia_ops::{
    Origin,
    production::{self, Action, Pipeline, Provider, Run, State},
};
use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, TryRecvError},
};

#[derive(Default)]
pub struct Production {
    config: Option<Pipeline>,
    revision: String,
    run: Option<Run>,
    selected: usize,
    reply: String,
    error: String,
    dirty: bool,
    worker: Option<Receiver<Result<Run, String>>>,
    active_project: Option<Project>,
    checked: f64,
}

impl Drop for Production {
    fn drop(&mut self) {
        if self.worker.is_some()
            && let Some(project) = &self.active_project
        {
            let _ = production::request_stop(project);
        }
    }
}
impl Production {
    fn reload(&mut self, project: &Project) {
        match production::load_pipeline(project) {
            Ok((config, revision)) => {
                self.config = Some(config);
                self.revision = revision;
                self.dirty = false;
                self.error.clear();
            }
            Err(e) if !project.root.join(production::CONFIG).exists() => {
                self.config = Some(production::defaults());
                self.revision = spearfia_ops::version("");
                self.dirty = true;
                let _ = e;
            }
            Err(e) => {
                self.error = e.to_string();
                self.config = None;
            }
        }
        self.read_run(project);
    }
    fn read_run(&mut self, project: &Project) {
        match production::load_run(project) {
            Ok(run) => self.run = run,
            Err(e) => self.error = e.to_string(),
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui, project: &Project, source: &str) {
        if self.config.is_none() && self.error.is_empty() {
            self.reload(project);
        }
        if let Some(worker) = &self.worker {
            match worker.try_recv() {
                Ok(result) => {
                    self.worker = None;
                    self.active_project = None;
                    match result {
                        Ok(run) => self.run = Some(run),
                        Err(e) => self.error = e,
                    };
                }
                Err(TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.active_project = None;
                    self.error="Production worker stopped. Inspect the checkpoint and logs before retrying.".into();
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let now = ui.ctx().input(|i| i.time);
        if now - self.checked > 0.5 {
            self.checked = now;
            self.read_run(project);
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(500));
        let mut action = None;
        let mut reload = false;
        egui::CentralPanel::default().frame(widgets::panel(theme::BG_APP).inner_margin(24)).show(ui, |ui| {
            ui.heading("Production");
            ui.label("Assign agents to stages, review their work, and continue from the saved checkpoint.");
            let approved=matches!(project.script_approval(Some(source)), Ok(Approval::Approved));
            ui.horizontal(|ui| {
                ui.label(if approved { "Saved script approved" } else { "Script needs creator approval before production" });
                if !approved && ui.button("Approve saved script").clicked()
                    && let Err(e)=project.approve_script(source) { self.error=e.to_string(); }
            });
            if let Some(run)=&self.run {
                ui.separator();
                ui.label(format!("{:?}: {}", run.state, run.message));
            }
            if !self.error.is_empty() { ui.colored_label(theme::DANGER, &self.error); }
            let busy=self.worker.is_some() || production::is_running(project).unwrap_or(true);
            ui.horizontal(|ui| {
                if ui.add_enabled(!busy, egui::Button::new("Reload assignments")).clicked() { reload=true; }
                if ui.add_enabled(!busy && self.dirty && self.config.is_some(), egui::Button::new("Save assignments")).clicked()
                    && let Some(config)=&self.config {
                    match production::save_pipeline(project,config,&self.revision,Origin::App) {
                        Ok(version)=>{self.revision=version;self.dirty=false;self.error.clear();}, Err(e)=>self.error=e.to_string(),
                    }
                }
                if ui.add_enabled(!busy && !self.dirty && self.config.is_some(), egui::Button::new("Start new run")).clicked() {
                    action=Some(Action {start:true,..Default::default()});
                }
                if busy && ui.button("Stop run").clicked() && let Err(e)=production::request_stop(project) { self.error=e.to_string(); }
                if ui.button("Open production folder").clicked() { crate::reveal(&project.root.join("sync/production")); }
            });
            if !busy && let Some(run)=&self.run && run.state!=State::Complete {
                let current=run.steps.iter().find(|s| s.state!=State::Complete);
                let review=current.is_some_and(|s|s.state==State::Review);
                let retry=current.is_some_and(|s|matches!(s.state,State::Failed|State::Cancelled|State::Running));
                let needs_input=current.is_some_and(|s|s.state==State::NeedsInput);
                ui.label(if needs_input { "Agent question — enter your reply:" } else { "Review feedback or instructions for continuing:" });
                ui.text_edit_multiline(&mut self.reply);
                let label=if review {"Accept review and continue"} else if retry {"Retry after inspecting logs"} else {"Continue"};
                if ui.add_enabled(!self.dirty && (!needs_input || !self.reply.trim().is_empty()), egui::Button::new(label)).clicked() {
                    action=Some(Action {start:false,accept_review:review,retry,reply:self.reply.clone()});
                }
            }
            ui.separator();
            egui::ScrollArea::vertical().id_salt("production-editor").show(ui, |ui| {
                let Some(config)=&mut self.config else { return };
                let mut move_stage = 0i32;
                let mut remove_stage = false;
                ui.columns(2, |columns| {
                    columns[0].heading("Stages");
                    for (index,stage) in config.stages.iter().enumerate() {
                        let status=self.run.as_ref().and_then(|r|r.steps.get(index)).filter(|s|s.id==stage.id).map(|s|format!(" · {:?}",s.state)).unwrap_or_default();
                        if columns[0].selectable_label(self.selected==index,format!("{}. {}{}",index+1,stage.title,status)).clicked() {self.selected=index;}
                    }
                    columns[0].add_enabled_ui(!busy, |ui| {
                        if ui.add_enabled(self.selected>0,egui::Button::new("Move earlier")).clicked(){move_stage = -1;}
                        if ui.add_enabled(self.selected+1<config.stages.len(),egui::Button::new("Move later")).clicked(){move_stage=1;}
                        if ui.add_enabled(config.stages.len()>1,egui::Button::new("Remove stage")).clicked(){remove_stage=true;}
                    });
                    self.selected=self.selected.min(config.stages.len().saturating_sub(1));
                    let Some(stage)=config.stages.get_mut(self.selected) else{return};
                    let ui=&mut columns[1];
                    ui.heading(&stage.title);
                    if let Some(step)=self.run.as_ref().and_then(|r|r.steps.iter().find(|s|s.id==stage.id)) {
                        ui.label(&step.message);
                        if !step.log.is_empty() && ui.button("Open stage logs").clicked() { crate::reveal(&project.root.join(&step.log)); }
                    }
                    ui.add_enabled_ui(!busy, |ui| {
                        egui::ComboBox::from_id_salt("production-provider").selected_text(format!("{:?}",stage.agent.provider)).show_ui(ui,|ui| {
                            self.dirty|=ui.selectable_value(&mut stage.agent.provider,Provider::Claude,"Claude Code").changed();
                            self.dirty|=ui.selectable_value(&mut stage.agent.provider,Provider::Codex,"Codex").changed();
                        });
                        ui.label("Model (empty uses the CLI's configured model)");
                        self.dirty|=ui.text_edit_singleline(&mut stage.agent.model).changed();
                        ui.label("Reasoning effort (empty uses CLI default)");
                        self.dirty|=ui.text_edit_singleline(&mut stage.agent.effort).changed();
                        ui.label("Stage instructions");
                        self.dirty|=ui.add(egui::TextEdit::multiline(&mut stage.prompt).desired_rows(9).desired_width(f32::INFINITY)).changed();
                        self.dirty|=ui.checkbox(&mut stage.production,"Requires approved script").changed();
                        self.dirty|=ui.checkbox(&mut stage.review,"Pause for creator review after completion").changed();
                        ui.collapsing("Advanced assignment",|ui| {
                            ui.label("Native CLI executable (empty discovers installed CLI)");
                            self.dirty|=ui.text_edit_singleline(&mut stage.agent.executable).changed();
                            ui.horizontal(|ui|{ ui.label("Timeout, seconds"); self.dirty|=ui.add(egui::DragValue::new(&mut stage.agent.timeout_seconds).range(1..=86400)).changed(); });
                        });
                        ui.label("Output checks");
                        let mut remove_check=None;
                        let count=stage.checks.len();
                        for (index,check) in stage.checks.iter_mut().enumerate() {
                            ui.push_id(index,|ui| {
                                match check {
                                    production::Check::File {path}=>{ui.label("Required file");self.dirty|=ui.text_edit_singleline(path).changed();},
                                    production::Check::Links {role,minimum,coverage}=>{
                                        ui.label(format!("Required {} links",role.label()));
                                        self.dirty|=ui.add(egui::DragValue::new(minimum).range(1..=5000)).changed();
                                        if *role==spearfia_core::project::MediaRole::Voiceover{self.dirty|=ui.checkbox(coverage,"Cover every narration line").changed();}
                                    },
                                    other=>{ui.monospace(serde_json::to_string(other).unwrap_or_default());},
                                }
                                if count>1 && ui.small_button("Remove check").clicked(){remove_check=Some(index);}
                            });
                        }
                        if let Some(index)=remove_check{stage.checks.remove(index);self.dirty=true;}
                        if stage.checks.len()<32 && ui.small_button("Add required file").clicked(){stage.checks.push(production::Check::File{path:"production/output.md".into()});self.dirty=true;}
                    });
                });
                if remove_stage{config.stages.remove(self.selected);self.selected=self.selected.min(config.stages.len()-1);self.dirty=true;}
                else if move_stage!=0{let next=(self.selected as i32+move_stage) as usize;config.stages.swap(self.selected,next);self.selected=next;self.dirty=true;}
            });
        });
        if reload {
            self.reload(project);
        }
        if let Some(action) = action {
            self.start(project, action, ui.ctx().clone());
        }
    }
    fn start(&mut self, project: &Project, action: Action, ctx: egui::Context) {
        let result = (|| {
            let project = Project::open(project.root.canonicalize().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let cli = exe
                .parent()
                .map(|p| p.join(format!("spearfia{}", std::env::consts::EXE_SUFFIX)))
                .unwrap_or_else(PathBuf::new);
            if !cli.is_file() {
                return Err("Build/install the spearfia CLI beside the desktop executable before running production.".into());
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let active = project.clone();
            std::thread::Builder::new()
                .name("spearfia-production".into())
                .spawn(move || {
                    let result =
                        std::panic::catch_unwind(|| production::execute(&project, &cli, action))
                            .map_err(|_| {
                                "Production worker failed; inspect logs before retrying.".to_owned()
                            })
                            .and_then(|r| r.map_err(|e| e.to_string()));
                    let _ = tx.send(result);
                    ctx.request_repaint();
                })
                .map_err(|e| e.to_string())?;
            self.worker = Some(rx);
            self.active_project = Some(active);
            self.error.clear();
            self.reply.clear();
            Ok::<_, String>(())
        })();
        if let Err(e) = result {
            self.error = e;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn production_panel_loads_saved_assignments_without_starting_agents() {
        let root = std::env::temp_dir().join(format!(
            "spearfia-production-ui-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("project.json"),
            r#"{"name":"Test","spec_version":1,"adapter":"filmcraft"}"#,
        )
        .unwrap();
        let project = Project::open(root).unwrap();
        let config = production::defaults();
        production::save_pipeline(&project, &config, &spearfia_ops::version(""), Origin::App)
            .unwrap();
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let mut view = Production::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 1000.0),
                )),
                ..Default::default()
            },
            |ui| view.show(ui, &project, ""),
        );
        output.textures_delta.clear(); // Headless verification intentionally has no GPU renderer.
        assert!(!output.shapes.is_empty());
        assert!(view.error.is_empty());
        assert_eq!(view.config.as_ref().unwrap().stages.len(), 11);
        assert!(view.worker.is_none());
        assert!(production::load_run(&project).unwrap().is_none());
    }
}
