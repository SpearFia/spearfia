//! Per-user app state kept outside any project: recently opened projects and catalog
//! sources. Stored as JSON in the user's config folder (`%APPDATA%\spearfia` on Windows,
//! `$XDG_CONFIG_HOME/spearfia` or `~/.config/spearfia` elsewhere). `SPEARFIA_CONFIG_DIR`
//! overrides the location.

use crate::error::{CoreError, Result};
use crate::project::{Project, write_json};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const RECENTS_FILE: &str = "recents.json";
const SETTINGS_FILE: &str = "settings.json";
const MAX_RECENTS: usize = 50;

/// The folder where app state lives. `None` only if no home folder can be found.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SPEARFIA_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("spearfia"))
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentProject {
    pub path: PathBuf,
    pub name: String,
    pub last_opened: u64,
}

/// Recently opened projects, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recents {
    pub projects: Vec<RecentProject>,
}

impl Recents {
    pub fn load_from(dir: &Path) -> Self {
        read_json(&dir.join(RECENTS_FILE)).unwrap_or_default()
    }

    pub fn save_to(&self, dir: &Path) -> Result<()> {
        write_json(&dir.join(RECENTS_FILE), self)
    }

    /// Move `project` to the top of the list, stamped with `now`.
    pub fn touch(&mut self, project: &Project, now: u64) {
        self.projects.retain(|r| r.path != project.root);
        self.projects.insert(
            0,
            RecentProject {
                path: project.root.clone(),
                name: project.name().to_owned(),
                last_opened: now,
            },
        );
        self.projects.truncate(MAX_RECENTS);
    }

    pub fn remove(&mut self, path: &Path) {
        self.projects.retain(|r| r.path != path);
    }
}

/// A folder the catalog scans for `*.asset.json` descriptors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSourceSetting {
    pub name: String,
    pub path: PathBuf,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// App settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub catalog_sources: Vec<CatalogSourceSetting>,
    /// Where new projects are created by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects_dir: Option<PathBuf>,
}

impl AppSettings {
    /// Settings from disk, or defaults (no catalog sources) on first run.
    pub fn load_from(dir: &Path) -> Self {
        read_json(&dir.join(SETTINGS_FILE)).unwrap_or_default()
    }

    pub fn save_to(&self, dir: &Path) -> Result<()> {
        write_json(&dir.join(SETTINGS_FILE), self)
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).map_err(|e| CoreError::io(path, e))?;
    serde_json::from_str(&text).map_err(|e| CoreError::json(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Project, ProjectFile, SCHEMA_URL, Settings};
    use crate::testutil::temp_dir;

    fn project(path: &str, name: &str) -> Project {
        Project {
            root: PathBuf::from(path),
            file: ProjectFile {
                schema: SCHEMA_URL.into(),
                name: name.into(),
                spec_version: 1,
                adapter: "filmcraft".into(),
                library_roots: vec![],
                script: "script/script.md".into(),
                vision: "vision.yaml".into(),
                settings: Settings::default(),
                series: None,
                format: Default::default(),
                target_runtime_seconds: None,
                media_folders: vec![],
                filmcraft: None,
            },
        }
    }

    #[test]
    fn touch_moves_to_top_without_duplicates() {
        let mut r = Recents::default();
        r.touch(&project("/a", "A"), 1);
        r.touch(&project("/b", "B"), 2);
        r.touch(&project("/a", "A"), 3);
        let names: Vec<_> = r.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["A", "B"]);
        assert_eq!(r.projects[0].last_opened, 3);
    }

    #[test]
    fn recents_and_settings_round_trip() {
        let dir = temp_dir("state");
        let mut r = Recents::default();
        r.touch(&project("/a", "A"), 7);
        r.save_to(&dir).unwrap();
        assert_eq!(Recents::load_from(&dir), r);

        let s = AppSettings {
            catalog_sources: vec![CatalogSourceSetting {
                name: "sfx".into(),
                path: "/sfx".into(),
                enabled: false,
            }],
            projects_dir: Some("/media".into()),
        };
        s.save_to(&dir).unwrap();
        assert_eq!(AppSettings::load_from(&dir), s);
    }

    #[test]
    fn missing_recents_file_is_empty() {
        assert!(Recents::load_from(&temp_dir("empty")).projects.is_empty());
    }
}
