//! Project-bound agent edits: path checks and validation. An edit becomes a [`Proposal`]
//! (before/after text) that `spearfia-ops` applies and records in the project history.
//! Revisions are content versions ([`crate::version`]), so they work across processes.
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
/// A validated change to one document, not yet written.
#[derive(Debug, Clone)]
pub struct Proposal {
    pub document: Document,
    /// Project-relative path, `/`-separated.
    pub rel: String,
    pub before: String,
    pub after: String,
    pub summary: String,
}
/// Agent access to one project's script and Visual Script.
pub struct Editor {
    pub project: Project,
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
        let editor = Self { project };
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
    /// Project-relative path of a document, `/`-separated.
    pub fn rel(&self, document: Document) -> String {
        match document {
            Document::Script => self.project.file.script.replace('\\', "/"),
            Document::Visuals => visuals::FILE.to_owned(),
        }
    }
    pub fn read(&self, document: Document) -> Result<Snapshot> {
        let source = self.disk(document)?;
        Ok(Snapshot {
            document,
            revision: crate::version(&source),
            source,
        })
    }
    fn base(&self, document: Document, expected: &str) -> Result<String> {
        let current = self.disk(document)?;
        if crate::version(&current) != expected {
            return Err(invalid(
                "The document changed since it was read, or the revision is wrong. Read it again and reconcile the edit.",
            ));
        }
        Ok(current)
    }
    fn propose(
        &self,
        document: Document,
        before: String,
        after: String,
        summary: String,
    ) -> Result<Proposal> {
        if after.len() > visuals::MAX_BYTES {
            return Err(invalid("Agent documents are limited to 1 MiB."));
        }
        self.checked_path(document)?;
        Ok(Proposal {
            document,
            rel: self.rel(document),
            before,
            after,
            summary,
        })
    }
    pub fn edit_script(&self, revision: &str, edit: TextEdit) -> Result<Proposal> {
        self.edit_document(Document::Script, revision, edit)
    }
    pub fn edit_document(
        &self,
        document: Document,
        revision: &str,
        edit: TextEdit,
    ) -> Result<Proposal> {
        let source = self.base(document, revision)?;
        let out = visuals::replace(&source, edit.start..edit.end, &edit.text)?;
        if document == Document::Visuals {
            Plan::parse(&out)?;
        }
        let summary = match document {
            Document::Script => "Agent edit to the script",
            Document::Visuals => "Agent edit to the Visual Script",
        };
        self.propose(document, source, out, summary.into())
    }
    pub fn edit_visuals(
        &self,
        revision: &str,
        script_revision: &str,
        edit: VisualEdit,
    ) -> Result<(Proposal, Option<String>)> {
        let source = self.base(Document::Visuals, revision)?;
        let script = self.base(Document::Script, script_revision)?;
        let plan = Plan::parse(&source)?;
        let summary = format!("Agent Visual Script edit: {}", edit.action());
        let (out, id) = apply_visual(&plan, &script, edit)?;
        Plan::parse(&out)?;
        if self.disk(Document::Script)? != script {
            return Err(invalid(
                "Narration changed during the visual edit. Read both documents again.",
            ));
        }
        Ok((self.propose(Document::Visuals, source, out, summary)?, id))
    }
}
impl VisualEdit {
    /// The action name, as in the edit schema.
    pub fn action(&self) -> &'static str {
        match self {
            Self::Create { .. } => "create cue",
            Self::Update { .. } => "update cue",
            Self::Delete { .. } => "delete cue",
            Self::Split { .. } => "split cue",
            Self::Merge { .. } => "merge cues",
            Self::Reanchor { .. } => "reanchor cue",
            Self::BindReference { .. } => "bind reference",
            Self::AddShot { .. } => "add shot",
            Self::MoveShot { .. } => "move shot",
        }
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
    /// Write a proposal the way `spearfia-ops` would, and read the document back.
    fn commit(e: &Editor, p: Proposal) -> Snapshot {
        e.project.write_doc(&p.rel, &p.after).unwrap();
        e.read(p.document).unwrap()
    }
    #[test]
    fn typed_edits_and_references_preserve_ids_and_unknown_prose() {
        let e = editor();
        let s = e.read(Document::Script).unwrap();
        let v = e.read(Document::Visuals).unwrap();
        let (p, id) = e
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
        assert_eq!(p.summary, "Agent Visual Script edit: create cue");
        let v = commit(&e, p);
        let id = id.unwrap();
        let (p, _) = e
            .edit_visuals(
                &v.revision,
                &s.revision,
                VisualEdit::Update {
                    id: id.clone(),
                    patch: CuePatch {
                        body: Some(
                            "VISUAL: A garden.

Unrecognized prose 🍀"
                                .into(),
                        ),
                        method: Some("Reuse".into()),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        let v = commit(&e, p);
        assert!(v.source.contains("Unrecognized prose 🍀"));
        let (p, _) = e
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
        let v = commit(&e, p);
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
        let (p, _) = e
            .edit_visuals(&v.revision, &s.revision, VisualEdit::Delete { id })
            .unwrap();
        assert_eq!(p.before, v.source);
        assert!(Plan::parse(&p.after).unwrap().cues.is_empty());
        std::fs::remove_dir_all(e.project.root).unwrap();
    }
    #[test]
    fn stale_revisions_and_bad_ranges_are_refused() {
        let e = editor();
        let s = e.read(Document::Script).unwrap();
        assert_eq!(s.revision, crate::version(&s.source));
        for (start, end) in [(13, 14), (usize::MAX, 0)] {
            assert!(
                e.edit_script(
                    &s.revision,
                    TextEdit {
                        start,
                        end,
                        text: "bad".into()
                    }
                )
                .is_err()
            );
        }
        let p = e
            .edit_script(
                &s.revision,
                TextEdit {
                    start: 0,
                    end: 0,
                    text: "VO: Intro.
"
                    .into(),
                },
            )
            .unwrap();
        assert_eq!(p.rel, "script/script.md");
        // Proposing writes nothing.
        assert_eq!(e.project.read_script().unwrap(), s.source);
        commit(&e, p);
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
    fn project_configuration_changes_require_a_new_session() {
        let e = editor();
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
