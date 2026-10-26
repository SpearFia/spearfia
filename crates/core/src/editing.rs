//! Project-bound agent edits. Revision tokens guard exact saved bytes, not timestamps.
use crate::{
    CoreError,
    error::Result,
    project::Project,
    visuals::{self, Anchor, Metadata, Plan, Reference},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

pub const DOCUMENT_SCHEMA: &str = include_str!("../../../schemas/document-edit.schema.json");
pub const VISUAL_SCHEMA: &str = include_str!("../../../schemas/visual-edit.schema.json");
const MAX_HISTORY_BYTES: usize = 32 * 1024 * 1024;
fn invalid(text: impl Into<String>) -> CoreError {
    CoreError::Invalid(text.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Document {
    Script,
    Visuals,
}
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub document: Document,
    pub revision: String,
    pub source: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CuePatch {
    pub title: Option<String>,
    pub body: Option<String>,
    pub visual_type: Option<String>,
    pub method: Option<String>,
    pub asset_state: Option<String>,
    pub review: Option<String>,
    pub timing: Option<String>,
    pub details: Option<BTreeMap<String, String>>,
    pub references: Option<Vec<Reference>>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum VisualEdit {
    Create {
        title: String,
        start: usize,
        end: usize,
    },
    Update {
        id: String,
        patch: CuePatch,
    },
    Delete {
        id: String,
    },
    Split {
        id: String,
        at: usize,
    },
    Merge {
        id: String,
    },
    Reanchor {
        id: String,
        start: usize,
        end: usize,
    },
    BindReference {
        id: String,
        reference: Reference,
    },
    AddShot {
        id: String,
    },
    MoveShot {
        id: String,
        from: usize,
        to: usize,
    },
}
#[derive(Debug, Clone)]
struct Change {
    document: Document,
    before: String,
    after: String,
}
/// History lives for this server process. It never overwrites intervening external edits.
pub struct Editor {
    pub project: Project,
    snapshots: Vec<Snapshot>,
    undo: Vec<Change>,
    redo: Vec<Change>,
}
impl Editor {
    pub fn open(root: &Path) -> Result<Self> {
        let root = root.canonicalize().map_err(|e| CoreError::io(root, e))?;
        let project_path = root.join(crate::project::PROJECT_FILE);
        let real_project = project_path
            .canonicalize()
            .map_err(|e| CoreError::io(&project_path, e))?;
        if !real_project.starts_with(&root) {
            return Err(invalid(
                "Project configuration resolves outside the bound project.",
            ));
        }
        let source = crate::channels::read_limited(&project_path, visuals::MAX_BYTES)?;
        let file = serde_json::from_str(&source).map_err(|e| CoreError::json(&project_path, e))?;
        let project = Project { root, file };
        let editor = Self {
            project,
            snapshots: vec![],
            undo: vec![],
            redo: vec![],
        };
        editor.checked_path(Document::Script)?;
        editor.checked_path(Document::Visuals)?;
        editor.inside(&editor.project.file.vision)?;
        Ok(editor)
    }
    fn inside(&self, relative: &str) -> Result<PathBuf> {
        let normalized = relative.replace('\\', "/");
        if normalized.contains(':') {
            return Err(invalid(
                "Agent document paths cannot contain drive prefixes.",
            ));
        }
        let relative = Path::new(&normalized);
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(invalid(
                "Agent documents must use ordinary project-relative paths without .. or absolute prefixes.",
            ));
        }
        let path = self.project.root.join(relative);
        let mut ancestor = path.as_path();
        loop {
            match ancestor.canonicalize() {
                Ok(real) => {
                    if !real.starts_with(&self.project.root) {
                        return Err(invalid(
                            "Agent document resolves outside the bound project.",
                        ));
                    }
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    ancestor = ancestor
                        .parent()
                        .ok_or_else(|| invalid("Invalid project document path."))?;
                }
                Err(e) => return Err(CoreError::io(ancestor, e)),
            }
        }
        Ok(path)
    }
    pub fn checked_path(&self, document: Document) -> Result<PathBuf> {
        let relative = match document {
            Document::Script => &self.project.file.script,
            Document::Visuals => visuals::FILE,
        };
        if document == Document::Script {
            let normalized = relative.replace('\\', "/").to_ascii_lowercase();
            if normalized == visuals::FILE
                || normalized.starts_with("sync/")
                || ["project.json", "channel.md", "comments.json", "vision.yaml"]
                    .contains(&normalized.as_str())
                || !normalized.ends_with(".md")
            {
                return Err(invalid(
                    "The configured script must be a Markdown document separate from project, visual and approval records.",
                ));
            }
        }
        let path = self.inside(relative)?;
        let real = resolved_destination(&path)?;
        let protected = [
            "project.json",
            "channel.md",
            "comments.json",
            "sync/approved-script.md",
            "sync/approved-channel.md",
        ];
        for relative in protected {
            if real == resolved_destination(&self.project.root.join(relative))? {
                return Err(invalid(
                    "Agent edits cannot target project metadata or approval records, including through aliases.",
                ));
            }
        }
        let other = if document == Document::Script {
            self.project.root.join(visuals::FILE)
        } else {
            self.project.script_path()
        };
        if real == resolved_destination(&other)?
            || real == resolved_destination(&self.project.vision_path())?
        {
            return Err(invalid(
                "Script, Visual Script and Vision must resolve to separate files.",
            ));
        }
        Ok(path)
    }
    pub fn validate_paths(&self) -> Result<()> {
        let path = self.inside(crate::project::PROJECT_FILE)?;
        let source = crate::channels::read_limited(&path, visuals::MAX_BYTES)?;
        let current: crate::project::ProjectFile =
            serde_json::from_str(&source).map_err(|e| CoreError::json(&path, e))?;
        if current != self.project.file {
            return Err(invalid(
                "Project configuration changed. Restart this server before editing the project again.",
            ));
        }
        self.checked_path(Document::Script)?;
        self.checked_path(Document::Visuals)?;
        self.inside(&self.project.file.vision)?;
        Ok(())
    }
    fn disk(&self, document: Document) -> Result<String> {
        self.validate_paths()?;
        let path = self.checked_path(document)?;
        if document == Document::Visuals {
            self.project.read_visuals()
        } else {
            crate::channels::read_limited(&path, visuals::MAX_BYTES)
        }
    }
    pub fn read(&mut self, document: Document) -> Result<Snapshot> {
        let source = self.disk(document)?;
        if let Some(snapshot) = self
            .snapshots
            .iter()
            .find(|s| s.document == document && s.source == source)
        {
            return Ok(snapshot.clone());
        }
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|e| invalid(format!("Cannot create an edit revision: {e}")))?;
        let revision = random.iter().map(|b| format!("{b:02x}")).collect();
        let snapshot = Snapshot {
            document,
            revision,
            source,
        };
        self.snapshots.retain(|s| s.document != document);
        self.snapshots.push(snapshot.clone());
        Ok(snapshot)
    }
    fn base(&self, document: Document, expected: &str) -> Result<String> {
        let snapshot = self
            .snapshots
            .iter()
            .find(|s| s.document == document && s.revision == expected)
            .ok_or_else(|| {
                invalid(
                    "Unknown/stale revision. Read this document from this server before editing.",
                )
            })?;
        let current = self.disk(document)?;
        if current != snapshot.source {
            return Err(invalid(
                "The document changed since it was read. Read it again and reconcile the edit.",
            ));
        }
        Ok(current)
    }
    fn save(&mut self, document: Document, before: &str, after: &str) -> Result<Snapshot> {
        if after.len() > visuals::MAX_BYTES {
            return Err(invalid("Agent documents are limited to 1 MiB."));
        }
        self.checked_path(document)?;
        if self.disk(document)? != before {
            return Err(invalid(
                "The document changed during the edit. Read it again.",
            ));
        }
        if document == Document::Visuals {
            self.project.write_visuals(before, after)?;
        } else {
            self.project
                .write_doc_expected(&self.project.file.script, before, after)?;
        }
        self.read(document)
    }
    fn commit(&mut self, document: Document, before: String, after: String) -> Result<Snapshot> {
        let snapshot = self.save(document, &before, &after)?;
        if before != after {
            self.undo.push(Change {
                document,
                before,
                after,
            });
            self.redo.clear();
            while self.undo.len() > 32
                || self
                    .undo
                    .iter()
                    .map(|c| c.before.len().saturating_add(c.after.len()))
                    .sum::<usize>()
                    > MAX_HISTORY_BYTES
            {
                self.undo.remove(0);
            }
        }
        Ok(snapshot)
    }
    pub fn edit_script(&mut self, revision: &str, edit: TextEdit) -> Result<Snapshot> {
        self.edit_document(Document::Script, revision, edit)
    }
    pub fn edit_document(
        &mut self,
        document: Document,
        revision: &str,
        edit: TextEdit,
    ) -> Result<Snapshot> {
        let source = self.base(document, revision)?;
        let out = visuals::replace(&source, edit.start..edit.end, &edit.text)?;
        if document == Document::Visuals {
            Plan::parse(&out)?;
        }
        self.commit(document, source, out)
    }
    pub fn edit_visuals(
        &mut self,
        revision: &str,
        script_revision: &str,
        edit: VisualEdit,
    ) -> Result<(Snapshot, Option<String>)> {
        let source = self.base(Document::Visuals, revision)?;
        let script = self.base(Document::Script, script_revision)?;
        let plan = Plan::parse(&source)?;
        let (out, id) = apply_visual(&plan, &script, edit)?;
        Plan::parse(&out)?;
        if self.disk(Document::Script)? != script {
            return Err(invalid(
                "Narration changed during the visual edit. Read both documents again.",
            ));
        }
        let snapshot = self.commit(Document::Visuals, source, out)?;
        Ok((snapshot, id))
    }
    pub fn history_document(&self, redo: bool) -> Option<Document> {
        (if redo { &self.redo } else { &self.undo })
            .last()
            .map(|c| c.document)
    }
    pub fn history(&mut self, revision: &str, redo: bool) -> Result<Snapshot> {
        let change = (if redo { &self.redo } else { &self.undo })
            .last()
            .cloned()
            .ok_or_else(|| invalid("There is no edit to undo/redo in this server session."))?;
        let current = self.base(change.document, revision)?;
        let expected = if redo { &change.before } else { &change.after };
        if &current != expected {
            return Err(invalid(
                "A later external edit prevents this undo/redo. Reconcile the current document first.",
            ));
        }
        let target = if redo { &change.after } else { &change.before };
        let snapshot = self.save(change.document, &current, target)?;
        if redo {
            self.redo.pop();
            self.undo.push(change);
        } else {
            self.undo.pop();
            self.redo.push(change);
        }
        Ok(snapshot)
    }
}
fn resolved_destination(path: &Path) -> Result<PathBuf> {
    let mut ancestor = path;
    let mut missing = Vec::new();
    loop {
        match ancestor.canonicalize() {
            Ok(mut real) => {
                for part in missing.iter().rev() {
                    real.push(part);
                }
                return Ok(real);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or_else(|| invalid("Invalid document destination."))?
                        .to_owned(),
                );
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| invalid("Invalid document destination."))?;
            }
            Err(e) => return Err(CoreError::io(ancestor, e)),
        }
    }
}
fn title(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 200 || value.contains(['\n', '\r']) {
        return Err(invalid(
            "Use a nonempty, single-line cue title of up to 200 characters.",
        ));
    }
    Ok(())
}
fn reference_limits(reference: &Reference) -> Result<()> {
    if [
        &reference.role,
        &reference.path,
        &reference.revision,
        &reference.scope,
    ]
    .iter()
    .any(|s| s.len() > 8192)
    {
        return Err(invalid("Reference fields are limited to 8192 bytes."));
    }
    Ok(())
}
fn patch_meta(meta: &mut Metadata, patch: &CuePatch) -> Result<()> {
    for (value, target) in [
        (&patch.visual_type, &mut meta.visual_type),
        (&patch.method, &mut meta.method),
        (&patch.asset_state, &mut meta.asset_state),
        (&patch.review, &mut meta.review),
        (&patch.timing, &mut meta.timing),
    ] {
        if let Some(value) = value {
            if value.len() > 8192 {
                return Err(invalid("Cue fields are limited to 8192 bytes."));
            }
            *target = value.clone();
        }
    }
    if patch.review.is_none() {
        meta.review = "draft".into();
    }
    if let Some(details) = &patch.details {
        if details.len() > 128
            || details
                .iter()
                .any(|(k, v)| k.len() > 8192 || v.len() > 8192)
        {
            return Err(invalid(
                "Use at most 128 detail fields of up to 8192 bytes.",
            ));
        }
        meta.details.extend(details.clone());
    }
    if let Some(references) = &patch.references {
        for reference in references {
            reference_limits(reference)?;
        }
        meta.references = references.clone();
    }
    Ok(())
}
fn apply_visual(plan: &Plan, script: &str, edit: VisualEdit) -> Result<(String, Option<String>)> {
    let same = |source: String, id: String| Ok((source, Some(id)));
    match edit {
        VisualEdit::Create {
            title: name,
            start,
            end,
        } => {
            title(&name)?;
            let (out, id) = plan.add(&name, Anchor::new(script, start..end)?)?;
            same(out, id)
        }
        VisualEdit::Update { id, patch } => {
            let mut meta = plan.cue(&id)?.meta.clone();
            patch_meta(&mut meta, &patch)?;
            let mut source = plan.update(&meta)?;
            if let Some(body) = patch.body {
                let updated = Plan::parse(&source)?;
                let cue = updated.cue(&id)?;
                source = visuals::replace(
                    &source,
                    cue.body.clone(),
                    &format!("\n{}\n\n", body.trim_end()),
                )?;
            }
            if let Some(name) = patch.title {
                title(&name)?;
                let updated = Plan::parse(&source)?;
                let cue = updated.cue(&id)?;
                let length = source
                    .get(cue.span.start..)
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .len();
                source = visuals::replace(
                    &source,
                    cue.span.start..cue.span.start.saturating_add(length),
                    &format!("## {name}"),
                )?;
            }
            let updated = Plan::parse(&source)?;
            if updated.cues.iter().map(|c| &c.meta.id).collect::<Vec<_>>()
                != plan.cues.iter().map(|c| &c.meta.id).collect::<Vec<_>>()
            {
                return Err(invalid(
                    "Cue body edits cannot create/delete cue records. Use the corresponding typed operation.",
                ));
            }
            same(source, id)
        }
        VisualEdit::Delete { id } => {
            let out = visuals::replace(&plan.source, plan.cue(&id)?.span.clone(), "")?;
            Ok((out, None))
        }
        VisualEdit::Split { id, at } => {
            let (out, new_id) = plan.split(&id, at)?;
            same(out, new_id)
        }
        VisualEdit::Merge { id } => same(plan.merge_next(&id, script)?, id),
        VisualEdit::Reanchor { id, start, end } => {
            same(plan.reanchor(&id, Anchor::new(script, start..end)?)?, id)
        }
        VisualEdit::BindReference { id, reference } => {
            reference_limits(&reference)?;
            let mut meta = plan.cue(&id)?.meta.clone();
            meta.references.push(reference);
            meta.review = "draft".into();
            same(plan.update(&meta)?, id)
        }
        VisualEdit::AddShot { id } => same(plan.add_shot(&id)?, id),
        VisualEdit::MoveShot { id, from, to } => same(plan.move_shot(&id, from, to)?, id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn editor() -> Editor {
        let root = crate::testutil::temp_dir("agent-editor");
        std::fs::write(
            root.join("project.json"),
            r#"{"name":"Test","spec_version":1,"adapter":"filmcraft"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("script")).unwrap();
        std::fs::write(
            root.join("script/script.md"),
            "LIVE: Hello éveryone.\nVO: Welcome back.\n",
        )
        .unwrap();
        Editor::open(&root).unwrap()
    }
    #[test]
    fn typed_edits_references_and_history_preserve_ids_and_unknown_prose() {
        let mut e = editor();
        let s = e.read(Document::Script).unwrap();
        let v = e.read(Document::Visuals).unwrap();
        let (v, id) = e
            .edit_visuals(
                &v.revision,
                &s.revision,
                VisualEdit::Create {
                    title: "Opening".into(),
                    start: 0,
                    end: 21,
                },
            )
            .unwrap();
        let id = id.unwrap();
        let (v, _) = e
            .edit_visuals(
                &v.revision,
                &s.revision,
                VisualEdit::Update {
                    id: id.clone(),
                    patch: CuePatch {
                        body: Some("VISUAL: A garden.\n\nUnrecognized prose 🍀".into()),
                        method: Some("Reuse".into()),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        assert!(v.source.contains("Unrecognized prose 🍀"));
        let (v, _) = e
            .edit_visuals(
                &v.revision,
                &s.revision,
                VisualEdit::BindReference {
                    id: id.clone(),
                    reference: Reference {
                        role: "style".into(),
                        path: "art.svg".into(),
                        revision: "v2".into(),
                        scope: "use".into(),
                        extra: BTreeMap::new(),
                    },
                },
            )
            .unwrap();
        assert_eq!(
            Plan::parse(&v.source)
                .unwrap()
                .cue(&id)
                .unwrap()
                .meta
                .references
                .len(),
            1
        );
        let undo = e.history(&v.revision, false).unwrap();
        let redo = e.history(&undo.revision, true).unwrap();
        assert_eq!(redo.source, v.source);
        let (deleted, _) = e
            .edit_visuals(&redo.revision, &s.revision, VisualEdit::Delete { id })
            .unwrap();
        assert!(Plan::parse(&deleted.source).unwrap().cues.is_empty());
        assert_eq!(
            e.history(&deleted.revision, false).unwrap().source,
            v.source
        );
        std::fs::remove_dir_all(e.project.root).unwrap();
    }
    #[test]
    fn stale_revisions_ranges_and_narration_changes_never_overwrite() {
        let mut e = editor();
        let s = e.read(Document::Script).unwrap();
        assert!(
            e.edit_script(
                &s.revision,
                TextEdit {
                    start: 13,
                    end: 14,
                    text: "bad".into()
                }
            )
            .is_err()
        );
        assert!(
            e.edit_script(
                &s.revision,
                TextEdit {
                    start: usize::MAX,
                    end: 0,
                    text: "bad".into()
                }
            )
            .is_err()
        );
        let updated = e
            .edit_script(
                &s.revision,
                TextEdit {
                    start: 0,
                    end: 0,
                    text: "VO: Intro.\n".into(),
                },
            )
            .unwrap();
        assert!(
            e.edit_script(
                &s.revision,
                TextEdit {
                    start: 0,
                    end: 0,
                    text: "stale".into()
                }
            )
            .is_err()
        );
        e.project.write_script("VO: External edit.").unwrap();
        assert!(e.history(&updated.revision, false).is_err());
        assert_eq!(e.project.read_script().unwrap(), "VO: External edit.");
        std::fs::remove_dir_all(e.project.root).unwrap();
    }
    #[test]
    fn aliases_cannot_turn_script_edits_into_approval_edits() {
        let e = editor();
        let root = e.project.root.clone();
        std::fs::create_dir_all(root.join("sync")).unwrap();
        std::fs::write(root.join("sync/approved-script.md"), "approved").unwrap();
        let link = root.join("script/alias.md");
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(root.join("sync/approved-script.md"), &link);
        #[cfg(windows)]
        let linked =
            std::os::windows::fs::symlink_file(root.join("sync/approved-script.md"), &link);
        #[cfg(not(any(unix, windows)))]
        let linked: std::io::Result<()> =
            Err(std::io::Error::from(std::io::ErrorKind::Unsupported));
        if linked.is_ok() {
            let mut file = e.project.file.clone();
            file.script = "script/alias.md".into();
            crate::project::write_json(&root.join("project.json"), &file).unwrap();
            assert!(Editor::open(&root).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn simultaneous_sessions_cannot_both_commit_the_same_base_revision() {
        let mut a = editor();
        let root = a.project.root.clone();
        let mut b = Editor::open(&root).unwrap();
        let ar = a.read(Document::Script).unwrap().revision;
        let br = b.read(Document::Script).unwrap().revision;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let second = barrier.clone();
        let one = std::thread::spawn(move || {
            barrier.wait();
            a.edit_script(
                &ar,
                TextEdit {
                    start: 0,
                    end: 0,
                    text: "VO: First.\n".into(),
                },
            )
            .is_ok()
        });
        let two = std::thread::spawn(move || {
            second.wait();
            b.edit_script(
                &br,
                TextEdit {
                    start: 0,
                    end: 0,
                    text: "VO: Second.\n".into(),
                },
            )
            .is_ok()
        });
        assert_eq!(
            usize::from(one.join().unwrap()) + usize::from(two.join().unwrap()),
            1
        );
        let saved = std::fs::read_to_string(root.join("script/script.md")).unwrap();
        assert!(saved.starts_with("VO: First.\nLIVE:") || saved.starts_with("VO: Second.\nLIVE:"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn project_configuration_changes_require_a_new_session() {
        let mut e = editor();
        let read = e.read(Document::Script).unwrap();
        let mut changed = e.project.file.clone();
        changed.script = "script/another.md".into();
        crate::project::write_json(&e.project.root.join("project.json"), &changed).unwrap();
        assert!(
            e.edit_script(
                &read.revision,
                TextEdit {
                    start: 0,
                    end: 0,
                    text: "wrong file".into()
                }
            )
            .is_err()
        );
        assert!(e.project.read_script().unwrap().starts_with("LIVE:"));
        std::fs::remove_dir_all(e.project.root).unwrap();
    }
    #[test]
    fn hostile_config_cannot_target_approval_or_outside_files() {
        let e = editor();
        let root = e.project.root.clone();
        for path in [
            "../outside.md",
            "sync/approved-script.md",
            visuals::FILE,
            "C:/outside.md",
        ] {
            let mut project = e.project.file.clone();
            project.script = path.into();
            crate::project::write_json(&root.join("project.json"), &project).unwrap();
            assert!(Editor::open(&root).is_err(), "{path}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
