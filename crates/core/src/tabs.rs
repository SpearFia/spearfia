//! Document tabs: the Markdown files in the project's `script/` folder. The episode script
//! (`script.md`, or whatever `project.json` names) comes first; other tabs hold the series
//! bible, hook options, notes. Only the main script drives timing, media and sync.

use crate::error::{CoreError, Result};
use crate::project::{Project, write_text};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    /// Project-relative path, `/`-separated.
    pub rel: String,
    /// File name without `.md`, or the episode title for the main script.
    pub name: String,
    pub main: bool,
}

impl Project {
    /// The main script first, then the other `.md` files next to it, A–Z.
    pub fn tabs(&self) -> Vec<Tab> {
        let main = self.file.script.replace('\\', "/");
        let dir = Path::new(&main)
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let mut tabs = vec![Tab {
            name: self.name().to_owned(),
            rel: main.clone(),
            main: true,
        }];
        let mut others: Vec<Tab> = fs::read_dir(self.resolve(&dir))
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let file = e.file_name().to_string_lossy().into_owned();
                let stem = file.strip_suffix(".md")?.to_owned();
                let rel = if dir.is_empty() {
                    file.clone()
                } else {
                    format!("{dir}/{file}")
                };
                (rel != main && !file.starts_with('.')).then_some(Tab {
                    name: stem,
                    rel,
                    main: false,
                })
            })
            .collect();
        others.sort_by_key(|t| t.name.to_lowercase());
        tabs.extend(others);
        tabs
    }

    pub fn read_doc(&self, rel: &str) -> Result<String> {
        let path = self.resolve(rel);
        fs::read_to_string(&path).map_err(|e| CoreError::io(&path, e))
    }

    pub fn write_doc(&self, rel: &str, text: &str) -> Result<()> {
        write_text(&self.resolve(rel), text)
    }

    /// Create a new tab next to the main script and return its path.
    pub fn new_tab(&self, name: &str) -> Result<String> {
        let clean: String = name
            .trim()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == ' ' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        if clean.trim().is_empty() {
            return Err(CoreError::Invalid("Give the tab a name.".into()));
        }
        let main = self.file.script.replace('\\', "/");
        let dir = Path::new(&main)
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let rel = if dir.is_empty() {
            format!("{}.md", clean.trim())
        } else {
            format!("{dir}/{}.md", clean.trim())
        };
        if self.resolve(&rel).exists() {
            return Err(CoreError::Invalid(format!("{rel} already exists.")));
        }
        self.write_doc(&rel, &format!("# {}\n\n", name.trim()))?;
        Ok(rel)
    }
}

#[cfg(test)]
mod tests {
    use crate::project::{
        Format, MediaFolder, NewProject, NewVision, Project, ScriptStart, Timebase, create_project,
    };
    use crate::testutil::temp_dir;

    fn project() -> Project {
        create_project(&NewProject {
            name: "Ep 1".into(),
            series: None,
            root: temp_dir("tabs").join("P"),
            format: Format::LongForm,
            resolution: [1920, 1080],
            timebase: Timebase::FPS_60,
            target_runtime_seconds: None,
            vision: NewVision {
                extends: None,
                goal: String::new(),
                avg_shot_seconds: None,
                cut_style: None,
            },
            script: ScriptStart::Sections,
            media_folders: MediaFolder::defaults(),
            filmcraft: None,
        })
        .unwrap()
    }

    #[test]
    fn lists_and_creates_tabs() {
        let p = project();
        assert_eq!(p.tabs().len(), 1);
        assert!(p.tabs()[0].main);
        let rel = p.new_tab("Series bible").unwrap();
        assert_eq!(rel, "script/Series bible.md");
        assert!(p.new_tab("Series bible").is_err());
        assert!(p.new_tab("  ").is_err());
        p.new_tab("Hook options").unwrap();
        let names: Vec<String> = p.tabs().into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["Ep 1", "Hook options", "Series bible"]);
        assert!(p.read_doc(&rel).unwrap().starts_with("# Series bible"));
    }
}
