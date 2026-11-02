// SPDX-License-Identifier: MIT OR Apache-2.0
//! Read-only bridge for the experimental editing worker, using the production aligner.
use serde_json::{Value, json};
use spearfia_core::{links::Links, project::Project};
use spearfia_sync::{Heard, Holds, Timing};
use std::{collections::BTreeMap, error::Error, fs};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: editing_words <approved project> <voice media path>".into());
    }
    let project = Project::open(&args[0])?;
    let script = spearfia_script::parse(&project.approved_script()?);
    let links = Links::load(&project)?;
    let tb = project.file.settings.timebase;
    let cache: Value = serde_json::from_str(&fs::read_to_string(
        project.root.join("media/vo-words.json"),
    )?)?;
    let voices: Value =
        serde_json::from_str(&fs::read_to_string(project.root.join("media/vo.json"))?)?;
    let mut media = BTreeMap::new();
    for voice in voices
        .as_object()
        .ok_or("invalid voice duration cache")?
        .values()
    {
        let file = voice["file"].as_str().ok_or("missing voice file")?;
        let duration = voice["duration"]
            .as_f64()
            .filter(|v| v.is_finite() && *v > 0.0)
            .ok_or("invalid voice duration")?;
        media.insert(file.to_string(), Some((duration * tb.fps()).round() as u64));
    }
    let mut heard = Heard::new();
    for (path, value) in cache.as_object().ok_or("invalid voice word cache")? {
        let words = value["words"]
            .as_array()
            .ok_or("missing word observations")?
            .iter()
            .map(|word| {
                Ok((
                    word[0].as_str().ok_or("missing observed word")?.to_string(),
                    word[1]
                        .as_f64()
                        .filter(|v| v.is_finite() && *v >= 0.0)
                        .ok_or("invalid observed time")?,
                ))
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        heard.insert(path.clone(), words);
    }
    if !media.contains_key(&args[1]) || !heard.contains_key(&args[1]) {
        return Err("selected voice lacks cached timing".into());
    }
    let timing = Timing::new(&script, &links, &media, tb, &heard, &Holds::default());
    let lines: Vec<_> = script
        .blocks
        .iter()
        .flat_map(|block| match block {
            spearfia_script::Block::Line {
                kind,
                segments,
                line,
            } if kind.is_spoken() => {
                vec![*line; spearfia_script::format::line_words(segments).len()]
            }
            _ => vec![],
        })
        .collect();
    let words: Vec<_> = timing.words().iter().enumerate().filter(|(_, w)| w.path == args[1]).map(|(i,w)| {
        json!({"id": i, "line": lines[i], "text": w.text, "startFrame": w.start as i64-w.origin, "endFrame": w.end as i64-w.origin})
    }).collect();
    if words.is_empty() {
        return Err("selected voice has no resolved script words".into());
    }
    println!(
        "{}",
        json!({"experimental": true, "method": "spearfia-sync::Timing", "fps": tb.fps(), "voice": args[1], "words": words})
    );
    Ok(())
}
