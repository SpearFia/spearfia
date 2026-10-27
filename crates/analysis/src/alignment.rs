// SPDX-License-Identifier: MIT OR Apache-2.0
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spearfia_core::{
    CoreError,
    error::Result,
    links::{self, Links},
    project::{MediaRole, Project, Timebase},
};
use spearfia_script::{Block, format::line_words};
use std::{collections::BTreeMap, fs, io::Read, ops::Range, path::Path};
fn invalid(s: impl Into<String>) -> CoreError {
    CoreError::Invalid(s.into())
}
pub fn hash_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = f.read(&mut buffer).map_err(|e| invalid(e.to_string()))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("sha256:{:x}", hash.finalize()))
}
pub fn hash_text(text: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(text.as_bytes()))
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Word {
    pub line: usize,
    pub word: usize,
    pub text: String,
    pub start_frame: u64,
    pub end_frame: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub path: String,
    pub hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Analyzer {
    pub name: String,
    pub version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alignment {
    pub spec_version: u32,
    pub script_revision: String,
    pub source: Source,
    pub analyzer: Analyzer,
    pub timebase: Timebase,
    pub duration_frames: u64,
    pub words: Vec<Word>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct MeasuredWord {
    pub word: String,
    pub start: f64,
    pub end: f64,
}
#[derive(Debug, Clone)]
pub struct ScriptWord {
    pub line: usize,
    pub word: usize,
    pub text: String,
    pub bytes: Range<usize>,
}
pub fn script_words(source: &str) -> Vec<ScriptWord> {
    let script = spearfia_script::parse(source);
    let mut result = Vec::new();
    for (block_index, block) in script.blocks.iter().enumerate() {
        let Block::Line {
            kind,
            segments,
            line,
        } = block
        else {
            continue;
        };
        if !kind.is_spoken() {
            continue;
        }
        let ranges = spearfia_script::format::word_ranges(source, &script, block_index);
        for (index, (text, bytes)) in line_words(segments).into_iter().zip(ranges).enumerate() {
            if spearfia_script::is_word(text) {
                result.push(ScriptWord {
                    line: *line,
                    word: index,
                    text: text.into(),
                    bytes,
                });
            }
        }
    }
    result
}
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
pub fn frame(seconds: f64, tb: Timebase) -> Result<u64> {
    let f = seconds * f64::from(tb.num) / f64::from(tb.den);
    if tb.num == 0 || tb.den == 0 || !f.is_finite() || f < 0.0 || f > u64::MAX as f64 {
        return Err(invalid("Invalid measured word time/timebase."));
    }
    Ok(f.round() as u64)
}
impl Alignment {
    /// Match measured words to the approved text. Missing/reordered words are errors;
    /// no interpolation or invented word timestamps is used.
    pub fn measured(
        source: &str,
        path: String,
        hash: String,
        lines: &[usize],
        measured: &[MeasuredWord],
        duration: f64,
        tb: Timebase,
    ) -> Result<Self> {
        let expected: Vec<_> = script_words(source)
            .into_iter()
            .filter(|w| lines.contains(&w.line))
            .collect();
        let actual: Vec<_> = measured
            .iter()
            .filter(|w| !norm(&w.word).is_empty())
            .collect();
        if expected.is_empty() || actual.is_empty() {
            return Err(invalid(
                "Alignment needs spoken script words and measured word timestamps.",
            ));
        }
        let mut words = Vec::new();
        frame(duration, tb)?;
        let duration_frames = (duration * f64::from(tb.num) / f64::from(tb.den)).floor() as u64;
        let mut at = 0;
        let mut prior = 0.0;
        for word in expected {
            let wanted = norm(&word.text);
            let begin = at;
            let mut joined = String::new();
            while at < actual.len() && joined != wanted {
                let w = actual[at];
                if !w.start.is_finite()
                    || !w.end.is_finite()
                    || w.start < prior
                    || w.end <= w.start
                    || w.end > duration + 0.05
                {
                    return Err(invalid(
                        "Measured words have invalid, overlapping or out-of-range times.",
                    ));
                }
                joined.push_str(&norm(&w.word));
                prior = w.end;
                at += 1;
                if !wanted.starts_with(&joined) {
                    break;
                }
            }
            if joined != wanted {
                return Err(invalid(format!(
                    "Audio/script mismatch at line {}, word '{}'. Correct the narration or supply verified forced alignment; timing was not guessed.",
                    word.line + 1,
                    word.text
                )));
            }
            let start_frame = frame(actual[begin].start, tb)?;
            let end_frame = frame(actual[at - 1].end, tb)?.min(duration_frames);
            if end_frame <= start_frame {
                return Err(invalid("A measured word is shorter than the frame grid."));
            }
            words.push(Word {
                line: word.line,
                word: word.word,
                text: word.text,
                start_frame,
                end_frame,
            });
        }
        if at != actual.len() {
            return Err(invalid(
                "Audio has additional words not present in the approved narration.",
            ));
        }
        let alignment = Self {
            spec_version: 1,
            script_revision: spearfia_core::version(source),
            source: Source { path, hash },
            analyzer: Analyzer {
                name: "voicestudio-word-transcription".into(),
                version: "1".into(),
            },
            timebase: tb,
            duration_frames,
            words,
        };
        alignment.validate(source)?;
        Ok(alignment)
    }
    pub fn validate(&self, script: &str) -> Result<()> {
        if self.spec_version != 1
            || self.script_revision != spearfia_core::version(script)
            || self.timebase.num == 0
            || self.timebase.den == 0
            || self.words.is_empty()
            || self.words.len() > 100000
        {
            return Err(invalid("Alignment is stale, empty or unsupported."));
        }
        let expected: BTreeMap<_, _> = script_words(script)
            .into_iter()
            .map(|w| ((w.line, w.word), w.text))
            .collect();
        let mut prior = 0;
        let mut positions = std::collections::BTreeSet::new();
        for w in &self.words {
            if w.start_frame < prior
                || w.end_frame <= w.start_frame
                || w.end_frame > self.duration_frames
                || !positions.insert((w.line, w.word))
                || expected.get(&(w.line, w.word)) != Some(&w.text)
            {
                return Err(invalid(
                    "Alignment words do not match the saved narration or measured frame bounds.",
                ));
            }
            prior = w.end_frame;
        }
        Ok(())
    }
}
fn alignment_files(path: &Path, depth: usize, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    for item in fs::read_dir(path).map_err(|e| invalid(e.to_string()))? {
        let path = item.map_err(|e| invalid(e.to_string()))?.path();
        if path.is_dir() && depth < 3 {
            alignment_files(&path, depth + 1, out)?;
        } else if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(".alignment.json"))
        {
            out.push(path);
        }
        if out.len() > 5000 {
            return Err(invalid("Too many alignment sidecars."));
        }
    }
    Ok(())
}
pub fn current(project: &Project) -> Result<Vec<Alignment>> {
    let script = project.read_script()?;
    let revision = spearfia_core::version(&script);
    let mut files = Vec::new();
    alignment_files(&project.root.join("analysis"), 0, &mut files)?;
    files.sort();
    let mut result = Vec::new();
    for file in files {
        let text = spearfia_core::channels::read_limited(&file, 16 * 1024 * 1024)?;
        let alignment: Alignment =
            serde_json::from_str(&text).map_err(|e| invalid(format!("{}: {e}", file.display())))?;
        if alignment.script_revision != revision
            || alignment.timebase != project.file.settings.timebase
        {
            continue;
        }
        alignment.validate(&script)?;
        if hash_file(&project.resolve(&alignment.source.path))
            .ok()
            .as_deref()
            != Some(&alignment.source.hash)
        {
            continue;
        }
        result.push(alignment);
    }
    Ok(result)
}
pub fn timeline_words(project: &Project, links: &Links, tb: Timebase) -> Result<Vec<Word>> {
    if tb.num == 0 || tb.den == 0 {
        return Err(invalid("Timeline timebase must be positive."));
    }
    let source = project.read_script()?;
    let script = spearfia_script::parse(&source);
    let lines: Vec<_> = script
        .blocks
        .iter()
        .filter_map(|b| {
            if let Block::Line { line, segments, .. } = b {
                Some((*line, line_words(segments)))
            } else {
                None
            }
        })
        .collect();
    let alignments = current(project)?;
    let mut voice = Vec::new();
    for link in &links.links {
        if link.kind == links::LinkKind::Audio
            && !matches!(link.role, MediaRole::Music | MediaRole::Sfx)
        {
            let at = links::locate(&link.quote, link.line, &lines)
                .ok_or_else(|| invalid(format!("Narration link {} lost its words.", link.id)))?;
            voice.push(((at.line, at.first_word), link));
        }
    }
    voice.sort_by_key(|v| v.0);
    let mut start = 0;
    let mut words = BTreeMap::new();
    for (_, link) in voice {
        let media = project
            .resolve(&link.media.path)
            .canonicalize()
            .map_err(|e| invalid(e.to_string()))?;
        let matching: Vec<_> = alignments
            .iter()
            .filter(|a| {
                project.resolve(&a.source.path).canonicalize().ok().as_ref() == Some(&media)
            })
            .collect();
        let alignment = matching.first().ok_or_else(|| {
            invalid(format!(
                "Measured word alignment is missing/stale for {}.",
                link.media.path
            ))
        })?;
        if alignment.timebase != project.file.settings.timebase {
            return Err(invalid(
                "Narration frame rate changed; rerun measured alignment at the current project frame rate.",
            ));
        }
        if matching.iter().any(|other| {
            other.words != alignment.words
                || other.duration_frames != alignment.duration_frames
                || other.timebase != alignment.timebase
        }) {
            return Err(invalid(format!(
                "Conflicting alignment sidecars for {}.",
                link.media.path
            )));
        }
        let convert = |f: u64| -> u64 {
            ((f as u128 * tb.num as u128 * alignment.timebase.den as u128)
                / (alignment.timebase.num as u128 * tb.den as u128)) as u64
        };
        let point = |s: &Option<String>| -> Result<Option<u64>> {
            s.as_deref()
                .map(|s| {
                    let mut seconds = 0.0;
                    for p in s.split(':') {
                        seconds = seconds * 60.0
                            + p.parse::<f64>()
                                .map_err(|_| invalid("Invalid VO source point."))?;
                    }
                    frame(seconds, tb)
                })
                .transpose()
        };
        let input = point(&link.in_point)?.unwrap_or(0);
        let output = point(&link.out_point)?.unwrap_or(convert(alignment.duration_frames));
        if output <= input || output > convert(alignment.duration_frames) {
            return Err(invalid("VO trim exceeds aligned source bounds."));
        }
        for word in &alignment.words {
            let a = convert(word.start_frame);
            let b = convert(word.end_frame);
            if a < input || b > output {
                continue;
            }
            let mut w = word.clone();
            w.start_frame = start + a - input;
            w.end_frame = start + b - input;
            if let Some(old) = words.insert((w.line, w.word), w.clone())
                && old != w
            {
                return Err(invalid(
                    "Conflicting measured alignments cover the same narration.",
                ));
            }
        }
        start += output - input;
    }
    Ok(words.into_values().collect())
}
pub fn range_frames(source: &str, range: Range<usize>, words: &[Word]) -> Result<(u64, u64)> {
    let selected: std::collections::BTreeSet<_> = script_words(source)
        .iter()
        .filter(|w| w.bytes.start < range.end && w.bytes.end > range.start)
        .map(|w| (w.line, w.word))
        .collect();
    let matched: Vec<_> = words
        .iter()
        .filter(|w| selected.contains(&(w.line, w.word)))
        .collect();
    if matched.len() != selected.len() || matched.is_empty() {
        return Err(invalid(
            "The visual treatment has narration without measured word timing.",
        ));
    }
    Ok((
        matched.iter().map(|w| w.start_frame).min().unwrap(),
        matched.iter().map(|w| w.end_frame).max().unwrap(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn measured(words: &[(&str, f64, f64)]) -> Vec<MeasuredWord> {
        words
            .iter()
            .map(|(w, s, e)| MeasuredWord {
                word: (*w).into(),
                start: *s,
                end: *e,
            })
            .collect()
    }
    fn align(source: &str, words: &[MeasuredWord]) -> Result<Alignment> {
        Alignment::measured(
            source,
            "voice.wav".into(),
            hash_text("fixture"),
            &[1],
            words,
            2.0,
            Timebase {
                num: 30000,
                den: 1001,
            },
        )
    }
    #[test]
    fn exact_measured_words_support_unicode_markup_and_hyphens() {
        let source = "## Test\nVO: Café [co-op]{color=red}, again.\n";
        let words = measured(&[
            ("Café", 0.1, 0.3),
            ("co", 0.4, 0.5),
            ("op", 0.5, 0.7),
            ("again.", 1.1, 1.4),
        ]);
        let result = align(source, &words).unwrap();
        assert_eq!(result.words.len(), 3);
        assert_eq!(result.words[1].text, "co-op,");
        assert_eq!(result.words[2].start_frame, 33);
        let start = source.find("co-op").unwrap();
        assert_eq!(
            range_frames(source, start..start + 5, &result.words).unwrap(),
            (12, 21)
        );
    }
    #[test]
    fn missing_reordered_extra_and_overlapping_words_fail() {
        let source = "## Test\nOne two three.\n";
        for words in [
            measured(&[("One", 0.1, 0.2), ("three", 0.3, 0.4)]),
            measured(&[("two", 0.1, 0.2), ("One", 0.3, 0.4), ("three", 0.5, 0.6)]),
            measured(&[
                ("One", 0.1, 0.2),
                ("two", 0.3, 0.4),
                ("three", 0.5, 0.6),
                ("extra", 0.7, 0.8),
            ]),
            measured(&[("One", 0.1, 0.4), ("two", 0.3, 0.5), ("three", 0.6, 0.7)]),
        ] {
            assert!(align(source, &words).is_err());
        }
    }
    #[test]
    fn stale_and_out_of_range_sidecars_fail() {
        let source = "## Test\nOne two.\n";
        let mut result = align(source, &measured(&[("One", 0.1, 0.2), ("two", 0.4, 0.6)])).unwrap();
        assert!(result.validate("## Test\nOne changed.\n").is_err());
        result.words[1].end_frame = 10000;
        assert!(result.validate(source).is_err());
    }
}
