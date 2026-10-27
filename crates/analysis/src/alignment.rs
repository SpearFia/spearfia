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
    /// Placed between measured neighbours because transcription misheard it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub estimated: bool,
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
/// Longest run of consecutive script words that may be misheard and still be placed.
const MAX_GAP_WORDS: usize = 8;
/// Most script words (percent) that may be placed rather than measured.
const MAX_ESTIMATED_PERCENT: usize = 20;
/// Most heard tokens (percent) that may be filler not in the script.
const MAX_EXTRA_PERCENT: usize = 15;
/// How far ahead to look for the next agreement after a mismatch.
const RESYNC_WINDOW: usize = 12;

/// For each script word, the heard tokens that spell it (`None` when misheard). Up to
/// three heard tokens may spell one word ("co" "op" / "co-op"), and two script words may
/// share one token ("Iron Man" / "ironman"). After a mismatch, the closest point where the
/// next two words agree again resumes the matching. `Err(i)` when script word `i` starts
/// a misheard run longer than [`MAX_GAP_WORDS`].
fn anchor(
    want: &[String],
    heard: &[String],
) -> std::result::Result<Vec<Option<Range<usize>>>, usize> {
    // Tokens consumed if script word `i` is spelled from token `j` (0 = no match).
    let spell = |i: usize, j: usize| -> usize {
        let mut joined = String::new();
        for (k, token) in heard.iter().enumerate().skip(j).take(3) {
            joined.push_str(token);
            if joined == want[i] {
                return k + 1 - j;
            }
            if !want[i].starts_with(&joined) {
                break;
            }
        }
        0
    };
    let pair = |i: usize, j: usize| {
        i + 1 < want.len() && j < heard.len() && format!("{}{}", want[i], want[i + 1]) == heard[j]
    };
    // Do the next `n` script words from `i` agree with the tokens from `j`?
    let agrees = |mut i: usize, mut j: usize, n: usize| {
        for _ in 0..n {
            if i >= want.len() {
                return true;
            }
            if pair(i, j) {
                i += 2;
                j += 1;
                continue;
            }
            match spell(i, j) {
                0 => return false,
                k => {
                    i += 1;
                    j += k;
                }
            }
        }
        true
    };
    let mut out: Vec<Option<Range<usize>>> = vec![None; want.len()];
    let (mut i, mut j) = (0, 0);
    while i < want.len() {
        if pair(i, j) {
            out[i] = Some(j..j + 1);
            out[i + 1] = Some(j..j + 1);
            i += 2;
            j += 1;
            continue;
        }
        let k = spell(i, j);
        if k > 0 {
            out[i] = Some(j..j + k);
            i += 1;
            j += k;
            continue;
        }
        // Resync: skip the fewest script words and heard tokens that make the next words
        // (two, or what's left) agree again.
        let need = 2.min(want.len() - i);
        let next = (1..=2 * RESYNC_WINDOW)
            .flat_map(|cost| (0..=cost.min(RESYNC_WINDOW)).map(move |di| (di, cost - di)))
            .filter(|&(_, dj)| dj <= RESYNC_WINDOW)
            .find(|&(di, dj)| {
                i + di < want.len() && j + dj < heard.len() && agrees(i + di, j + dj, need)
            });
        match next {
            Some((di, dj)) if di <= MAX_GAP_WORDS => {
                i += di;
                j += dj;
            }
            // Nothing agrees again: the rest of the script was not heard.
            None if want.len() - i <= MAX_GAP_WORDS => break,
            _ => return Err(i),
        }
    }
    Ok(out)
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
    /// Match measured words to the approved text. Transcription is only a clock: words it
    /// hears exactly (including split or joined tokens) anchor the timing, filler it adds
    /// is ignored, and short misheard runs (names, numbers) are placed between their
    /// measured neighbours and flagged `estimated`. A take that diverges more than that
    /// fails, so the creator corrects it instead of getting invented timing.
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
        frame(duration, tb)?;
        let duration_frames = (duration * f64::from(tb.num) / f64::from(tb.den)).floor() as u64;
        let mut prior = 0.0;
        for w in &actual {
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
            prior = w.end;
        }
        let want: Vec<String> = expected.iter().map(|w| norm(&w.text)).collect();
        let heard: Vec<String> = actual.iter().map(|w| norm(&w.word)).collect();
        let spans = anchor(&want, &heard).map_err(|i| {
            let w = &expected[i.min(expected.len() - 1)];
            invalid(format!(
                "Audio/script mismatch near line {}, word '{}': more than {MAX_GAP_WORDS} words in a row don't match. Correct the narration or the take; timing was not guessed.",
                w.line + 1,
                w.text
            ))
        })?;
        let estimated = spans.iter().filter(|s| s.is_none()).count();
        if estimated * 100 > expected.len() * MAX_ESTIMATED_PERCENT {
            return Err(invalid(format!(
                "Only {} of {} words were heard clearly enough to time. Check the take and the transcription language.",
                expected.len() - estimated,
                expected.len()
            )));
        }
        let used: usize = spans.iter().flatten().map(|r| r.len()).sum();
        let ignored = heard.len().saturating_sub(used);
        if ignored * 100 > heard.len() * MAX_EXTRA_PERCENT && ignored > MAX_GAP_WORDS {
            return Err(invalid(format!(
                "The audio has {ignored} words that aren't in the approved narration. Check that this take belongs to these lines."
            )));
        }
        // Seconds per script word: measured, or shared out between measured neighbours.
        let mut times: Vec<Option<(f64, f64)>> = spans
            .iter()
            .map(|s| {
                s.as_ref()
                    .map(|r| (actual[r.start].start, actual[r.end - 1].end))
            })
            .collect();
        // Two script words heard as one token ("Iron Man" / "ironman") share its time.
        for i in 1..spans.len() {
            if let (Some(a), Some(b)) = (&spans[i - 1], &spans[i])
                && a == b
                && let Some((s0, e0)) = times[i - 1]
            {
                let split = s0
                    + (e0 - s0) * want[i - 1].len() as f64
                        / (want[i - 1].len() + want[i].len()).max(1) as f64;
                times[i - 1] = Some((s0, split));
                times[i] = Some((split, e0));
            }
        }
        let mut i = 0;
        while i < times.len() {
            if times[i].is_some() {
                i += 1;
                continue;
            }
            let run_end = (i..times.len())
                .find(|&k| times[k].is_some())
                .unwrap_or(times.len());
            let from = if i == 0 {
                0.0
            } else {
                times[i - 1].map_or(0.0, |t| t.1)
            };
            let to = times
                .get(run_end)
                .copied()
                .flatten()
                .map_or(actual.last().map_or(duration, |w| w.end), |t| t.0);
            let chars: usize = want[i..run_end].iter().map(|w| w.len().max(1)).sum();
            let mut at = from;
            for k in i..run_end {
                let share = (to - from) * want[k].len().max(1) as f64 / chars as f64;
                times[k] = Some((at, at + share));
                at += share;
            }
            i = run_end;
        }
        let mut words = Vec::new();
        for (k, word) in expected.into_iter().enumerate() {
            let (start, end) = times[k].expect("every word is timed");
            let start_frame = frame(start, tb)?;
            let end_frame = frame(end, tb)?.min(duration_frames);
            if end_frame <= start_frame {
                return Err(invalid(format!(
                    "Line {}, word '{}' is too short to place on the frame grid. Check the take around it.",
                    word.line + 1,
                    word.text
                )));
            }
            words.push(Word {
                line: word.line,
                word: word.word,
                text: word.text,
                start_frame,
                end_frame,
                estimated: spans[k].is_none(),
            });
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
    fn badly_matching_takes_and_overlapping_times_fail() {
        let source = "## Test\nOne two three.\n";
        for words in [
            // A third of the words unheard is too much to place.
            measured(&[("One", 0.1, 0.2), ("three", 0.3, 0.4)]),
            measured(&[("two", 0.1, 0.2), ("One", 0.3, 0.4), ("three", 0.5, 0.6)]),
            measured(&[("One", 0.1, 0.4), ("two", 0.3, 0.5), ("three", 0.6, 0.7)]),
        ] {
            assert!(align(source, &words).is_err());
        }
    }
    fn long(source_words: &str, heard: &[&str]) -> Result<Alignment> {
        let source = format!("## Test\nVO: {source_words}\n");
        let step = 1.8 / heard.len() as f64;
        let words: Vec<_> = heard
            .iter()
            .enumerate()
            .map(|(i, w)| MeasuredWord {
                word: (*w).into(),
                start: i as f64 * step,
                end: (i as f64 + 0.9) * step,
            })
            .collect();
        align(&source, &words)
    }
    #[test]
    fn misheard_names_are_placed_between_measured_neighbours() {
        let script =
            "Could Thor lift Mjolnir after the snap if Hermione helped him with one spell tonight?";
        let heard = [
            "could", "thor", "lift", "mule", "near", "after", "the", "snap", "if", "her", "my",
            "oni", "helped", "him", "with", "one", "spell", "tonight",
        ];
        let a = long(script, &heard).unwrap();
        assert_eq!(a.words.len(), 15);
        let estimated: Vec<_> = a
            .words
            .iter()
            .filter(|w| w.estimated)
            .map(|w| w.text.as_str())
            .collect();
        assert_eq!(estimated, ["Mjolnir", "Hermione"]);
        // Placed inside the gap its neighbours leave, in order.
        let (lift, mj, after) = (&a.words[2], &a.words[3], &a.words[4]);
        assert!(lift.end_frame <= mj.start_frame && mj.end_frame <= after.start_frame);
        // Measured sidecars don't carry the flag.
        let json = serde_json::to_string(&a.words[0]).unwrap();
        assert!(!json.contains("estimated"), "{json}");
    }
    #[test]
    fn joined_split_and_filler_tokens_still_match() {
        let script = "Iron Man and Spider-Man could win this fight in about ten seconds flat.";
        let heard = [
            "ironman", "and", "um", "spider", "man", "could", "win", "this", "fight", "in",
            "about", "ten", "seconds", "flat",
        ];
        let a = long(script, &heard).unwrap();
        assert!(a.words.iter().all(|w| !w.estimated), "{:?}", a.words);
        // "Iron" and "Man" share the one heard token, in order.
        assert!(a.words[0].end_frame <= a.words[1].start_frame);
    }
    #[test]
    fn a_long_unheard_stretch_fails_instead_of_being_guessed() {
        let script =
            "One two three four five six seven eight nine ten eleven twelve thirteen fourteen.";
        let heard = [
            "one", "two", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "x", "fourteen",
        ];
        let e = long(script, &heard).unwrap_err().to_string();
        assert!(e.contains("mismatch") || e.contains("heard clearly"), "{e}");
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
