//! Measured narration timing: real voiceover lengths from linked audio files, so timelines
//! stop guessing at 150 words per minute once VO is recorded.
//!
//! A voiceover file covers the spoken lines from the first line it's linked to up to the
//! next voiceover file or the end of its `##` section, whichever comes first. Its length is
//! spread over those lines by word count.

use spearfia_core::links::{self, Links};
use spearfia_core::project::{MediaRole, Project};
use spearfia_script::{Block, Script, format::line_words};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Audio lengths, cached by file size and modification time.
#[derive(Default)]
pub struct Probe {
    cache: HashMap<PathBuf, (u64, Option<SystemTime>, Option<f32>)>,
}

impl Probe {
    pub fn seconds(&mut self, path: &Path) -> Option<f32> {
        let meta = std::fs::metadata(path).ok()?;
        let key = (meta.len(), meta.modified().ok());
        if let Some((len, modified, secs)) = self.cache.get(path)
            && (*len, *modified) == key
        {
            return *secs;
        }
        let secs = audio_seconds(path);
        self.cache.insert(path.to_owned(), (key.0, key.1, secs));
        secs
    }
}

/// Length of an audio file: read from a WAV header, otherwise asked of `ffprobe` (if it's
/// installed). `None` when neither works.
pub fn audio_seconds(path: &Path) -> Option<f32> {
    audio_seconds_precise(path).map(|s| s as f32)
}
/// Preserve source duration precision for rational-rate frame placement.
pub fn audio_seconds_precise(path: &Path) -> Option<f64> {
    wav_seconds(path).or_else(|| ffprobe_seconds(path))
}

fn wav_seconds(path: &Path) -> Option<f64> {
    use std::io::Read;
    let mut head = vec![0u8; 64 * 1024];
    let n = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    let head = &head[..n];
    if head.get(0..4)? != b"RIFF" || head.get(8..12)? != b"WAVE" {
        return None;
    }
    let u32_at = |i: usize| Some(u32::from_le_bytes(head.get(i..i + 4)?.try_into().ok()?));
    let (mut at, mut byte_rate) = (12usize, None);
    while at + 8 <= head.len() {
        let size = u32_at(at + 4)? as usize;
        match head.get(at..at + 4)? {
            b"fmt " => byte_rate = u32_at(at + 16),
            b"data" => {
                // A streamed WAV may leave the data size at 0 or u32::MAX; use the file size.
                let file = std::fs::metadata(path).ok()?.len() as usize;
                let data = if size == 0 || size == u32::MAX as usize {
                    file.saturating_sub(at + 8)
                } else {
                    size
                };
                return byte_rate.filter(|r| *r > 0).map(|r| data as f64 / r as f64);
            }
            _ => {}
        }
        at = at.checked_add(8 + size + (size & 1))?;
    }
    None
}

fn ffprobe_seconds(path: &Path) -> Option<f64> {
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .ok()?;
    String::from_utf8(out.stdout).ok()?.trim().parse().ok()
}

/// Seconds of recorded voiceover per script line (zero-based line number), for the lines
/// covered by voiceover-role links whose files can be measured.
// shortcut: in/out points on VO links are ignored (the whole file counts); honour them if
// VO files ever get trimmed in place.
pub fn vo_seconds(
    project: &Project,
    script: &Script,
    links: &Links,
    probe: &mut Probe,
) -> HashMap<usize, f32> {
    // Spoken lines in order, with their words and section number.
    let mut spoken: Vec<(usize, usize, usize)> = Vec::new(); // (line, words, section)
    let mut located: Vec<(usize, Vec<&str>)> = Vec::new();
    let mut section = 0;
    for b in &script.blocks {
        match b {
            Block::Heading { level, .. } if *level >= 2 => section += 1,
            Block::Line {
                kind,
                segments,
                line,
            } => {
                let words = line_words(segments);
                if kind.is_spoken() {
                    spoken.push((*line, words.len(), section));
                }
                located.push((*line, words));
            }
            _ => {}
        }
    }

    // Each VO file starts at its earliest linked line.
    let mut starts: BTreeMap<usize, &str> = BTreeMap::new();
    let mut first: HashMap<&str, usize> = HashMap::new();
    for l in &links.links {
        if l.role != MediaRole::Voiceover {
            continue;
        }
        let Some(at) = links::locate(&l.quote, l.line, &located) else {
            continue;
        };
        let line = located[at.line].0;
        let entry = first.entry(l.media.path.as_str()).or_insert(line);
        *entry = (*entry).min(line);
    }
    for (path, line) in first {
        starts.insert(line, path);
    }

    let mut out = HashMap::new();
    let starts: Vec<(usize, &str)> = starts.into_iter().collect();
    for (k, &(start, path)) in starts.iter().enumerate() {
        let Some(total) = probe.seconds(&project.resolve(path)) else {
            continue;
        };
        let next = starts.get(k + 1).map_or(usize::MAX, |s| s.0);
        let Some(&(_, _, sec)) = spoken.iter().find(|s| s.0 >= start) else {
            continue;
        };
        let covered: Vec<_> = spoken
            .iter()
            .filter(|s| s.0 >= start && s.0 < next && s.2 == sec && s.1 > 0)
            .collect();
        let words: usize = covered.iter().map(|s| s.1).sum();
        for s in covered {
            out.insert(s.0, total * s.1 as f32 / words as f32);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::project;
    use spearfia_core::links::{Link, LinkKind, LinkMedia};

    /// A silent 16-bit mono WAV of `seconds` at 8 kHz.
    fn wav(path: &Path, seconds: u32) {
        let data = 8000 * 2 * seconds;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes()); // PCM
        b.extend_from_slice(&1u16.to_le_bytes()); // mono
        b.extend_from_slice(&8000u32.to_le_bytes());
        b.extend_from_slice(&16000u32.to_le_bytes()); // byte rate
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data.to_le_bytes());
        b.resize(b.len() + data as usize, 0);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b).unwrap();
    }

    fn vo(quote: &str, line: usize, path: &str) -> Link {
        Link {
            id: String::new(),
            quote: quote.into(),
            line,
            media: LinkMedia {
                path: path.into(),
                hash: None,
            },
            role: MediaRole::Voiceover,
            kind: LinkKind::Audio,
            in_point: None,
            out_point: None,
        }
    }

    #[test]
    fn vo_files_time_their_lines_by_words_within_their_section() {
        let script =
            "## One\nVO: one two three.\nVO: four.\n## Two\nVO: five six.\nVO: seven eight.\n";
        let p = project("timing", script);
        wav(&p.root.join("VO/01.wav"), 8);
        wav(&p.root.join("VO/02.wav"), 3);
        let mut links = Links::default();
        links.add(vo("one two three.", 1, "VO/01.wav"));
        links.add(vo("seven eight.", 5, "VO/02.wav"));
        let parsed = spearfia_script::parse(script);
        let secs = vo_seconds(&p, &parsed, &links, &mut Probe::default());
        // 8 s over 3 + 1 words; section Two's first line has no file and stays estimated.
        assert_eq!(secs.get(&1), Some(&6.0));
        assert_eq!(secs.get(&2), Some(&2.0));
        assert_eq!(secs.get(&4), None);
        assert_eq!(secs.get(&5), Some(&3.0));
        let tl = spearfia_script::timing::timeline_measured(&parsed, &secs);
        let estimate = spearfia_script::estimate_seconds(2);
        assert!(
            (tl.total - (8.0 + estimate + 3.0)).abs() < 1e-3,
            "{}",
            tl.total
        );
        assert_eq!(audio_seconds(&p.root.join("VO/02.wav")), Some(3.0));
        assert_eq!(audio_seconds(&p.root.join("script/script.md")), None);
    }
}
