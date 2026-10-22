//! `comments.json`: comments and suggested edits on the script, like comments in a doc.
//!
//! A comment keeps the quoted text, the document it is on and the line it was on, and is
//! found again after edits the same way links are: the nearest occurrence of the quote. A
//! suggestion is a comment that proposes replacement text; accepting it edits the script.
//!
//! The format is not in `spearfia-spec` yet; it needs a `comments` schema there.

use crate::error::{CoreError, Result};
use crate::project::{Project, ensure_loadable, write_json};
use serde::{Deserialize, Serialize};
use std::fs;
use std::ops::Range;

pub const COMMENTS_FILE: &str = "comments.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub author: String,
    pub body: String,
    /// Unix seconds.
    pub created: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    /// Project-relative path of the document (`script/script.md`).
    pub doc: String,
    /// The commented text, as it appeared.
    pub quote: String,
    /// Zero-based line where the quote started.
    pub line: usize,
    pub author: String,
    pub body: String,
    pub created: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<Reply>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comments {
    #[serde(default)]
    pub comments: Vec<Comment>,
    /// The number the next new id gets, so a deleted comment's id is never handed out again.
    #[serde(default)]
    pub next_id: u32,
}

impl Comments {
    /// Read `comments.json`; a missing file is an empty list.
    pub fn load(project: &Project) -> Result<Self> {
        let path = project.root.join(COMMENTS_FILE);
        match fs::read_to_string(&path) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| CoreError::json(&path, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(CoreError::io(&path, e)),
        }
    }

    /// Write `comments.json`. Refuses if the file on disk can't be read, so a load failure
    /// never turns into an empty file.
    pub fn save(&self, project: &Project) -> Result<()> {
        let path = project.root.join(COMMENTS_FILE);
        ensure_loadable::<Self>(&path)?;
        write_json(&path, self)
    }

    /// Add a comment and return its id (`c1`, `c2`, ...). Ids are never reused.
    pub fn add(&mut self, mut c: Comment) -> String {
        let n = self
            .comments
            .iter()
            .filter_map(|c| c.id.strip_prefix('c')?.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(self.next_id);
        self.next_id = n.saturating_add(1);
        c.id = format!("c{n}");
        let id = c.id.clone();
        self.comments.push(c);
        id
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Comment> {
        self.comments.iter_mut().find(|c| c.id == id)
    }

    pub fn remove(&mut self, id: &str) {
        self.comments.retain(|c| c.id != id);
    }

    /// Open comments on one document.
    pub fn open_on<'a>(&'a self, doc: &'a str) -> impl Iterator<Item = &'a Comment> + 'a {
        self.comments
            .iter()
            .filter(move |c| !c.resolved && c.doc == doc)
    }
}

/// `src` with the suggestion applied: its quote, found again, replaced by its text. `None`
/// when the comment isn't a suggestion or its quote is no longer in `src`; then nothing
/// should be resolved, because the change was not made.
pub fn apply_suggestion(src: &str, c: &Comment) -> Option<String> {
    let s = c.suggestion.as_deref()?;
    let r = find_quote(src, &c.quote, c.line)?;
    Some(format!("{}{s}{}", &src[..r.start], &src[r.end..]))
}

/// Find `quote` in `src` again: the occurrence whose line is closest to `line_hint`.
/// Returns its byte range.
pub fn find_quote(src: &str, quote: &str, line_hint: usize) -> Option<Range<usize>> {
    if quote.is_empty() {
        return None;
    }
    let mut best: Option<(usize, Range<usize>)> = None;
    let mut from = 0;
    while let Some(i) = src[from..].find(quote) {
        let at = from + i;
        let line = src[..at].matches('\n').count();
        let d = line.abs_diff(line_hint);
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, at..at + quote.len()));
        }
        from = at + quote.len().max(1);
        if from >= src.len() {
            break;
        }
    }
    best.map(|(_, r)| r)
}

/// Seconds since the Unix epoch.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(quote: &str, line: usize) -> Comment {
        Comment {
            id: String::new(),
            doc: "script/script.md".into(),
            quote: quote.into(),
            line,
            author: "Caesar".into(),
            body: "Needs the barrier hit here".into(),
            created: 0,
            suggestion: None,
            resolved: false,
            replies: vec![],
        }
    }

    #[test]
    fn ids_and_filters() {
        let mut c = Comments::default();
        assert_eq!(c.add(comment("a", 0)), "c1");
        let id = c.add(comment("b", 1));
        c.get_mut(&id).unwrap().resolved = true;
        assert_eq!(c.open_on("script/script.md").count(), 1);
        assert_eq!(c.open_on("script/bible.md").count(), 0);
        c.remove("c1");
        assert_eq!(c.add(comment("c", 2)), "c3");
        // Deleting the newest comment must not free its id.
        c.remove("c3");
        assert_eq!(c.add(comment("d", 3)), "c4");
        let back: Comments = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back.clone().add(comment("e", 4)), "c5");
    }

    #[test]
    fn suggestions_apply_only_where_the_quote_still_is() {
        let mut s = comment("24 hours", 0);
        s.suggestion = Some("8 hours".into());
        assert_eq!(
            apply_suggestion("I spent 24 hours\n", &s).as_deref(),
            Some("I spent 8 hours\n")
        );
        assert_eq!(
            apply_suggestion("I spent a day\n", &s),
            None,
            "quote edited away"
        );
        assert_eq!(
            apply_suggestion("I spent 24 hours\n", &comment("24", 0)),
            None,
            "not a suggestion"
        );
    }

    #[test]
    fn never_overwrites_an_unreadable_file() {
        let root = crate::testutil::temp_dir("comments");
        let p = Project {
            file: serde_json::from_value(serde_json::json!({
                "name": "t", "spec_version": 1, "adapter": "filmcraft"
            }))
            .unwrap(),
            root,
        };
        let path = p.root.join(COMMENTS_FILE);
        // Valid JSON but not a comments file: loading fails, so saving must too.
        fs::write(&path, r#"{ "comments": [ { "id": 7 } ] }"#).unwrap();
        assert!(Comments::load(&p).is_err());
        assert!(Comments::default().save(&p).is_err());
        assert!(fs::read_to_string(&path).unwrap().contains("\"id\": 7"));
        fs::remove_file(&path).unwrap();
        Comments::default().save(&p).unwrap();
    }

    #[test]
    fn finds_the_nearest_quote() {
        let src = "hit the apex\nother\nhit the apex\n";
        assert_eq!(find_quote(src, "hit the apex", 0), Some(0..12));
        assert_eq!(find_quote(src, "hit the apex", 2), Some(19..31));
        assert_eq!(find_quote(src, "missing", 0), None);
    }

    #[test]
    fn round_trips_json() {
        let mut c = Comments::default();
        let mut s = comment("24.", 9);
        s.suggestion = Some("8.".into());
        c.add(s);
        let json = serde_json::to_string(&c).unwrap();
        let back: Comments = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
        assert!(!json.contains("replies"));
    }
}
