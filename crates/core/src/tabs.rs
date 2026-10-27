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
        self.document_tabs(&self.file.script, self.name())
    }

    pub fn document_tabs(&self, main: &str, title: &str) -> Vec<Tab> {
        let main = main.replace('\\', "/");
        let dir = Path::new(&main)
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let mut tabs = vec![Tab {
            name: title.to_owned(),
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

    /// Serialize cooperating writers and compare the exact bytes under the OS lock.
    pub fn write_doc_expected(&self, rel: &str, expected: &str, text: &str) -> Result<()> {
        let _lock = self.edit_lock()?;
        let current = if rel == crate::visuals::FILE {
            self.read_visuals()?
        } else {
            self.read_doc(rel)?
        };
        if current != expected {
            return Err(CoreError::Invalid(
                "The document changed before saving. Read it again and reconcile your edit.".into(),
            ));
        }
        write_text(&self.resolve(rel), text)
    }

    /// The project-wide OS lock cooperating writers (desktop, MCP, HTTP) hold while they
    /// compare and save. Released when the returned file is dropped.
    pub fn edit_lock(&self) -> Result<fs::File> {
        let root = self
            .root
            .canonicalize()
            .map_err(|e| CoreError::io(&self.root, e))?;
        let lock_path = root.join(".spearfia-edit.lock");
        match fs::symlink_metadata(&lock_path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(CoreError::Invalid(
                    "The project edit lock must be an ordinary file, not a symlink.".into(),
                ));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(CoreError::io(&lock_path, e)),
        }
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| CoreError::io(&lock_path, e))?;
        lock.try_lock().map_err(|e| CoreError::Invalid(format!("Another document save is active or locking failed ({e}). Read the document and retry.")))?;
        Ok(lock)
    }

    /// Create a new tab next to the main script and return its path.
    pub fn new_tab(&self, name: &str) -> Result<String> {
        self.new_document_tab(&self.file.script, name)
    }

    pub fn new_document_tab(&self, main: &str, name: &str) -> Result<String> {
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
        let main = main.replace('\\', "/");
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
            channel_brief: None,
            name: "Ep 1".into(),
            series: None,
            root: temp_dir("tabs").join("P"),
            format: Format::LongForm,
            resolution: [1920, 1080],
            timebase: Timebase::FPS_60,
            target_runtime_seconds: None,
            vision: NewVision {
                template: None,
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
