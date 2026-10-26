//! App state and screen routing.

use crate::screens::{
    catalog::CatalogView, new_project::NewProjectForm, projects::CardInfo, workspace::Workspace,
};
use crate::{screens, theme};
use spearfia_adapter_filmcraft::Install;
use spearfia_catalog::{Catalog, LocalSource};
use spearfia_core::app_state::{self, AppSettings, Recents};
use spearfia_core::project::Project;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeTab {
    Projects,
    Catalog,
    Channels,
}

pub enum View {
    /// The project browser / catalog (the screen the app opens on).
    Home,
    NewProject(Box<NewProjectForm>),
    Workspace(Box<Workspace>),
}

pub struct App {
    pub config_dir: Option<PathBuf>,
    pub recents: Recents,
    pub settings: AppSettings,
    pub catalog: Catalog,
    pub visions: Vec<spearfia_core::visions::Vision>,
    pub channels: Vec<spearfia_core::channels::Channel>,
    pub channel_view: screens::channels::ChannelView,
    pub channel_error: Option<String>,
    pub vision_errors: Vec<String>,
    pub filmcraft: Option<Install>,
    pub view: View,
    pub home_tab: HomeTab,
    /// Index into `recents.projects` of the selected card.
    pub selected: Option<usize>,
    pub project_search: String,
    /// Card details per project folder, read on first view. `None` = unreadable.
    pub cards: HashMap<PathBuf, Option<CardInfo>>,
    pub catalog_view: CatalogView,
    /// Shown as a banner until dismissed.
    pub error: Option<String>,
    pub now: u64,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let config_dir = app_state::config_dir();
        let (recents, settings) = match &config_dir {
            Some(d) => (Recents::load_from(d), AppSettings::load_from(d)),
            None => Default::default(),
        };
        let mut app = Self {
            config_dir,
            recents,
            settings,
            catalog: Catalog::default(),
            visions: Vec::new(),
            channels: Vec::new(),
            channel_view: screens::channels::ChannelView::default(),
            channel_error: None,
            vision_errors: Vec::new(),
            filmcraft: spearfia_adapter_filmcraft::detect(),
            view: View::Home,
            home_tab: HomeTab::Projects,
            selected: None,
            project_search: String::new(),
            cards: HashMap::new(),
            catalog_view: CatalogView::default(),
            error: None,
            now: app_state::now_unix(),
        };
        app.reload_catalog();
        screens::channels::reload(&mut app);
        if !app.recents.projects.is_empty() {
            app.selected = Some(0);
        }
        app
    }

    pub fn reload_catalog(&mut self) {
        let sources: Vec<LocalSource> = self
            .settings
            .catalog_sources
            .iter()
            .filter(|s| s.enabled)
            .map(|s| LocalSource::new(&s.name, &s.path))
            .collect();
        self.catalog = Catalog::load(&sources);
        self.visions = spearfia_vision::builtin_templates();
        self.vision_errors.clear();
        if let Some(directory) = &self.config_dir {
            let (custom, errors) = spearfia_core::visions::load(directory);
            self.vision_errors = errors;
            for v in custom {
                if self.visions.iter().any(|p| p.name() == v.name()) {
                    self.vision_errors
                        .push(format!("Vision name {} is already in use.", v.name()));
                } else {
                    self.visions.push(v);
                }
            }
        }
    }

    pub fn save_state(&mut self) {
        let Some(dir) = &self.config_dir else { return };
        let r = self
            .recents
            .save_to(dir)
            .and_then(|()| self.settings.save_to(dir));
        if let Err(e) = r {
            self.error = Some(format!("Couldn't save app settings: {e}"));
        }
    }

    pub fn open_project(&mut self, root: &Path) {
        match Project::open(root) {
            Ok(project) => self.enter_workspace(project),
            Err(e) => self.error = Some(format!("Couldn't open the project: {e}")),
        }
    }

    pub fn enter_workspace(&mut self, project: Project) {
        self.recents.touch(&project, app_state::now_unix());
        self.selected = Some(0);
        self.save_state();
        self.view = View::Workspace(Box::new(Workspace::new(project)));
    }

    pub fn go_home(&mut self) {
        self.view = View::Home;
    }

    pub fn start_new_project(&mut self) {
        let form = NewProjectForm::new(
            self.settings.projects_dir.clone(),
            self.series_hint(),
            self.visions.clone(),
            self.channels.clone(),
        );
        self.view = View::NewProject(Box::new(form));
    }

    /// The series of the selected project, so "New Project" defaults to its next episode.
    fn series_hint(&self) -> Option<String> {
        let r = self.recents.projects.get(self.selected?)?;
        Project::open(&r.path).ok()?.file.series
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.now = app_state::now_unix();
        match &self.view {
            View::Home => screens::home::show(self, ui),
            View::NewProject(_) => screens::new_project::show(self, ui),
            View::Workspace(_) => screens::workspace::show(self, ui),
        }
        screens::channels::guard_close(self, ui.ctx());
    }
}
