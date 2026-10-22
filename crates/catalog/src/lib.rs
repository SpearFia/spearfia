//! `spearfia-catalog`: asset catalog. Loads `*.asset.json` descriptors (the `asset`
//! schema) from pluggable sources and searches them. Local folders are the only source
//! for now; a Git-based community registry and third-party APIs come later.

pub mod sources;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub use sources::local::LocalSource;

/// The crate name, used in logs.
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Transition,
    Effect,
    Music,
    Sfx,
    Template,
    Preset,
    Recipe,
    Lut,
    Font,
    Broll,
}

impl AssetKind {
    /// Kinds in the order the sidebar lists them.
    pub const ALL: [Self; 10] = [
        Self::Transition,
        Self::Effect,
        Self::Music,
        Self::Sfx,
        Self::Template,
        Self::Recipe,
        Self::Preset,
        Self::Lut,
        Self::Font,
        Self::Broll,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Transition => "Transitions",
            Self::Effect => "Effects & looks",
            Self::Music => "Music",
            Self::Sfx => "Sound effects",
            Self::Template => "Templates",
            Self::Preset => "Vision presets",
            Self::Recipe => "Recipes",
            Self::Lut => "LUTs",
            Self::Font => "Fonts",
            Self::Broll => "Stock B-roll",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Transition => "transition",
            Self::Effect => "effect",
            Self::Music => "music",
            Self::Sfx => "sfx",
            Self::Template => "template",
            Self::Preset => "preset",
            Self::Recipe => "recipe",
            Self::Lut => "lut",
            Self::Font => "font",
            Self::Broll => "broll",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSource {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// One capability an effect exposes (`pip.face-circle`), with what it needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub inputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub binds: HashMap<String, String>,
}

/// An `*.asset.json` descriptor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub kind: AssetKind,
    #[serde(default)]
    pub summary: Option<String>,
    pub license: String,
    pub source: AssetSource,
    pub hash: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub hosts: Vec<serde_json::Value>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    /// Music and SFX details. Not in the `asset` schema yet (it allows extra fields).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioInfo>,
}

/// Tempo, length and sync points of a music track or sound effect.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f32>,
    /// Length in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f32>,
    /// Named moments (`drop`, `outro`) in seconds from the start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<AudioMarker>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioMarker {
    pub name: String,
    pub at: f32,
}

/// Whether an asset's license is safe to publish with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseStatus {
    /// A known open license (CC0, CC-BY, MIT, Apache, ...).
    Open,
    /// Some license is named, but check its terms (e.g. "Royalty-free").
    Check,
    /// Missing or unknown: keep out of published videos.
    Unknown,
}

const OPEN_LICENSES: [&str; 8] = [
    "CC0-1.0",
    "CC-BY-4.0",
    "CC-BY-3.0",
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "OFL-1.1",
];

/// A name or label for comparing: trimmed, Unicode lowercase, and `-`, `_` and a space are
/// the same character, so `Telemetry Overlay`, `telemetry-overlay` and `telemetry_overlay`
/// are one name (and `Écho` is `écho`).
pub fn norm_label(s: &str) -> String {
    s.trim()
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| if c == ' ' || c == '_' { '-' } else { c })
        .collect()
}

impl Asset {
    /// `hype-drop` from `spearfia/hype-drop@0.1.0`.
    pub fn short_name(&self) -> &str {
        self.id
            .split_once('/')
            .map(|(_, n)| n.split('@').next().unwrap_or(n))
            .unwrap_or(&self.id)
    }

    /// Whether a cue label (`hype-drop`, `spearfia/hype-drop@0.1.0`, the display name)
    /// names this asset.
    pub fn answers_to(&self, label: &str) -> bool {
        self.match_quality(label).is_some()
    }

    /// How well a label names this asset: `Some(0)` written exactly as the id, the id
    /// without its version, the short name or the name; `Some(1)` the same apart from case
    /// and `-` / `_` / space; `None` if it doesn't name it.
    pub fn match_quality(&self, label: &str) -> Option<u8> {
        let unversioned = self.id.split('@').next().unwrap_or(&self.id);
        let names = [
            self.id.as_str(),
            unversioned,
            self.short_name(),
            self.name.as_str(),
        ];
        let raw = label.trim();
        if names.contains(&raw) {
            return Some(0);
        }
        let l = norm_label(raw);
        names.iter().any(|n| norm_label(n) == l).then_some(1)
    }

    /// Whether one of the asset's tags is this label (same rules as [`norm_label`]).
    pub fn has_tag(&self, label: &str) -> bool {
        let l = norm_label(label);
        self.tags.iter().any(|t| norm_label(t) == l)
    }

    pub fn license_status(&self) -> LicenseStatus {
        let l = self.license.trim();
        if l.is_empty() || l.eq_ignore_ascii_case("unknown") || l == "NOASSERTION" {
            return LicenseStatus::Unknown;
        }
        let all_open = l
            .split([' ', '(', ')'])
            .filter(|t| !t.is_empty() && *t != "OR" && *t != "AND")
            .all(|t| OPEN_LICENSES.contains(&t));
        if all_open {
            LicenseStatus::Open
        } else {
            LicenseStatus::Check
        }
    }

    /// Sum of vision weights for this asset's tags and name, clamped to -1..=1.
    pub fn fit(&self, weights: &HashMap<String, f32>) -> f32 {
        let short_name = self.short_name();
        let mut keys: Vec<&str> = self.tags.iter().map(String::as_str).collect();
        keys.push(short_name);
        keys.sort_unstable();
        keys.dedup();
        keys.iter()
            .filter_map(|k| weights.get(*k))
            .fold(0.0, |a, b| a + b)
            .clamp(-1.0, 1.0)
    }
}

/// A loaded asset and where it came from.
#[derive(Debug, Clone)]
pub struct Entry {
    pub asset: Asset,
    pub descriptor: PathBuf,
    /// Display name of the source (`starter-pack`, `E:\SFX library`, ...).
    pub source: String,
}

/// A descriptor that could not be read.
#[derive(Debug, Clone)]
pub struct LoadError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct Query {
    pub text: String,
    pub kind: Option<AssetKind>,
    pub only_open_licenses: bool,
    /// Vision weights: tag → bias (-1 avoid … 1 prefer).
    pub weights: HashMap<String, f32>,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub entries: Vec<Entry>,
    pub errors: Vec<LoadError>,
}

impl Catalog {
    pub fn load(sources: &[LocalSource]) -> Self {
        let mut c = Self::default();
        for s in sources {
            s.scan_into(&mut c);
        }
        c.entries.sort_by(|a, b| {
            a.asset
                .name
                .to_lowercase()
                .cmp(&b.asset.name.to_lowercase())
        });
        c
    }

    /// The asset of `kind` a cue label names. An exact spelling wins over a match that
    /// ignores case and separators; among equals, catalog order decides. Use
    /// [`Catalog::find_all`] to see whether the label is ambiguous.
    pub fn find(&self, kind: AssetKind, label: &str) -> Option<&Entry> {
        self.find_all(kind, label).into_iter().next()
    }

    /// Every asset of `kind` a cue label names, best match first.
    pub fn find_all(&self, kind: AssetKind, label: &str) -> Vec<&Entry> {
        let mut hits: Vec<(u8, &Entry)> = self
            .entries
            .iter()
            .filter(|e| e.asset.kind == kind)
            .filter_map(|e| Some((e.asset.match_quality(label)?, e)))
            .collect();
        hits.sort_by_key(|(q, _)| *q);
        hits.into_iter().map(|(_, e)| e).collect()
    }

    /// Assets of `kind` with this label as a tag, in catalog order.
    pub fn tagged(&self, kind: AssetKind, label: &str) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| e.asset.kind == kind && e.asset.has_tag(label))
            .collect()
    }

    pub fn count(&self, kind: AssetKind) -> usize {
        self.entries.iter().filter(|e| e.asset.kind == kind).count()
    }

    /// Matching entries, best first. Every word of the query must appear in the name,
    /// tags, summary or id; matches in the name count most. Vision weights break ties.
    pub fn search(&self, q: &Query) -> Vec<&Entry> {
        let words: Vec<String> = q.text.split_whitespace().map(str::to_lowercase).collect();
        let mut hits: Vec<(f32, &Entry)> = self
            .entries
            .iter()
            .filter(|e| q.kind.is_none_or(|k| e.asset.kind == k))
            .filter(|e| !q.only_open_licenses || e.asset.license_status() == LicenseStatus::Open)
            .filter_map(|e| {
                let a = &e.asset;
                let name = a.name.to_lowercase();
                let summary = a.summary.as_deref().unwrap_or("").to_lowercase();
                let id = a.id.to_lowercase();
                let mut score = 0.0;
                for w in &words {
                    let s = if name.contains(w.as_str()) {
                        3.0
                    } else if a.tags.iter().any(|t| t.to_lowercase().contains(w.as_str())) {
                        2.0
                    } else if summary.contains(w.as_str()) || id.contains(w.as_str()) {
                        1.0
                    } else {
                        return None;
                    };
                    score += s;
                }
                Some((score + a.fit(&q.weights), e))
            })
            .collect();
        hits.sort_by(|a, b| b.0.total_cmp(&a.0));
        hits.into_iter().map(|(_, e)| e).collect()
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub fn temp_dir(label: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "spearfia-catalog-{label}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::temp_dir;
    use std::fs;

    fn asset_json(id: &str, kind: &str, license: &str, tags: &[&str]) -> String {
        let name = id.split('/').nth(1).unwrap().split('@').next().unwrap();
        serde_json::json!({
            "$schema": "https://spearfia.dev/schemas/asset/v1.json",
            "id": id, "name": name, "kind": kind, "license": license,
            "summary": format!("{name} test asset"),
            "source": { "type": "local" },
            "hash": format!("sha256:{}", "0".repeat(64)),
            "tags": tags,
        })
        .to_string()
    }

    fn catalog() -> Catalog {
        let dir = temp_dir("search");
        let files = [
            (
                "sfx/tire/tire-screech.asset.json",
                asset_json(
                    "spearfia/tire-screech@0.2.0",
                    "sfx",
                    "CC0-1.0",
                    &["tire", "skid", "drift"],
                ),
            ),
            (
                "sfx/brake/brake-lockup.asset.json",
                asset_json(
                    "local/brake-lockup@0.1.0",
                    "sfx",
                    "unknown",
                    &["brake", "skid"],
                ),
            ),
            (
                "transitions/whip/whip-pan.asset.json",
                asset_json(
                    "spearfia/whip-pan@0.2.0",
                    "transition",
                    "CC0-1.0",
                    &["whip-pan"],
                ),
            ),
            ("broken.asset.json", "{ not json".into()),
            (
                ".git/ignored.asset.json",
                asset_json("x/ignored@1.0.0", "sfx", "CC0-1.0", &[]),
            ),
        ];
        for (p, body) in files {
            let path = dir.join(p);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }
        Catalog::load(&[LocalSource::new("test", dir)])
    }

    #[test]
    fn loads_descriptors_and_reports_errors() {
        let c = catalog();
        assert_eq!(c.entries.len(), 3);
        assert_eq!(c.errors.len(), 1);
        assert_eq!(c.count(AssetKind::Sfx), 2);
        assert!(c.entries.iter().all(|e| e.source == "test"));
    }

    #[test]
    fn search_ranks_and_filters() {
        let c = catalog();
        let mut q = Query {
            text: "skid".into(),
            ..Default::default()
        };
        assert_eq!(c.search(&q).len(), 2);
        q.only_open_licenses = true;
        let hits = c.search(&q);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].asset.name, "tire-screech");

        q = Query {
            kind: Some(AssetKind::Transition),
            ..Default::default()
        };
        assert_eq!(c.search(&q)[0].asset.name, "whip-pan");
        q.text = "nothing-matches".into();
        assert!(c.search(&q).is_empty());
    }

    #[test]
    fn vision_weights_reorder_results() {
        let c = catalog();
        let q = Query {
            kind: Some(AssetKind::Sfx),
            weights: HashMap::from([("brake".to_owned(), 0.9)]),
            ..Default::default()
        };
        assert_eq!(c.search(&q)[0].asset.name, "brake-lockup");
    }

    #[test]
    fn license_status() {
        let mut a: Asset =
            serde_json::from_str(&asset_json("a/b@1.0.0", "sfx", "MIT OR Apache-2.0", &[]))
                .unwrap();
        assert_eq!(a.license_status(), LicenseStatus::Open);
        a.license = "Royalty-free".into();
        assert_eq!(a.license_status(), LicenseStatus::Check);
        a.license = "".into();
        assert_eq!(a.license_status(), LicenseStatus::Unknown);
    }

    #[test]
    fn reads_the_focus_loupe_descriptor() {
        let json = r#"{
          "id": "spearfia/focus-loupe@0.1.0", "name": "Focus Loupe", "kind": "effect",
          "license": "MIT OR Apache-2.0",
          "source": { "type": "git", "url": "https://github.com/spearfia/plugins" },
          "hash": "sha256:3b22726b2b60a68301ecf5f2f2d9b9ba5b4394a4a1385101ddf6018648e66039",
          "capabilities": [{
            "id": "pip.face-circle", "summary": "Circle PiP.",
            "inputs": { "radius": { "type": "float", "default": 0.15 } },
            "requires": ["face_track"],
            "binds": { "center": "face_track.center" }
          }],
          "tags": ["zoom", "pip"]
        }"#;
        let a: Asset = serde_json::from_str(json).unwrap();
        assert_eq!(a.capabilities[0].requires, ["face_track"]);
        assert_eq!(a.capabilities[0].binds["center"], "face_track.center");
    }

    #[test]
    fn audio_info_and_label_lookup() {
        let json = r#"{
          "id": "spearfia/hype-drop@0.1.0", "name": "Hype Drop", "kind": "music",
          "license": "CC0-1.0", "source": { "type": "local" }, "hash": "sha256:00",
          "audio": { "bpm": 140, "seconds": 108, "markers": [{ "name": "drop", "at": 3.4 }] }
        }"#;
        let a: Asset = serde_json::from_str(json).unwrap();
        let audio = a.audio.as_ref().unwrap();
        assert_eq!(audio.bpm, Some(140.0));
        assert_eq!(audio.markers[0].name, "drop");
        for label in [
            "hype-drop",
            "Hype Drop",
            "spearfia/hype-drop",
            "hype_drop",
            "HYPE drop",
            "spearfia/hype-drop@0.1.0",
        ] {
            assert!(a.answers_to(label), "{label}");
        }
        assert!(!a.answers_to("hype"));
        let c = Catalog {
            entries: vec![Entry {
                asset: a,
                descriptor: PathBuf::new(),
                source: "t".into(),
            }],
            errors: vec![],
        };
        assert!(c.find(AssetKind::Music, "hype-drop").is_some());
        assert!(c.find(AssetKind::Sfx, "hype-drop").is_none());
    }

    fn entry(id: &str, name: &str, kind: &str, tags: &[&str]) -> Entry {
        let json = format!(
            r#"{{"id": "{id}", "name": "{name}", "kind": "{kind}", "license": "CC0-1.0",
            "source": {{ "type": "local" }}, "hash": "sha256:00", "tags": {tags:?}}}"#
        );
        Entry {
            asset: serde_json::from_str(&json).unwrap(),
            descriptor: PathBuf::new(),
            source: "t".into(),
        }
    }

    #[test]
    fn unicode_case_is_ignored() {
        let e = entry("spearfia/echo@0.1.0", "Écho", "sfx", &["Réverb"]);
        assert!(e.asset.answers_to("écho"));
        assert!(e.asset.answers_to("ÉCHO"));
        assert!(e.asset.has_tag("réverb"));
    }

    #[test]
    fn exact_names_win_and_ambiguity_is_visible() {
        let c = Catalog {
            entries: vec![
                entry(
                    "spearfia/tire-screech@0.1.0",
                    "Tire Screech",
                    "sfx",
                    &["tire"],
                ),
                entry(
                    "spearfia/tire-screech-2@0.1.0",
                    "Tire_Screech",
                    "sfx",
                    &["tire"],
                ),
            ],
            errors: vec![],
        };
        let all = c.find_all(AssetKind::Sfx, "Tire_Screech");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].asset.name, "Tire_Screech", "exact spelling first");
        assert_eq!(
            c.find(AssetKind::Sfx, "Tire Screech").unwrap().asset.name,
            "Tire Screech"
        );
        assert_eq!(c.tagged(AssetKind::Sfx, "TIRE").len(), 2);
    }
}
