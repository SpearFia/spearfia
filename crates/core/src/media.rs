//! The project's media folders on disk: a folder tree with each folder's role, and the
//! media files inside. Subfolders inherit the role of the folder they're in.

use crate::error::{CoreError, Result};
use crate::project::{MediaFolder, MediaRole, Project};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const MAX_DEPTH: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Audio,
    Image,
}

impl MediaKind {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" | "mxf" => Self::Video,
            "wav" | "mp3" | "aac" | "m4a" | "flac" | "ogg" | "opus" => Self::Audio,
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "tif" | "tiff" => Self::Image,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Image => "image",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFile {
    /// Path as stored in links: project-relative when inside the project, `/`-separated.
    pub path: String,
    pub name: String,
    pub kind: MediaKind,
    pub role: MediaRole,
    pub bytes: u64,
}

/// A folder in the media tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderNode {
    /// Same form as [`MediaFile::path`].
    pub path: String,
    pub name: String,
    pub role: MediaRole,
    /// False if the folder recorded in `project.json` is missing on disk.
    pub exists: bool,
    pub children: Vec<FolderNode>,
    /// Media files directly in this folder.
    pub files: Vec<MediaFile>,
}

impl FolderNode {
    /// Media files in this folder and all subfolders.
    pub fn all_files(&self) -> Vec<&MediaFile> {
        let mut out: Vec<&MediaFile> = self.files.iter().collect();
        for c in &self.children {
            out.extend(c.all_files());
        }
        out
    }

    pub fn find(&self, path: &str) -> Option<&FolderNode> {
        if self.path == path {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(path))
    }
}

/// `/`-separated path relative to the project root, or absolute if outside it.
pub fn display_path(project: &Project, abs: &Path) -> String {
    let p = abs.strip_prefix(&project.root).unwrap_or(abs);
    p.to_string_lossy().replace('\\', "/")
}

/// Scan every media folder recorded in `project.json`. Recorded folders that share a
/// parent inside the project (`A-roll/facecam`, `A-roll/voiceover`) are shown under that
/// parent (`A-roll`), and each keeps its own role.
pub fn scan(project: &Project) -> Vec<FolderNode> {
    let mut tops: Vec<(String, MediaRole)> = Vec::new();
    for f in &project.file.media_folders {
        let top = if Path::new(&f.path).is_absolute() {
            f.path.clone()
        } else {
            f.path.split('/').next().unwrap_or(&f.path).to_owned()
        };
        if !tops.iter().any(|(t, _)| *t == top) {
            tops.push((top, f.role));
        }
    }
    // A recorded outside subfolder keeps its role without appearing as a second tree.
    let roots = tops.clone();
    tops.retain(|(path, _)| {
        !roots
            .iter()
            .any(|(other, _)| other != path && Path::new(path).starts_with(other))
    });
    tops.into_iter()
        .map(|(top, role)| {
            let abs = project.resolve(&top);
            scan_dir(project, &abs, role, &project.file.media_folders, 0)
        })
        .collect()
}

fn scan_dir(
    project: &Project,
    dir: &Path,
    role: MediaRole,
    recorded: &[MediaFolder],
    depth: usize,
) -> FolderNode {
    let path = display_path(project, dir);
    // A recorded subfolder keeps its own role.
    let role = recorded
        .iter()
        .find(|f| f.path == path)
        .map_or(role, |f| f.role);
    let mut node = FolderNode {
        name: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone()),
        path,
        role,
        exists: dir.is_dir(),
        children: vec![],
        files: vec![],
    };
    let Ok(read) = fs::read_dir(dir) else {
        return node;
    };
    let mut entries: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if depth < MAX_DEPTH {
                node.children
                    .push(scan_dir(project, &p, role, recorded, depth + 1));
            }
        } else if let Some(kind) = MediaKind::from_path(&p) {
            node.files.push(MediaFile {
                path: display_path(project, &p),
                name: name.to_owned(),
                kind,
                role,
                bytes: p.metadata().map(|m| m.len()).unwrap_or(0),
            });
        }
    }
    node
}

impl Project {
    /// Record a folder (inside the project or anywhere else) with its role, create it if
    /// it's inside the project and missing, and save `project.json`.
    pub fn add_media_folder(&mut self, abs: &Path, role: MediaRole) -> Result<()> {
        let path = display_path(self, abs);
        if self.file.media_folders.iter().any(|f| f.path == path) {
            return Err(CoreError::Invalid(format!("{path} is already linked.")));
        }
        if abs.starts_with(&self.root) && !abs.exists() {
            fs::create_dir_all(abs).map_err(|e| CoreError::io(abs, e))?;
        }
        self.file.media_folders.push(MediaFolder::new(path, role));
        self.save()
    }

    pub fn remove_media_folder(&mut self, path: &str) -> Result<()> {
        self.file.media_folders.retain(|f| f.path != path);
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Format, NewProject, NewVision, ScriptStart, Timebase, create_project};
    use crate::testutil::temp_dir;

    fn project() -> Project {
        let root = temp_dir("media").join("P");
        create_project(&NewProject {
            name: "P".into(),
            series: None,
            root,
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
    fn scans_folders_roles_and_files() {
        let p = project();
        let beam = p.root.join("B-roll/BeamNG");
        fs::create_dir_all(&beam).unwrap();
        fs::write(beam.join("barrier-hit.mp4"), b"x").unwrap();
        fs::write(beam.join("notes.txt"), b"x").unwrap();
        fs::write(p.root.join("A-roll/voiceover/take3.wav"), b"x").unwrap();

        let tree = scan(&p);
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["A-roll", "B-roll", "Screen", "Graphics"]);

        let aroll = &tree[0];
        assert_eq!(aroll.children.len(), 2);
        let vo = aroll.find("A-roll/voiceover").unwrap();
        assert_eq!(vo.role, MediaRole::Voiceover);
        assert_eq!(vo.files[0].kind, MediaKind::Audio);

        let broll = &tree[1];
        let beam_node = broll.find("B-roll/BeamNG").unwrap();
        assert_eq!(beam_node.role, MediaRole::BRoll, "subfolder inherits role");
        assert_eq!(beam_node.files.len(), 1, "non-media files are skipped");
        assert_eq!(beam_node.files[0].path, "B-roll/BeamNG/barrier-hit.mp4");
        assert_eq!(broll.all_files().len(), 1);
    }

    #[test]
    fn links_an_outside_folder() {
        let mut p = project();
        let outside = temp_dir("outside");
        fs::write(outside.join("clip.mov"), b"x").unwrap();
        p.add_media_folder(&outside, MediaRole::BRoll).unwrap();
        assert!(p.add_media_folder(&outside, MediaRole::BRoll).is_err());

        let reopened = Project::open(&p.root).unwrap();
        assert_eq!(reopened.file.media_folders.len(), 6);
        let tree = scan(&reopened);
        let node = tree.last().unwrap();
        assert!(Path::new(&node.path).is_absolute());
        assert_eq!(node.files[0].name, "clip.mov");
    }
}
