//! User-created, self-contained vision templates, stored separately from app settings.
use crate::error::{CoreError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub const MAX_BYTES: usize = 256 * 1024;
pub const CUT_STYLES: [&str; 4] = ["jump", "invisible", "rhythmic", "mixed"];

#[derive(Debug, Clone, PartialEq)]
pub struct Vision {
    pub document: Value,
}

impl Vision {
    pub fn parse(source: &str) -> Result<Self> {
        let invalid = |message: &str| CoreError::Invalid(message.into());
        if source.len() > MAX_BYTES {
            return Err(invalid("A vision must be smaller than 256 KiB."));
        }
        let document: Value = serde_json::from_str(source)
            .map_err(|e| invalid(&format!("Invalid vision JSON: {e}")))?;
        let object = document
            .as_object()
            .ok_or_else(|| invalid("A vision must be a JSON object."))?;
        let name = object.get("name").and_then(Value::as_str).unwrap_or("");
        if name.is_empty()
            || name.len() > 80
            || name.starts_with('-')
            || name.ends_with('-')
            || !name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return Err(invalid(
                "Use a name of 1–80 lowercase letters, digits and hyphens (for example kurzgesagt-inspired).",
            ));
        }
        if object.contains_key("extends") {
            return Err(invalid(
                "Import a complete template without extends; include its pacing, rules and weights directly.",
            ));
        }
        for key in ["title", "goal", "$schema"] {
            if object.get(key).is_some_and(|v| !v.is_string()) {
                return Err(invalid(&format!("{key} must be text.")));
            }
        }
        for key in ["script_rules", "avoid"] {
            if let Some(v) = object.get(key)
                && !v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
            {
                return Err(invalid(&format!("{key} must be an array of text strings.")));
            }
        }
        if let Some(pacing) = object.get("pacing") {
            let p = pacing
                .as_object()
                .ok_or_else(|| invalid("pacing must be an object."))?;
            for (key, positive) in [("avg_shot_seconds", true), ("max_silence_seconds", false)] {
                if let Some(v) = p.get(key)
                    && !v.as_f64().is_some_and(|n| {
                        n.is_finite()
                            && if positive {
                                n > 0.0 && n <= f32::MAX as f64
                            } else {
                                n >= 0.0 && n <= f32::MAX as f64
                            }
                    })
                {
                    return Err(invalid(&format!(
                        "{key} must be a finite {} number.",
                        if positive { "positive" } else { "nonnegative" }
                    )));
                }
            }
            if let Some(v) = p.get("cut_style")
                && !v.as_str().is_some_and(|s| CUT_STYLES.contains(&s))
            {
                return Err(invalid(
                    "cut_style must be jump, invisible, rhythmic or mixed.",
                ));
            }
        }
        if let Some(weights) = object.get("weights") {
            let weights = weights
                .as_object()
                .ok_or_else(|| invalid("weights must be an object of tag: number pairs."))?;
            if weights.iter().any(|(tag, w)| {
                tag.trim().is_empty() || !w.as_f64().is_some_and(|n| (-1.0..=1.0).contains(&n))
            }) {
                return Err(invalid(
                    "Each weight needs a nonempty tag and a number between -1 and 1.",
                ));
            }
        }
        if let Some(references) = object.get("references") {
            let references = references
                .as_array()
                .ok_or_else(|| invalid("references must be an array."))?;
            for r in references {
                let url = r.get("url").and_then(Value::as_str).unwrap_or("");
                if !url.split_once(':').is_some_and(|(scheme, rest)| {
                    scheme
                        .bytes()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic())
                        && scheme
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
                        && !rest.is_empty()
                }) || url.chars().any(char::is_whitespace)
                    || r.get("note").is_some_and(|v| !v.is_string())
                {
                    return Err(invalid(
                        "Each reference needs an absolute URL and an optional text note.",
                    ));
                }
            }
        }
        Ok(Self { document })
    }

    pub fn name(&self) -> &str {
        self.document
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
    }
    pub fn title(&self) -> &str {
        self.document
            .get("title")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| self.name())
    }
    pub fn goal(&self) -> &str {
        self.document
            .get("goal")
            .and_then(Value::as_str)
            .unwrap_or("")
    }
    pub fn avg_shot_seconds(&self) -> f32 {
        self.document
            .pointer("/pacing/avg_shot_seconds")
            .and_then(Value::as_f64)
            .unwrap_or(2.0) as f32
    }
    pub fn cut_style(&self) -> &'static str {
        let style = self
            .document
            .pointer("/pacing/cut_style")
            .and_then(Value::as_str)
            .unwrap_or("mixed");
        CUT_STYLES
            .into_iter()
            .find(|s| *s == style)
            .unwrap_or("mixed")
    }
    pub fn weight_map(&self) -> HashMap<String, f32> {
        weights(&self.document)
    }
    pub fn to_source(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.document).map_err(|e| CoreError::Invalid(e.to_string()))
    }
}

pub fn weights(document: &Value) -> HashMap<String, f32> {
    document
        .get("weights")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(tag, value)| {
            value
                .as_f64()
                .filter(|n| (-1.0..=1.0).contains(n))
                .map(|n| (tag.clone(), n as f32))
        })
        .collect()
}

/// Refuse duplicates, including case variants, rather than overwrite someone's template.
pub fn add(config: &Path, source: &str) -> Result<Vision> {
    let vision = Vision::parse(source)?;
    let source = vision.to_source()?;
    if source.len() > MAX_BYTES {
        return Err(CoreError::Invalid(
            "The formatted vision exceeds 256 KiB. Shorten it before saving.".into(),
        ));
    }
    let directory = config.join("visions");
    fs::create_dir_all(&directory).map_err(|e| CoreError::io(&directory, e))?;
    let path = directory.join(format!("{}.json", vision.name()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                CoreError::Invalid(format!(
                    "A vision named {} already exists. Choose a different name.",
                    vision.name()
                ))
            } else {
                CoreError::io(&path, e)
            }
        })?;
    if let Err(e) = file
        .write_all(source.as_bytes())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(CoreError::io(path, e));
    }
    Ok(vision)
}

pub fn read(path: &Path) -> Result<Vision> {
    let mut source = String::new();
    fs::File::open(path)
        .map_err(|e| CoreError::io(path, e))?
        .take(MAX_BYTES as u64 + 1)
        .read_to_string(&mut source)
        .map_err(|e| CoreError::io(path, e))?;
    Vision::parse(&source)
}

pub fn load(config: &Path) -> (Vec<Vision>, Vec<String>) {
    let directory = config.join("visions");
    let files = match fs::read_dir(&directory) {
        Ok(files) => files,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Default::default(),
        Err(e) => return (Vec::new(), vec![format!("{}: {e}", directory.display())]),
    };
    let mut visions = Vec::new();
    let mut errors = Vec::new();
    for (index, file) in files.take(513).enumerate() {
        if index >= 512 {
            errors.push("Only the first 512 vision files are loaded.".into());
            break;
        }
        let file = match file {
            Ok(f) => f,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        let path = file.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match read(&path) {
            Ok(v) if visions.iter().any(|old: &Vision| old.name() == v.name()) => {
                errors.push(format!("Duplicate vision name in {}", path.display()))
            }
            Ok(v) => visions.push(v),
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }
    visions.sort_by(|a, b| a.title().cmp(b.title()));
    (visions, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = r#"{"name":"explainer","goal":"Explain clearly","pacing":{"avg_shot_seconds":3,"cut_style":"mixed"},"weights":{"diagram":0.9},"script_rules":["Explain one concept at a time"],"notes":"Keep this field"}"#;
    #[test]
    fn imports_persist_without_overwriting_and_survive_bad_neighbors() {
        let dir = crate::testutil::temp_dir("visions");
        let v = add(&dir, SOURCE).unwrap();
        assert_eq!(v.weight_map()["diagram"], 0.9);
        assert_eq!(v.document["notes"], "Keep this field");
        assert!(add(&dir, SOURCE).is_err());
        fs::write(dir.join("visions/bad.json"), "{").unwrap();
        let (loaded, errors) = load(&dir);
        assert_eq!(loaded, vec![v]);
        assert_eq!(errors.len(), 1);
    }
    #[test]
    fn rejects_hostile_and_wrongly_typed_inputs() {
        for bad in [
            "null",
            "[]",
            r#"{"name":"../../outside"}"#,
            r#"{"name":"ok","weights":{"x":2}}"#,
            r#"{"name":"ok","pacing":{"avg_shot_seconds":0}}"#,
            r#"{"name":"ok","pacing":{"max_silence_seconds":-1}}"#,
            r#"{"name":"ok","pacing":{"cut_style":"bad"}}"#,
            r#"{"name":"ok","script_rules":[1]}"#,
            r#"{"name":"ok","extends":"missing"}"#,
        ] {
            assert!(Vision::parse(bad).is_err(), "{bad}");
        }
        assert!(Vision::parse(&" ".repeat(MAX_BYTES + 1)).is_err());
    }
}
