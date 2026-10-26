//! `spearfia-vision`: vision presets and (later) layering of app defaults → preset →
//! project `vision.yaml` → section `:vision{}` overrides, weights and the feedback log.
//!
//! For now this holds the built-in presets, which mirror `starter-pack/presets/*.yaml`.
//! Reading presets from YAML replaces them once a YAML reader is added.

use std::collections::HashMap;

/// The crate name, used in logs.
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    /// Name used in `extends:`.
    pub name: &'static str,
    pub title: &'static str,
    pub goal: &'static str,
    pub avg_shot_seconds: f32,
    pub max_silence_seconds: f32,
    /// `jump`, `invisible`, `rhythmic` or `mixed`.
    pub cut_style: &'static str,
    pub weights: &'static [(&'static str, f32)],
}

impl Preset {
    pub fn weight_map(&self) -> HashMap<String, f32> {
        self.weights
            .iter()
            .map(|(k, v)| ((*k).to_owned(), *v))
            .collect()
    }
}

pub use spearfia_core::visions::CUT_STYLES;

const PRESETS: [Preset; 3] = [
    Preset {
        name: "chaotic-gaming-comedy",
        title: "Chaotic gaming comedy",
        goal: "Keep the viewer laughing and never let attention drop.",
        avg_shot_seconds: 1.5,
        max_silence_seconds: 0.25,
        cut_style: "jump",
        weights: &[
            ("record-scratch", 0.8),
            ("zoom-punch", 0.9),
            ("bass-boost", 0.6),
            ("slow-push", -0.5),
        ],
    },
    Preset {
        name: "cinematic",
        title: "Cinematic",
        goal: "Make the viewer feel the moment before they understand it.",
        avg_shot_seconds: 4.5,
        max_silence_seconds: 2.0,
        cut_style: "invisible",
        weights: &[
            ("slow-push", 0.8),
            ("film-grain", 0.5),
            ("whip-pan", -0.6),
            ("meme", -1.0),
        ],
    },
    Preset {
        name: "motorsport-news-react",
        title: "Motorsport news react",
        goal: "Explain what happened in the race and why it matters, with a clear opinion.",
        avg_shot_seconds: 3.0,
        max_silence_seconds: 0.5,
        cut_style: "mixed",
        weights: &[
            ("lower-third", 0.8),
            ("facecam-pip", 0.7),
            ("replay-loupe", 0.6),
            ("meme", -0.3),
        ],
    },
];

pub fn builtin_presets() -> &'static [Preset] {
    &PRESETS
}

pub fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_valid() {
        for p in builtin_presets() {
            assert!(CUT_STYLES.contains(&p.cut_style), "{}", p.name);
            assert!(p.weights.iter().all(|(_, w)| (-1.0..=1.0).contains(w)));
            assert_eq!(preset(p.name), Some(p));
        }
        assert_eq!(
            preset("chaotic-gaming-comedy").unwrap().weight_map()["zoom-punch"],
            0.9
        );
    }
}

/// The built-in templates in the same portable format as imported visions.
pub fn builtin_templates() -> Vec<spearfia_core::visions::Vision> {
    builtin_presets().iter().map(|p| spearfia_core::visions::Vision {
        document: serde_json::json!({
            "$schema": "https://spearfia.dev/schemas/vision/v1.json",
            "name": p.name, "title": p.title, "goal": p.goal,
            "pacing": {"avg_shot_seconds": p.avg_shot_seconds, "max_silence_seconds": p.max_silence_seconds, "cut_style": p.cut_style},
            "weights": p.weight_map(), "script_rules": [], "avoid": [], "references": []
        }),
    }).collect()
}

/// Custom projects embed their template so they keep working on another computer.
pub fn project_weights(project: &spearfia_core::project::Project) -> HashMap<String, f32> {
    if let Ok(file) = std::fs::File::open(project.vision_path()) {
        use std::io::Read;
        let mut source = String::new();
        if file
            .take(spearfia_core::visions::MAX_BYTES as u64 + 1)
            .read_to_string(&mut source)
            .is_ok()
            && source.len() <= spearfia_core::visions::MAX_BYTES
            && let Ok(document) = serde_json::from_str::<serde_json::Value>(&source)
        {
            return spearfia_core::visions::weights(&document);
        }
    }
    project
        .vision_preset()
        .and_then(|name| preset(&name))
        .map(|p| p.weight_map())
        .unwrap_or_default()
}

#[cfg(test)]
mod template_tests {
    #[test]
    fn builtin_templates_use_the_import_format_without_losing_weights() {
        for (template, preset) in super::builtin_templates()
            .iter()
            .zip(super::builtin_presets())
        {
            let reloaded =
                spearfia_core::visions::Vision::parse(&template.to_source().unwrap()).unwrap();
            assert_eq!(reloaded.name(), preset.name);
            assert_eq!(reloaded.weight_map(), preset.weight_map());
        }
    }
}
