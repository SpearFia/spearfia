//! `spearfia-ops`: the command layer. The desktop app, the CLI and (next) the MCP server
//! change a project only through here, so every surface gets the same guarantees:
//!
//! - **Plan, then apply.** A command computes a [`Plan`]: the new text of one document, a
//!   summary, and the version of the text it was computed from. Showing the plan's diff is a
//!   dry run; nothing is written until [`apply`].
//! - **No stale writes.** [`apply`] re-reads the file and refuses if it no longer has the
//!   plan's base version (someone, or something, edited it since).
//! - **One history.** Every applied plan is recorded in `sync/history.jsonl` inside the
//!   project, so [`undo`] works the same from the app, the CLI or an agent, and refuses to
//!   undo over a later edit.
//!
//! Commands live in modules per stage; [`transitions`] is the first.

pub mod transitions;

use serde::{Deserialize, Serialize};
use spearfia_core::CoreError;
use spearfia_core::project::Project;
use spearfia_script::Script;
use std::fmt;

const HISTORY: &str = "sync/history.jsonl";

/// A document's version: a hash of its exact bytes, e.g. `v3f9a1c0d2e4b`.
pub fn version(text: &str) -> String {
    // FNV-1a: stable across builds and processes, so a version printed by the CLI can be
    // passed back to it (or to the app) later.
    let h = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("v{:012x}", h >> 16)
}

#[derive(Debug)]
pub enum OpError {
    /// The document changed since it was read.
    Stale {
        doc: String,
        expected: String,
        current: String,
    },
    /// The thing the command points at doesn't exist (`cut 9`, no history).
    NotFound(String),
    /// The command can't do what was asked (`can't set a duration on a plain cut`).
    Invalid(String),
    Core(CoreError),
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale {
                doc,
                expected,
                current,
            } => write!(
                f,
                "{doc} changed since it was read (expected {expected}, now {current}). Read it again and retry."
            ),
            Self::NotFound(m) | Self::Invalid(m) => f.write_str(m),
            Self::Core(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OpError {}

impl From<CoreError> for OpError {
    fn from(e: CoreError) -> Self {
        Self::Core(e)
    }
}

pub type Result<T> = std::result::Result<T, OpError>;

/// One document as read: its project-relative path, text, parse and version.
pub struct Doc {
    pub rel: String,
    pub text: String,
    pub script: Script,
    pub version: String,
}

impl Doc {
    /// The project's main script.
    pub fn main(project: &Project) -> Result<Self> {
        Self::load(project, &project.file.script.replace('\\', "/"))
    }

    pub fn load(project: &Project, rel: &str) -> Result<Self> {
        Ok(Self::from_text(rel, project.read_doc(rel)?))
    }

    /// A document from text already in memory (the app's open buffer).
    pub fn from_text(rel: &str, text: String) -> Self {
        Self {
            rel: rel.to_owned(),
            script: spearfia_script::parse(&text),
            version: version(&text),
            text,
        }
    }

    /// Fail with [`OpError::Stale`] unless this is the version the caller expects.
    pub fn expect(&self, expected: Option<&str>) -> Result<()> {
        match expected {
            Some(v) if v != self.version => Err(OpError::Stale {
                doc: self.rel.clone(),
                expected: v.to_owned(),
                current: self.version.clone(),
            }),
            _ => Ok(()),
        }
    }

    /// A plan that turns this document into `text`.
    pub fn plan(&self, text: String, summary: impl Into<String>) -> Plan {
        Plan {
            doc: self.rel.clone(),
            base: self.version.clone(),
            before: self.text.clone(),
            text,
            summary: summary.into(),
        }
    }
}

/// A change to one document, not yet written.
#[derive(Debug, Clone)]
pub struct Plan {
    pub doc: String,
    /// Version of the text the plan was computed from.
    pub base: String,
    pub before: String,
    pub text: String,
    pub summary: String,
}

impl Plan {
    pub fn changes(&self) -> bool {
        self.text != self.before
    }

    /// Unified diff of the change, for review before applying.
    pub fn diff(&self) -> String {
        similar::TextDiff::from_lines(&self.before, &self.text)
            .unified_diff()
            .context_radius(2)
            .header(&format!("a/{}", self.doc), &format!("b/{}", self.doc))
            .to_string()
    }
}

/// Who made a change, shown in history and undo messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    App,
    Cli,
    Agent,
}

/// One applied change, as stored in `sync/history.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub origin: Origin,
    pub summary: String,
    pub doc: String,
    /// The document before the change, to restore on undo.
    pub before: String,
    /// The version right after the change; undo only runs if the file still has it.
    pub after: String,
    pub at: u64,
}

/// Write the plan if the document still has its base version, and record it for undo.
/// Returns the new version. A plan that changes nothing writes nothing.
pub fn apply(project: &Project, plan: &Plan, origin: Origin) -> Result<String> {
    let current = Doc::load(project, &plan.doc)?;
    current.expect(Some(&plan.base))?;
    if !plan.changes() {
        return Ok(current.version);
    }
    project.write_doc(&plan.doc, &plan.text)?;
    let after = version(&plan.text);
    let mut log = history(project)?;
    log.push(Entry {
        origin,
        summary: plan.summary.clone(),
        doc: plan.doc.clone(),
        before: plan.before.clone(),
        after: after.clone(),
        at: spearfia_core::comments::now(),
    });
    save_history(project, &log)?;
    Ok(after)
}

/// Every recorded change, oldest first.
pub fn history(project: &Project) -> Result<Vec<Entry>> {
    let text = match project.read_doc(HISTORY) {
        Ok(t) => t,
        Err(CoreError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(e) => return Err(e.into()),
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l)
                .map_err(|e| OpError::Invalid(format!("{HISTORY} can't be read ({e}); undo is unavailable until it's fixed or deleted.")))
        })
        .collect()
}

fn save_history(project: &Project, log: &[Entry]) -> Result<()> {
    let mut out = String::new();
    for e in log {
        let line = serde_json::to_string(e).map_err(|e| OpError::Invalid(e.to_string()))?;
        out.push_str(&line);
        out.push('\n');
    }
    Ok(project.write_doc(HISTORY, &out)?)
}

/// Undo the most recent change. Refuses if its document was edited after that change, so
/// undo never throws away someone else's later work. Returns what was undone.
pub fn undo(project: &Project) -> Result<Entry> {
    let mut log = history(project)?;
    let last = log
        .pop()
        .ok_or_else(|| OpError::NotFound("Nothing to undo.".into()))?;
    let current = Doc::load(project, &last.doc)?;
    if current.version != last.after {
        return Err(OpError::Invalid(format!(
            "Can't undo \"{}\": {} was edited after it.",
            last.summary, last.doc
        )));
    }
    project.write_doc(&last.doc, &last.before)?;
    save_history(project, &log)?;
    Ok(last)
}

#[cfg(test)]
pub(crate) mod testutil {
    use spearfia_core::project::Project;

    /// A project folder holding `script` as its main script.
    pub fn project(label: &str, script: &str) -> Project {
        let root =
            std::env::temp_dir().join(format!("spearfia-ops-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("script")).unwrap();
        std::fs::write(root.join("script/script.md"), script).unwrap();
        Project {
            file: serde_json::from_value(serde_json::json!({
                "name": "t", "spec_version": 1, "adapter": "filmcraft"
            }))
            .unwrap(),
            root,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use testutil::project;

    #[test]
    fn versions_are_stable_and_distinct() {
        assert_eq!(version("abc"), version("abc"));
        assert_ne!(version("abc"), version("abd"));
        assert_eq!(version("abc").len(), 13);
    }

    #[test]
    fn apply_refuses_stale_plans_and_undo_restores() {
        let p = project("apply", "hello\n");
        let doc = Doc::main(&p).unwrap();
        let plan = doc.plan("hello world\n".into(), "Add world");
        assert!(plan.diff().contains("+hello world"), "{}", plan.diff());
        let v = apply(&p, &plan, Origin::Cli).unwrap();
        assert_eq!(v, version("hello world\n"));
        // The same plan again is stale: the file moved on.
        assert!(matches!(
            apply(&p, &plan, Origin::Cli),
            Err(OpError::Stale { .. })
        ));
        assert_eq!(undo(&p).unwrap().summary, "Add world");
        assert_eq!(p.read_doc("script/script.md").unwrap(), "hello\n");
        assert!(matches!(undo(&p), Err(OpError::NotFound(_))));
    }

    #[test]
    fn undo_never_overwrites_a_later_edit() {
        let p = project("undo", "a\n");
        let doc = Doc::main(&p).unwrap();
        apply(&p, &doc.plan("b\n".into(), "a to b"), Origin::App).unwrap();
        p.write_doc("script/script.md", "edited by hand\n").unwrap();
        let err = undo(&p).unwrap_err().to_string();
        assert!(err.contains("was edited after it"), "{err}");
        assert_eq!(p.read_doc("script/script.md").unwrap(), "edited by hand\n");
        assert_eq!(history(&p).unwrap().len(), 1, "the entry stays");
    }

    #[test]
    fn expected_versions_are_checked() {
        let doc = Doc::from_text("script/script.md", "x".into());
        assert!(doc.expect(None).is_ok());
        assert!(doc.expect(Some(&version("x"))).is_ok());
        assert!(matches!(
            doc.expect(Some("v000")),
            Err(OpError::Stale { .. })
        ));
    }
}
