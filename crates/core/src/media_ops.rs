//! Filesystem operations behind the Media sidebar. No operation overwrites media.
use crate::error::{CoreError, Result};
use crate::links::Links;
use crate::media::{self, MediaKind};
use crate::project::{MediaFolder, MediaRole, PROJECT_FILE, Project, ensure_loadable};
use std::fs;
use std::path::{Path, PathBuf};

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::Invalid(message.into())
}

fn canonical(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).map_err(|e| CoreError::io(path, e))
}

/// Only the project root (for folder organization) or a folder in its media tree.
fn folder(project: &Project, path: &str, allow_root: bool) -> Result<PathBuf> {
    let abs = project.resolve(path);
    if !(allow_root && canonical(&abs)? == canonical(&project.root)?)
        && !media::scan(project)
            .iter()
            .any(|n| n.find(path).is_some_and(|n| n.exists))
    {
        return Err(invalid("Choose an existing folder in Project media."));
    }
    Ok(abs)
}

fn valid_name(name: &str) -> Result<&str> {
    let name = name.trim();
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.is_empty()
        || name.len() > 200
        || name.starts_with('.')
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (1..=9).any(|i| stem == format!("COM{i}") || stem == format!("LPT{i}"))
    {
        return Err(invalid(
            "Use a folder name without path separators or reserved characters.",
        ));
    }
    Ok(name)
}

fn check_depth(project: &Project, parent: &Path, height: usize) -> Result<()> {
    let path = media::display_path(project, parent);
    fn depth(nodes: &[media::FolderNode], path: &str, level: usize) -> Option<usize> {
        nodes.iter().find_map(|n| {
            if n.path == path {
                Some(level)
            } else {
                depth(&n.children, path, level + 1)
            }
        })
    }
    let levels = depth(&media::scan(project), &path, 0).map_or(height, |level| {
        level.saturating_add(1).saturating_add(height)
    });
    if levels > media::MAX_DEPTH {
        return Err(invalid(
            "This would exceed the media tree's six nested folder levels. Choose a folder higher in the tree.",
        ));
    }
    Ok(())
}

fn height(node: &media::FolderNode) -> usize {
    node.children
        .iter()
        .map(|n| 1 + height(n))
        .max()
        .unwrap_or(0)
}

impl Project {
    /// Make a real folder, registering its role so it remains visible after reopening.
    pub fn create_media_folder(
        &mut self,
        parent: &str,
        name: &str,
        role: MediaRole,
    ) -> Result<String> {
        let parent = folder(self, parent, true)?;
        check_depth(self, &parent, 0)?;
        let dir = parent.join(valid_name(name)?);
        ensure_loadable::<serde_json::Value>(&self.root.join(PROJECT_FILE))?;
        fs::create_dir(&dir).map_err(|e| CoreError::io(&dir, e))?;
        let path = media::display_path(self, &dir);
        let mut updated = self.clone();
        updated
            .file
            .media_folders
            .push(MediaFolder::new(&path, role));
        if let Err(e) = updated.save() {
            return match fs::remove_dir(&dir) {
                Ok(()) => Err(e),
                Err(cleanup) => Err(invalid(format!(
                    "{e}. Could not remove the new empty folder {}: {cleanup}",
                    dir.display()
                ))),
            };
        }
        *self = updated;
        Ok(path)
    }

    /// Copy Explorer files; keep the originals and roll back this batch on error.
    /// Run on a worker: large video copies must not block the UI.
    pub fn import_media_files(
        &self,
        destination: &str,
        sources: &[PathBuf],
    ) -> Result<Vec<String>> {
        if sources.is_empty() || sources.len() > 1000 {
            return Err(invalid("Drop between 1 and 1,000 media files at a time."));
        }
        let dir = folder(self, destination, false)?;
        let mut targets = Vec::new();
        let mut names = std::collections::HashSet::new();
        for src in sources {
            if !src.is_file() || MediaKind::from_path(src).is_none() {
                return Err(invalid(format!(
                    "{} is not a supported media file. Drop video, audio or image files; use Link folder for an existing folder.",
                    src.display()
                )));
            }
            let name = src
                .file_name()
                .ok_or_else(|| invalid("The dropped file has no name."))?;
            let dest = dir.join(name);
            if dest.exists() || !names.insert(name.to_string_lossy().to_lowercase()) {
                return Err(invalid(format!(
                    "{} already exists or appears twice in this drop. Nothing was copied. Rename the file or choose another folder.",
                    dest.display()
                )));
            }
            targets.push(dest);
        }
        let mut created = Vec::new();
        for (src, dest) in sources.iter().zip(&targets) {
            let result = (|| {
                let mut input = fs::File::open(src).map_err(|e| CoreError::io(src, e))?;
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(dest)
                    .map_err(|e| CoreError::io(dest, e))?;
                created.push(dest.clone());
                std::io::copy(&mut input, &mut output).map_err(|e| CoreError::io(dest, e))?;
                output.sync_all().map_err(|e| CoreError::io(dest, e))
            })();
            if let Err(e) = result {
                let mut leftovers = Vec::new();
                for path in &created {
                    if let Err(cleanup) = fs::remove_file(path) {
                        leftovers.push(format!("{}: {cleanup}", path.display()));
                    }
                }
                return Err(invalid(format!(
                    "Copy failed: {e}. Originals are unchanged.{}",
                    if leftovers.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " Incomplete copies could not be removed: {}",
                            leftovers.join("; ")
                        )
                    }
                )));
            }
        }
        Ok(targets
            .iter()
            .map(|p| media::display_path(self, p))
            .collect())
    }

    /// Move a media file/folder and remap every script link and recorded subfolder.
    /// Save failures roll back the rename and project metadata before returning.
    pub fn move_media(&mut self, source: &str, parent: &str, links: &mut Links) -> Result<String> {
        let tree = media::scan(self);
        let node = tree.iter().find_map(|n| n.find(source));
        let role = node
            .map(|n| n.role)
            .or_else(|| {
                tree.iter()
                    .flat_map(|n| n.all_files())
                    .find(|f| f.path == source)
                    .map(|f| f.role)
            })
            .ok_or_else(|| {
                invalid("The file or folder is no longer in Project media. Rescan and try again.")
            })?;
        let src = self.resolve(source);
        let parent_dir = folder(self, parent, node.is_some())?;
        if let Some(node) = node {
            check_depth(self, &parent_dir, height(node))?;
        }
        let dest = parent_dir.join(
            src.file_name()
                .ok_or_else(|| invalid("This folder cannot be moved."))?,
        );
        let src_real = canonical(&src)?;
        let parent_real = canonical(&parent_dir)?;
        let root_real = canonical(&self.root)?;
        if root_real.starts_with(&src_real) || parent_real.starts_with(&src_real) {
            return Err(invalid(
                "A folder cannot be moved into itself or its descendants, and the project folder cannot be moved.",
            ));
        }
        let protected = self
            .tabs()
            .into_iter()
            .map(|tab| self.resolve(&tab.rel))
            .chain([
                self.vision_path(),
                self.root.join(PROJECT_FILE),
                self.root.join(crate::links::LINKS_FILE),
                self.root.join(crate::channels::PROJECT_BRIEF),
                self.root.join(crate::visuals::FILE),
                self.root.join("sync/approved-script.md"),
                self.root.join("sync/approved-channel.md"),
            ]);
        if protected.into_iter().any(|p| {
            p.starts_with(&src) || canonical(&p).ok().is_some_and(|p| p.starts_with(&src_real))
        }) {
            return Err(invalid(
                "This folder contains project documents and cannot be moved as media.",
            ));
        }
        if src.parent().map(canonical).transpose()?.as_ref() == Some(&parent_real) {
            return Ok(media::display_path(self, &src));
        }
        if dest.exists() {
            return Err(invalid(format!(
                "{} already exists. Nothing was moved.",
                dest.display()
            )));
        }
        if !links.unreadable.is_empty() {
            return Err(invalid(
                "This project contains older links that cannot be read. Moving media could break them; resolve those links first.",
            ));
        }
        ensure_loadable::<serde_json::Value>(&self.root.join(PROJECT_FILE))?;
        ensure_loadable::<serde_json::Value>(&self.root.join(crate::links::LINKS_FILE))?;
        let remap = |path: &str| {
            let original = self.resolve(path);
            let tail = original
                .strip_prefix(&src)
                .ok()
                .map(Path::to_path_buf)
                .or_else(|| {
                    canonical(&original)
                        .ok()?
                        .strip_prefix(&src_real)
                        .ok()
                        .map(Path::to_path_buf)
                });
            tail.map(|tail| media::display_path(self, &dest.join(tail)))
        };
        let mut updated = self.clone();
        for f in &mut updated.file.media_folders {
            if let Some(path) = remap(&f.path) {
                f.path = path;
            }
        }
        let moved = media::display_path(self, &dest);
        if node.is_some() && !updated.file.media_folders.iter().any(|f| f.path == moved) {
            updated
                .file
                .media_folders
                .push(MediaFolder::new(&moved, role));
        }
        let mut updated_links = links.clone();
        for link in &mut updated_links.links {
            if let Some(path) = remap(&link.media.path) {
                link.media.path = path;
            }
        }
        fs::rename(&src, &dest).map_err(|e| invalid(format!("Could not move {} to {}: {e}. Choose folders on the same drive and close files in use.", src.display(), dest.display())))?;
        let save = updated.save().and_then(|()| updated_links.save(&updated));
        if let Err(e) = save {
            let mut failures = Vec::new();
            if let Err(rollback) = fs::rename(&dest, &src) {
                failures.push(format!(
                    "restore {} from {}: {rollback}",
                    src.display(),
                    dest.display()
                ));
            }
            if let Err(rollback) = self.save() {
                failures.push(format!("restore project.json: {rollback}"));
            }
            return Err(invalid(format!(
                "Could not save the move: {e}.{}",
                if failures.is_empty() {
                    " The move was undone.".into()
                } else {
                    format!(" Recovery needed: {}", failures.join("; "))
                }
            )));
        }
        *self = updated;
        *links = updated_links;
        Ok(moved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::{Link, LinkKind, LinkMedia};

    fn project() -> Project {
        let root = crate::testutil::temp_dir("media-ops");
        let p = Project {
            root,
            file: serde_json::from_value(serde_json::json!({
                "name": "Media test", "spec_version": 1, "adapter": "filmcraft",
                "media_folders": [{"path":"B-roll","role":"b-roll"},{"path":"Graphics","role":"graphics"}]
            })).unwrap(),
        };
        for name in ["B-roll", "Graphics"] {
            fs::create_dir(p.root.join(name)).unwrap();
        }
        p.save().unwrap();
        Links::default().save(&p).unwrap();
        p
    }

    fn linked(p: &Project, path: &str) -> Links {
        let mut links = Links::default();
        links.add(Link {
            id: String::new(),
            quote: "hello".into(),
            line: 0,
            media: LinkMedia {
                path: path.into(),
                hash: None,
            },
            role: MediaRole::Voiceover,
            kind: LinkKind::Audio,
            in_point: Some("2".into()),
            out_point: Some("4".into()),
        });
        links.save(p).unwrap();
        links
    }

    #[test]
    fn imports_multiple_files_without_touching_originals_or_overwriting() {
        let p = project();
        let outside = crate::testutil::temp_dir("media-import");
        let sources: Vec<_> = ["clip.mp4", "sound.wav"]
            .into_iter()
            .map(|name| {
                let path = outside.join(name);
                fs::write(&path, name).unwrap();
                path
            })
            .collect();
        assert_eq!(
            p.import_media_files("B-roll", &sources).unwrap(),
            ["B-roll/clip.mp4", "B-roll/sound.wav"]
        );
        for src in &sources {
            assert_eq!(
                fs::read(src).unwrap(),
                fs::read(p.root.join("B-roll").join(src.file_name().unwrap())).unwrap()
            );
        }
        fs::write(&sources[0], b"different").unwrap();
        assert!(p.import_media_files("B-roll", &sources).is_err());
        assert_eq!(
            fs::read(p.root.join("B-roll/clip.mp4")).unwrap(),
            b"clip.mp4"
        );
    }

    #[test]
    fn bad_import_batches_copy_nothing() {
        let p = project();
        let outside = crate::testutil::temp_dir("bad-import");
        let clip = outside.join("a.mp4");
        fs::write(&clip, b"video").unwrap();
        let text = outside.join("notes.txt");
        fs::write(&text, b"notes").unwrap();
        for sources in [
            vec![clip.clone(), text],
            vec![clip.clone(), clip.clone()],
            vec![outside],
            vec![],
        ] {
            assert!(p.import_media_files("B-roll", &sources).is_err());
        }
        assert!(
            p.import_media_files(".", std::slice::from_ref(&clip))
                .is_err()
        );
        assert!(p.import_media_files("../outside", &[clip]).is_err());
        assert_eq!(fs::read_dir(p.root.join("B-roll")).unwrap().count(), 0);
    }

    #[test]
    fn creates_folders_and_preserves_roles_and_links_when_reparented() {
        let mut p = project();
        p.create_media_folder("B-roll", "Scene", MediaRole::BRoll)
            .unwrap();
        p.create_media_folder("B-roll/Scene", "VO", MediaRole::Voiceover)
            .unwrap();
        fs::write(p.root.join("B-roll/Scene/VO/take.wav"), b"audio").unwrap();
        let mut links = linked(&p, "B-roll/Scene/VO/take.wav");
        let old = links.links[0].clone();
        assert_eq!(
            p.move_media("B-roll/Scene", "Graphics", &mut links)
                .unwrap(),
            "Graphics/Scene"
        );
        assert!(!p.root.join("B-roll/Scene").exists());
        assert_eq!(
            fs::read(p.root.join("Graphics/Scene/VO/take.wav")).unwrap(),
            b"audio"
        );
        assert_eq!(links.links[0].media.path, "Graphics/Scene/VO/take.wav");
        assert_eq!(links.links[0].quote, old.quote);
        assert_eq!(links.links[0].in_point, old.in_point);
        assert_eq!(links.links[0].kind, old.kind);
        let reopened = Project::open(&p.root).unwrap();
        assert_eq!(Links::load(&reopened).unwrap(), links);
        let tree = media::scan(&reopened);
        assert_eq!(
            tree.iter()
                .find_map(|n| n.find("Graphics/Scene/VO"))
                .unwrap()
                .role,
            MediaRole::Voiceover
        );
        assert_eq!(
            p.move_media("Graphics/Scene", ".", &mut links).unwrap(),
            "Scene"
        );
        assert_eq!(links.links[0].media.path, "Scene/VO/take.wav");
    }

    #[test]
    fn moves_files_and_rejects_cycles_collisions_and_hidden_depth() {
        let mut p = project();
        p.create_media_folder("B-roll", "Scene", MediaRole::BRoll)
            .unwrap();
        fs::write(p.root.join("B-roll/clip.mp4"), b"clip").unwrap();
        let mut links = linked(&p, "B-roll/clip.mp4");
        assert!(p.move_media("B-roll", "B-roll/Scene", &mut links).is_err());
        assert!(p.move_media("B-roll", "B-roll", &mut links).is_err());
        fs::write(p.root.join("Graphics/clip.mp4"), b"existing").unwrap();
        assert!(
            p.move_media("B-roll/clip.mp4", "Graphics", &mut links)
                .is_err()
        );
        assert_eq!(
            fs::read(p.root.join("Graphics/clip.mp4")).unwrap(),
            b"existing"
        );
        p.move_media("B-roll/clip.mp4", "B-roll/Scene", &mut links)
            .unwrap();
        assert_eq!(links.links[0].media.path, "B-roll/Scene/clip.mp4");
        let mut parent = "Graphics".to_owned();
        for _ in 0..media::MAX_DEPTH {
            parent = p
                .create_media_folder(&parent, "child", MediaRole::Graphics)
                .unwrap();
        }
        assert!(
            p.create_media_folder(&parent, "too-deep", MediaRole::Graphics)
                .is_err()
        );
        assert!(p.move_media("B-roll/Scene", &parent, &mut links).is_err());
    }

    #[test]
    fn save_failure_rolls_back_folder_move_and_creation() {
        let mut p = project();
        p.create_media_folder("B-roll", "Scene", MediaRole::BRoll)
            .unwrap();
        fs::write(p.root.join("B-roll/Scene/a.mp4"), b"video").unwrap();
        let mut links = linked(&p, "B-roll/Scene/a.mp4");
        let old = p.file.clone();
        let old_links = links.clone();
        fs::create_dir(
            p.root
                .join(format!(".links.json.{}.tmp", std::process::id())),
        )
        .unwrap();
        assert!(
            p.move_media("B-roll/Scene", "Graphics", &mut links)
                .is_err()
        );
        assert!(p.root.join("B-roll/Scene/a.mp4").is_file());
        assert!(!p.root.join("Graphics/Scene").exists());
        assert_eq!(p.file, old);
        assert_eq!(Project::open(&p.root).unwrap().file, old);
        assert_eq!(Links::load(&p).unwrap(), old_links);
        fs::create_dir(
            p.root
                .join(format!(".project.json.{}.tmp", std::process::id())),
        )
        .unwrap();
        assert!(
            p.create_media_folder("Graphics", "new", MediaRole::Graphics)
                .is_err()
        );
        assert!(!p.root.join("Graphics/new").exists());
        assert_eq!(p.file, old);
    }

    #[test]
    fn hostile_names_and_unsupported_links_leave_data_alone() {
        let mut p = project();
        for name in [
            "",
            ".",
            "..",
            "../bad",
            "a/b",
            "a\\b",
            "CON",
            "NUL.mp4",
            "LPT1",
            "trailing.",
            "line\nbreak",
            "a:b",
        ] {
            assert!(
                p.create_media_folder("B-roll", name, MediaRole::BRoll)
                    .is_err(),
                "{name}"
            );
        }
        let mut links = Links {
            unreadable: vec![serde_json::json!({"old": true})],
            ..Default::default()
        };
        assert!(p.move_media("B-roll", "Graphics", &mut links).is_err());
        fs::write(p.root.join("links.json"), b"broken").unwrap();
        assert!(
            p.move_media("B-roll", "Graphics", &mut Links::default())
                .is_err()
        );
        assert!(p.root.join("B-roll").is_dir());
    }

    #[test]
    fn outside_folders_collapse_to_one_tree_after_reparenting() {
        let mut p = project();
        let outside = crate::testutil::temp_dir("media-external");
        p.add_media_folder(&outside, MediaRole::Other).unwrap();
        let outside_path = media::display_path(&p, &outside);
        let nested = p
            .create_media_folder(&outside_path, "Nested", MediaRole::Voiceover)
            .unwrap();
        fs::write(p.resolve(&nested).join("take.wav"), b"sound").unwrap();
        let mut links = linked(&p, &format!("{nested}/take.wav"));
        let dest = p.move_media("B-roll", &outside_path, &mut links).unwrap();
        p.move_media(&nested, &dest, &mut links).unwrap();
        assert_eq!(
            media::scan(&p)
                .iter()
                .filter(|n| Path::new(&n.path).is_absolute())
                .count(),
            1
        );
        assert_eq!(links.links[0].media.path, format!("{dest}/Nested/take.wav"));
    }
    #[test]
    fn project_documents_stay_put_and_absolute_links_follow_media() {
        let mut p = project();
        fs::create_dir_all(p.root.join("script")).unwrap();
        fs::write(p.script_path(), b"my script").unwrap();
        p.add_media_folder(&p.root.join("script"), MediaRole::Other)
            .unwrap();
        assert!(
            p.move_media("script", "B-roll", &mut Links::default())
                .is_err()
        );
        assert_eq!(fs::read(p.script_path()).unwrap(), b"my script");
        fs::create_dir_all(p.root.join("sync")).unwrap();
        fs::write(p.root.join("sync/approved-script.md"), b"approved").unwrap();
        p.add_media_folder(&p.root.join("sync"), MediaRole::Other)
            .unwrap();
        assert!(
            p.move_media("sync", "B-roll", &mut Links::default())
                .is_err()
        );
        assert_eq!(
            fs::read(p.root.join("sync/approved-script.md")).unwrap(),
            b"approved"
        );
        fs::write(p.root.join("B-roll/a.mp4"), b"video").unwrap();
        let absolute = p
            .root
            .join("B-roll/a.mp4")
            .to_string_lossy()
            .replace('\\', "/");
        let mut links = linked(&p, &absolute);
        p.move_media("B-roll/a.mp4", "Graphics", &mut links)
            .unwrap();
        assert_eq!(links.links[0].media.path, "Graphics/a.mp4");
    }
}
