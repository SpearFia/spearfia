//! `links.json`: which media covers which words of the script.
//!
//! A link stores the quoted words and the script line they were on. Lines move when the
//! script is edited, so [`locate`] finds the quote again: on the hinted line first, then
//! the nearest line that contains it. A link whose words were deleted is reported as lost
//! instead of silently pointing at the wrong line.
//!
//! The format is not in `spearfia-spec` yet; it needs a `links` schema there.

use crate::error::{CoreError, Result};
use crate::project::{MediaRole, Project, ensure_loadable, write_text};
use serde::{Deserialize, Serialize};
use std::fs;

pub const LINKS_FILE: &str = "links.json";
pub const MAX_BATCH_LINKS: usize = 10_000;
pub const MAX_BATCH_BYTES: usize = 16 * 1024 * 1024;

/// Whether the media supplies picture or sound for the words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkKind {
    /// Shown on screen (B-roll, facecam, screen capture, graphics).
    Picture,
    /// Heard (voiceover, music, sound effects).
    Audio,
}

impl LinkKind {
    /// Voiceover, music and SFX folders supply sound; everything else supplies picture.
    pub fn for_role(role: MediaRole) -> Self {
        match role {
            MediaRole::Voiceover | MediaRole::Music | MediaRole::Sfx => Self::Audio,
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
    /// The number the next new id gets, so a deleted link's id is never handed out again.
    #[serde(default)]
    pub next_id: u32,
    /// Entries this version can't read (e.g. the older sample format with no `quote`).
    /// Not shown, but written back unchanged so saving never deletes them.
    #[serde(skip)]
    pub unreadable: Vec<serde_json::Value>,
}

impl Links {
    /// Read `links.json`; a missing file is an empty list. Entries that don't parse are
    /// kept in [`Links::unreadable`] rather than failing the whole file.
    pub fn load(project: &Project) -> Result<Self> {
        let path = project.root.join(LINKS_FILE);
        match fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).map_err(|e| CoreError::json(&path, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(CoreError::io(&path, e)),
        }
    }

    /// Parse `links.json` text. Empty text (a file created, then undone) is an empty list.
    pub fn parse(text: &str) -> std::result::Result<Self, serde_json::Error> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        let raw: serde_json::Value = serde_json::from_str(text)?;
        let mut out = Self {
            next_id: raw
                .get("next_id")
                .and_then(|n| n.as_u64())
                .map_or(0, |n| n as u32),
            ..Self::default()
        };
        for v in raw
            .get("links")
            .and_then(|l| l.as_array())
            .into_iter()
            .flatten()
        {
            match serde_json::from_value::<Link>(v.clone()) {
                Ok(l) => out.links.push(l),
                Err(_) => out.unreadable.push(v.clone()),
            }
        }
        Ok(out)
    }

    /// The exact text [`Links::save`] writes, unreadable entries included.
    pub fn to_text(&self) -> std::result::Result<String, serde_json::Error> {
        let mut v = serde_json::to_value(self)?;
        if let Some(items) = v.get_mut("links").and_then(|l| l.as_array_mut()) {
            items.extend(self.unreadable.iter().cloned());
        }
        let mut text = serde_json::to_string_pretty(&v)?;
        text.push('\n');
        Ok(text)
    }

    /// Write `links.json`. Refuses if the file on disk can't be read, so a load failure
    /// never turns into an empty file. Edits go through `spearfia-ops` for undo; this is for
    /// moves that rewrite paths.
    pub fn save(&self, project: &Project) -> Result<()> {
        let path = project.root.join(LINKS_FILE);
        ensure_loadable::<serde_json::Value>(&path)?;
        let text = self.to_text().map_err(|e| CoreError::json(&path, e))?;
        write_text(&path, &text)
    }

    /// Add a link and return its new id (`k1`, `k2`, ...). Ids are never reused.
    pub fn add(&mut self, mut link: Link) -> String {
        let next = self
            .links
            .iter()
            .filter_map(|l| l.id.strip_prefix('k')?.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(self.next_id);
        self.next_id = next.saturating_add(1);
        link.id = format!("k{next}");
        let id = link.id.clone();
        self.links.push(link);
        id
    }

    /// Add several clip/line anchors at once, returning their ids. An identical existing
    /// link returns its id instead. On error nothing changes.
    pub fn add_requests(&mut self, requests: Vec<Link>) -> Result<Vec<String>> {
        if requests.len() > MAX_BATCH_LINKS {
            return Err(CoreError::Invalid(
                "Link at most 10,000 clip/line pairs at a time.".into(),
            ));
        }
        let bytes = requests.iter().try_fold(0usize, |sum, l| {
            sum.checked_add(l.quote.len())?
                .checked_add(l.media.path.len())
        });
        if !bytes.is_some_and(|bytes| bytes <= MAX_BATCH_BYTES) {
            return Err(CoreError::Invalid(
                "The media link selection is too large. Link smaller groups of lines or clips."
                    .into(),
            ));
        }
        let mut updated = self.clone();
        let mut ids = Vec::new();
        for request in requests {
            if let Some(existing) = updated.links.iter().find(|l| {
                l.quote == request.quote
                    && l.line == request.line
                    && l.media.path == request.media.path
                    && l.role == request.role
                    && l.kind == request.kind
                    && l.in_point == request.in_point
                    && l.out_point == request.out_point
            }) {
                ids.push(existing.id.clone());
                continue;
            }
            let largest = updated
                .links
                .iter()
                .filter_map(|l| l.id.strip_prefix('k')?.parse::<u32>().ok())
                .max()
                .unwrap_or(0);
            if largest == u32::MAX {
                return Err(CoreError::Invalid(
                    "No media link IDs remain. The links were not changed.".into(),
                ));
            }
            ids.push(updated.add(request));
        }
        *self = updated;
        Ok(ids)
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
        assert_eq!(links.add(link.clone()), "k3");
        assert_eq!(links.uses("B-roll/a.mp4"), 2);
        // Deleting the newest link must not free its id.
        links.remove("k3");
        assert_eq!(links.add(link), "k4");
    }

    fn project() -> Project {
        let root = crate::testutil::temp_dir("links");
        Project {
            root,
            file: serde_json::from_value(serde_json::json!({
                "name": "t", "spec_version": 1, "adapter": "filmcraft"
            }))
            .unwrap(),
        }
    }

    fn sample() -> Link {
        Link {
            id: String::new(),
            quote: "a".into(),
            line: 0,
            media: LinkMedia {
                path: "a.mp4".into(),
                hash: None,
            },
            role: MediaRole::BRoll,
            kind: LinkKind::Picture,
            in_point: None,
            out_point: None,
        }
    }

    #[test]
    fn unreadable_entries_survive_a_save() {
        let p = project();
        let old = serde_json::json!({ "cue_id": "c1", "media": { "path": "x.mp4" } });
        fs::write(
            p.root.join(LINKS_FILE),
            serde_json::json!({ "links": [old] }).to_string(),
        )
        .unwrap();
        let mut links = Links::load(&p).unwrap();
        assert!(links.links.is_empty());
        links.add(sample());
        links.save(&p).unwrap();
        let text = fs::read_to_string(p.root.join(LINKS_FILE)).unwrap();
        assert!(text.contains("\"cue_id\": \"c1\""), "{text}");
        assert_eq!(Links::load(&p).unwrap().links.len(), 1);
    }

    #[test]
    fn ids_stay_unique_across_save_and_load() {
        let p = project();
        let mut links = Links::default();
        links.add(sample());
        links.add(sample());
        links.remove("k2");
        links.save(&p).unwrap();
        assert_eq!(Links::load(&p).unwrap().add(sample()), "k3");
    }

    #[test]
    fn never_overwrites_an_unreadable_file() {
        let p = project();
        let path = p.root.join(LINKS_FILE);
        fs::write(&path, "{ \"links\": [ broken").unwrap();
        assert!(Links::load(&p).is_err());
        let mut fresh = Links::default();
        fresh.add(sample());
        assert!(fresh.save(&p).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ \"links\": [ broken");
    }
    #[test]
    fn batch_links_keep_multiple_clips_and_duplicates_are_idempotent() {
        let project = project();
        let mut links = Links::default();
        let mut first = sample();
        first.quote = "one two".into();
        let mut second = first.clone();
        second.media.path = "b.mp4".into();
        let mut third = second.clone();
        third.line = 4;
        third.quote = "three four".into();
        let requests = vec![first, second, third];
        let ids = links.add_requests(requests.clone()).unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(links.add_requests(requests).unwrap(), ids);
        assert_eq!(links.links.len(), 3);
        // Saved text round-trips, and empty text is an empty list.
        links.save(&project).unwrap();
        assert_eq!(Links::load(&project).unwrap().links, links.links);
        assert_eq!(
            fs::read_to_string(project.root.join(LINKS_FILE)).unwrap(),
            links.to_text().unwrap()
        );
        assert_eq!(Links::parse("").unwrap(), Links::default());
    }

    #[test]
    fn exhausted_batch_ids_report_an_error() {
        let mut links = Links::default();
        let mut old = sample();
        old.id = format!("k{}", u32::MAX);
        links.links.push(old);
        let before = links.clone();
        let mut extra = sample();
        extra.media.path = "b.mp4".into();
        assert!(links.add_requests(vec![extra]).is_err());
        assert_eq!(links, before);
    }
}
