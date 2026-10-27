//! Applies resolved edit operations, captions, mix and filmstrips in the push's FilmCraft
//! session, so an agent never edits clip by clip.

use crate::mcp::Client;
use crate::push::ticks;
use serde_json::json;
use spearfia_core::project::{Project, Timebase};
use spearfia_sync::edit::{Action, Op, Style};
use spearfia_sync::{Item, Kind, TimelinePlan, TimedWord};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// What applying the edits did.
#[derive(Debug, Default)]
pub struct Applied {
    pub ops: usize,
    pub captions: usize,
    pub problems: Vec<String>,
}

/// The placed picture clip covering `frame` on V2, or `None`.
fn picture_at<'p>(plan: &'p TimelinePlan, frame: u64) -> Option<&'p Item> {
    plan.items.iter().find(|i| {
        i.kind == Kind::Clip && i.track == "V2" && i.start_frame <= frame && frame < i.start_frame + i.duration_frames
    })
}

/// The first placed picture clip starting at or after `frame` on V2.
fn picture_from<'p>(plan: &'p TimelinePlan, frame: u64) -> Option<&'p Item> {
    plan.items
        .iter()
        .filter(|i| i.kind == Kind::Clip && i.track == "V2" && i.start_frame >= frame)
        .min_by_key(|i| i.start_frame)
}

#[allow(clippy::too_many_arguments)]
pub fn apply(
    fc: &mut Client,
    project: &Project,
    plan: &TimelinePlan,
    clips: &BTreeMap<&str, u64>,
    style: &Style,
    ops: &[Op],
    tb: Timebase,
    size: [u32; 2],
) -> Applied {
    let mut out = Applied::default();
    let mut sfx_items: BTreeMap<String, u64> = BTreeMap::new();
    for (n, op) in ops.iter().enumerate() {
        // A card ends before the next one starts: they share V3.
        let next_card = ops[n + 1..]
            .iter()
            .find(|o| matches!(o.action, Action::Card { .. }) && o.frame > op.frame)
            .map(|o| o.frame);
        let r = match &op.action {
            Action::Zoom(name) => zoom(fc, plan, clips, style, op.frame, name, tb),
            Action::Transition { effect, seconds } => match picture_from(plan, op.frame).and_then(|i| clips.get(i.cue_id.as_str())) {
                Some(&clip) => fc
                    .command(
                        "sequence.applyVideoTransition",
                        json!({"clip": clip, "edge": "in", "effect": effect, "frames": (seconds * f64::from(tb.num) / f64::from(tb.den)).round() as u64}),
                    )
                    .map(drop)
                    .map_err(|e| e.to_string()),
                None => Err("no picture clip after it".into()),
            },
            Action::Card { text, seconds } => {
                let fps = f64::from(tb.num) / f64::from(tb.den);
                let room = next_card.map_or(f64::MAX, |f| (f - op.frame) as f64 / fps - 0.1);
                let secs = seconds.unwrap_or(style.edit.card.seconds).min(room);
                card(fc, style, text.as_deref().unwrap_or_default(), secs, op.frame, tb, size)
            }
            Action::Sfx { name, db } => sfx(fc, project, &mut sfx_items, name, db.unwrap_or(style.mix.sfx_db), op.frame),
            Action::Marker(text) => fc
                .command("markers.add", json!({"target": "program", "time": ticks(op.frame, tb), "name": text}))
                .map(drop)
                .map_err(|e| e.to_string()),
        };
        match r {
            Ok(()) => out.ops += 1,
            Err(e) => out.problems.push(format!("{} at {}: {e}", op.from, clock(op.frame, tb))),
        }
    }
    out
}

fn zoom(
    fc: &mut Client,
    plan: &TimelinePlan,
    clips: &BTreeMap<&str, u64>,
    style: &Style,
    frame: u64,
    name: &str,
    tb: Timebase,
) -> Result<(), String> {
    let recipe = style
        .recipes
        .iter()
        .find(|r| r.id.contains(&format!("/{name}@")) || r.id == name)
        .ok_or(format!("no recipe `{name}` in execution.md"))?;
    let (from, to) = recipe.scale().ok_or(format!("recipe `{name}` has no scale from/to"))?;
    let item = picture_at(plan, frame).ok_or("no picture clip there")?;
    let clip = *clips.get(item.cue_id.as_str()).ok_or("picture clip not placed")?;
    for (value, at) in [(from, item.start_frame), (to, item.start_frame + item.duration_frames.saturating_sub(1))] {
        fc.command(
            "effects.setParam",
            json!({"clip": clip, "effect": "motion", "param": "scale", "value": value, "time": ticks(at, tb), "keyframe": true}),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn card(fc: &mut Client, style: &Style, text: &str, seconds: f64, frame: u64, tb: Timebase, size: [u32; 2]) -> Result<(), String> {
    let c = &style.edit.card;
    let r = fc
        .command(
            "graphics.newText",
            json!({"text": text, "size": c.size, "font": c.font, "seconds": seconds,
                   "track": 2, "time": ticks(frame, tb)}),
        )
        .map_err(|e| e.to_string())?;
    let clip = r["clip"].as_u64().ok_or("FilmCraft made no text clip")?;
    fc.command(
        "graphics.set",
        json!({"clip": clip, "props": {"fill_color": c.color, "stroke": c.outline > 0.0, "stroke_width": c.outline,
                                        "align": "center", "position": [size[0] / 2, size[1] / 2]}}),
    )
    .map(drop)
    .map_err(|e| e.to_string())
}

fn sfx(
    fc: &mut Client,
    project: &Project,
    imported: &mut BTreeMap<String, u64>,
    name: &str,
    db: f64,
    frame: u64,
) -> Result<(), String> {
    let item = match imported.get(name) {
        Some(&i) => i,
        None => {
            let file = ["wav", "mp3", "ogg", "flac", "m4a"]
                .iter()
                .map(|ext| project.root.join("SFX").join(format!("{name}.{ext}")))
                .find(|p| p.is_file())
                .ok_or(format!("no SFX/{name}.(wav|mp3|ogg|flac|m4a) in the project"))?;
            let abs = std::path::absolute(&file).map_err(|e| e.to_string())?;
            let r = fc
                .tool("media_import", json!({"paths": [abs.to_string_lossy()]}))
                .map_err(|e| e.to_string())?;
            let i = r["items"][0].as_u64().ok_or("FilmCraft could not import it")?;
            imported.insert(name.to_string(), i);
            i
        }
    };
    let placed = fc
        .command("timeline.place", json!({"item": item, "track": "A3", "frame": frame}))
        .map_err(|e| e.to_string())?;
    if db != 0.0 {
        fc.command("clip.audioGain", json!({"clips": placed["clips"], "mode": "set", "db": db}))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Captions from the script's own words at their heard times: each voiceover item gets a
/// transcript of script words (correct spelling, no guessing), then FilmCraft makes and styles
/// the caption track.
pub fn captions(
    fc: &mut Client,
    items: &BTreeMap<String, u64>,
    words: &[TimedWord],
    style: &Style,
    tb: Timebase,
) -> Result<usize, String> {
    let c = &style.captions;
    let mut by_file: BTreeMap<&str, Vec<&TimedWord>> = BTreeMap::new();
    for w in words {
        by_file.entry(&w.path).or_default().push(w);
    }
    for (path, ws) in by_file {
        let Some(&item) = items.get(path) else { continue };
        let file_ticks = |f: u64| ticks((f as i64 - ws[0].origin).max(0) as u64, tb);
        let list: Vec<_> = ws
            .iter()
            .map(|w| json!({"text": w.text, "start": file_ticks(w.start), "end": file_ticks(w.end)}))
            .collect();
        fc.command(
            "transcript.set",
            json!({"item": item, "transcript": {"language": "en", "speakers": [{"name": "Narrator"}], "words": list}}),
        )
        .map_err(|e| e.to_string())?;
    }
    let r = fc
        .command("transcript.createCaptions", json!({"maxChars": c.max_chars, "lines": 1}))
        .map_err(|e| e.to_string())?;
    fc.command(
        "captions.setStyle",
        json!({"track": "C1", "font": c.font, "size": c.size, "color": c.color, "outline": c.outline,
               "outlineColor": "#000000", "anchor": "bottom", "margin": c.bottom_margin()}),
    )
    .map_err(|e| e.to_string())?;
    Ok(r["captions"].as_u64().unwrap_or(0) as usize)
}

/// Music (A2) and SFX links (A3) at the channel's mix levels.
pub fn mix(fc: &mut Client, plan: &TimelinePlan, clips: &BTreeMap<&str, u64>, style: &Style) -> Vec<String> {
    let mut problems = Vec::new();
    for (track, db) in [("A2", style.mix.music_db), ("A3", style.mix.sfx_db)] {
        let ids: Vec<u64> = plan
            .items
            .iter()
            .filter(|i| i.kind == Kind::Clip && i.track == track)
            .filter_map(|i| clips.get(i.cue_id.as_str()).copied())
            .collect();
        if ids.is_empty() || db == 0.0 {
            continue;
        }
        if let Err(e) = fc.command("clip.audioGain", json!({"clips": ids, "mode": "set", "db": db})) {
            problems.push(format!("mix {track}: {e}"));
        }
    }
    problems
}

/// One contact sheet per section: `frames` evenly spaced renders tiled 3 wide with ffmpeg
/// (when it's on PATH; the PNGs stay either way). Returns the sheets made.
pub fn filmstrips(fc: &mut Client, dir: &std::path::Path, sections: &[(String, u64, u64)], frames: u64, tb: Timebase) -> (Vec<PathBuf>, Vec<String>) {
    let (mut sheets, mut problems) = (Vec::new(), Vec::new());
    let secs = |f: u64| f as f64 * f64::from(tb.den) / f64::from(tb.num);
    for (n, (title, start, end)) in sections.iter().enumerate() {
        let name = format!("{:02}-{}", n + 1, slug(title));
        let frames_dir = dir.join(&name);
        if let Err(e) = std::fs::create_dir_all(&frames_dir) {
            problems.push(format!("filmstrip {name}: {e}"));
            continue;
        }
        for k in 0..frames {
            let at = start + (end.saturating_sub(*start)) * (2 * k + 1) / (2 * frames);
            match fc.frame_png(secs(at), 480) {
                Ok(png) => {
                    let _ = std::fs::write(frames_dir.join(format!("{k:02}.png")), png);
                }
                Err(e) => problems.push(format!("filmstrip {name}: {e}")),
            }
        }
        let sheet = dir.join(format!("{name}.jpg"));
        let tiled = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-framerate", "1", "-i"])
            .arg(frames_dir.join("%02d.png"))
            .args(["-frames:v", "1", "-vf", &format!("tile=3x{}", frames.div_ceil(3))])
            .arg(&sheet)
            .status()
            .is_ok_and(|s| s.success());
        if tiled {
            sheets.push(sheet);
        } else {
            sheets.push(frames_dir);
        }
    }
    (sheets, problems)
}

fn slug(s: &str) -> String {
    let s: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-").chars().take(32).collect()
}

/// `m:ss` for reports.
pub fn clock(frame: u64, tb: Timebase) -> String {
    let s = frame * u64::from(tb.den) / u64::from(tb.num);
    format!("{}:{:02}", s / 60, s % 60)
}
