//! `links.json`: which media covers which words of the script.
//!
//! A link stores the quoted words and the script line they were on. Lines move when the
//! script is edited, so [`locate`] finds the quote again: on the hinted line first, then
//! the nearest line that contains it. A link whose words were deleted is reported as lost
//! instead of silently pointing at the wrong line.
//!
//! The format is not in `spearfia-spec` yet; it needs a `links` schema there.

use crate::error::{CoreError, Result};
use crate::project::{MediaRole, Project, write_json};
use serde::{Deserialize, Serialize};
use std::fs;

pub const LINKS_FILE: &str = "links.json";

/// Whether the media supplies picture or sound for the words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkKind {
    /// Shown on screen (B-roll, facecam, screen capture, graphics).
    Picture,
    /// Heard (voiceover).
    Audio,
}

impl LinkKind {
    /// Voiceover folders supply sound; everything else supplies picture.
    pub fn for_role(role: MediaRole) -> Self {
        match role {
            MediaRole::Voiceover => Self::Audio,
            _ => Self::Picture,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkMedia {
    /// Project-relative (or absolute) path, `/`-separated.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub id: String,
    /// The linked words, single-spaced, as they appear in the script.
    pub quote: String,
    /// Zero-based line in `script.md` where the quote was when linked.
    pub line: usize,
    pub media: LinkMedia,
    pub role: MediaRole,
    pub kind: LinkKind,
    /// Source in/out points (`HH:MM:SS` or seconds), when set.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "in")]
    pub in_point: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "out")]
    pub out_point: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Links {
    #[serde(default)]
    pub links: Vec<Link>,
}

impl Links {
    /// Read `links.json`; a missing file is an empty list. Entries in the older
    /// sample format (no `quote`) are skipped rather than failing the whole file.
    pub fn load(project: &Project) -> Result<Self> {
        let path = project.root.join(LINKS_FILE);
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(CoreError::io(&path, e)),
        };
        let raw: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| CoreError::json(&path, e))?;
        let links = raw
            .get("links")
            .and_then(|l| l.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| serde_json::from_value::<Link>(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self { links })
    }

    pub fn save(&self, project: &Project) -> Result<()> {
        write_json(&project.root.join(LINKS_FILE), self)
    }

    /// Add a link and return its new id (`k1`, `k2`, ...).
    pub fn add(&mut self, mut link: Link) -> String {
        let next = self
            .links
            .iter()
            .filter_map(|l| l.id.strip_prefix('k')?.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        link.id = format!("k{next}");
        let id = link.id.clone();
        self.links.push(link);
        id
    }

    pub fn remove(&mut self, id: &str) -> Option<Link> {
        let i = self.links.iter().position(|l| l.id == id)?;
        Some(self.links.remove(i))
    }

    /// How many links use this media file.
    pub fn uses(&self, media_path: &str) -> usize {
        self.links
            .iter()
            .filter(|l| l.media.path == media_path)
            .count()
    }
}

/// Where a quote sits now: line index (into the `lines` given to [`locate`]) and the
/// inclusive word range on that line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub line: usize,
    pub first_word: usize,
    pub last_word: usize,
}

/// Find `quote` among `lines`, where each line is `(script line number, words)`.
/// Tries the line numbered `hint` first, then the closest line containing the quote.
pub fn locate(quote: &str, hint: usize, lines: &[(usize, Vec<&str>)]) -> Option<Anchor> {
    let needle: Vec<&str> = quote.split_whitespace().collect();
    if needle.is_empty() {
        return None;
    }
    let find_in = |words: &[&str]| {
        words
            .windows(needle.len())
            .position(|w| w == needle.as_slice())
    };
    let mut best: Option<(usize, Anchor)> = None;
    for (i, (line_no, words)) in lines.iter().enumerate() {
        if let Some(start) = find_in(words) {
            let dist = line_no.abs_diff(hint);
            if best.is_none_or(|(d, _)| dist < d) {
                best = Some((
                    dist,
                    Anchor {
                        line: i,
                        first_word: start,
                        last_word: start + needle.len() - 1,
                    },
                ));
            }
        }
    }
    best.map(|(_, a)| a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines<'a>(src: &[(usize, &'a str)]) -> Vec<(usize, Vec<&'a str>)> {
        src.iter()
            .map(|(n, s)| (*n, s.split_whitespace().collect()))
            .collect()
    }

    #[test]
    fn locates_on_the_hinted_line() {
        let l = lines(&[(3, "a cinematic drift, you spinning into a barrier,")]);
        let a = locate("you spinning into a barrier,", 3, &l).unwrap();
        assert_eq!((a.line, a.first_word, a.last_word), (0, 3, 7));
    }

    #[test]
    fn follows_the_quote_when_lines_move() {
        let l = lines(&[(2, "new intro line"), (9, "And it almost made me quit.")]);
        let a = locate("almost made me quit.", 7, &l).unwrap();
        assert_eq!(a.line, 1);
        assert_eq!(a.first_word, 2);
    }

    #[test]
    fn prefers_the_closest_repeat() {
        let l = lines(&[(1, "hit the apex"), (20, "hit the apex")]);
        assert_eq!(locate("hit the apex", 18, &l).unwrap().line, 1);
    }

    #[test]
    fn lost_quotes_are_none() {
        let l = lines(&[(1, "something else")]);
        assert!(locate("you celebrating", 1, &l).is_none());
        assert!(locate("   ", 1, &l).is_none());
    }

    #[test]
    fn ids_increment() {
        let mut links = Links::default();
        let link = Link {
            id: String::new(),
            quote: "a".into(),
            line: 0,
            media: LinkMedia {
                path: "B-roll/a.mp4".into(),
                hash: None,
            },
            role: MediaRole::BRoll,
            kind: LinkKind::Picture,
            in_point: None,
            out_point: None,
        };
        assert_eq!(links.add(link.clone()), "k1");
        assert_eq!(links.add(link.clone()), "k2");
        links.remove("k1");
        assert_eq!(links.add(link), "k3");
        assert_eq!(links.uses("B-roll/a.mp4"), 2);
    }
}
