//! Local source: a folder (on disk or the external drive) holding asset folders, each with
//! an `<id>.asset.json` descriptor next to its files.

use super::SourceKind;
use crate::{Asset, Catalog, Entry, LoadError};
use std::fs;
use std::path::{Path, PathBuf};

/// How deep to look below the root. Asset folders are shallow; this stops runaway scans
/// of a whole drive.
const MAX_DEPTH: usize = 8;
const DESCRIPTOR_SUFFIX: &str = ".asset.json";

#[derive(Debug, Clone)]
pub struct LocalSource {
    pub name: String,
    pub root: PathBuf,
}

impl LocalSource {
    pub fn new(name: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            root: root.into(),
        }
    }

    pub fn kind(&self) -> SourceKind {
        SourceKind::Local
    }

    /// Read every descriptor under the root into `catalog`.
    pub fn scan_into(&self, catalog: &mut Catalog) {
        self.walk(&self.root, 0, catalog);
    }

    fn walk(&self, dir: &Path, depth: usize, catalog: &mut Catalog) {
        let Ok(read) = fs::read_dir(dir) else {
            if depth == 0 {
                catalog.errors.push(LoadError {
                    path: dir.to_path_buf(),
                    message: "folder not found".into(),
                });
            }
            return;
        };
        let mut paths: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for path in paths {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            if path.is_dir() {
                if depth < MAX_DEPTH {
                    self.walk(&path, depth + 1, catalog);
                }
            } else if name.ends_with(DESCRIPTOR_SUFFIX) {
                match read_asset(&path) {
                    Ok(asset) => catalog.entries.push(Entry {
                        asset,
                        descriptor: path,
                        source: self.name.clone(),
                    }),
                    Err(message) => catalog.errors.push(LoadError { path, message }),
                }
            }
        }
    }
}

fn read_asset(path: &Path) -> Result<Asset, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}
