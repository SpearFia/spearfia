//! Applies resolved edit operations, captions, mix and filmstrips in the push's FilmCraft
//! session, so an agent never edits clip by clip.

use crate::mcp::Client;
use crate::push::ticks;
use serde_json::json;
use spearfia_core::project::{Project, Timebase};
use spearfia_sync::edit::{Action, Op, Style};
use spearfia_sync::{Item, Kind, TimelinePlan, TimedWord, Timing};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// What applying the edits did.
#[derive(Debug, Default)]
pub struct Applied {
    pub ops: usize,
    pub captions: usize,
    pub problems: Vec<String>,
    /// Cards as placed, for the layout and timing checks.
    pub cards: Vec<Card>,
}

/// A placed text card and the frame-space box FilmCraft measured for it.
#[derive(Debug, Clone)]
pub struct Card {
    pub from: String,
    pub frame: u64,
    pub seconds: f64,
    pub text: String,
    /// `[x0, y0, x1, y1]` in frame pixels, when FilmCraft reported it.
    pub bounds: Option<[f64; 4]>,
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
    holds: &[(u64, u64)],
    tb: Timebase,
    size: [u32; 2],
) -> Applied {
    let mut out = Applied::default();
    let mut sfx_items: BTreeMap<String, u64> = BTreeMap::new();
    for (n, op) in ops.iter().enumerate() {
        // A card ends before the next one or a hold (the intro's card) starts: they share V3.
        let next_card = ops[n + 1..]
            .iter()
            .filter(|o| matches!(o.action, Action::Card { .. }) && o.frame > op.frame)
            .map(|o| o.frame)
            .chain(holds.iter().map(|h| h.0).filter(|&h| h > op.frame))
            .min();
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
                let text = text.as_deref().unwrap_or_default();
                card(fc, style, text, secs, op.frame, tb, size).map(|bounds| {
                    out.cards.push(Card { from: op.from.clone(), frame: op.frame, seconds: secs, text: text.to_string(), bounds });
                })
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

/// What the intro and end screen placed: their picture spans (for checks and the report),
/// the intro card, and anything that failed.
#[derive(Debug, Default)]
pub struct Extras {
    pub pictures: Vec<(u64, u64)>,
    pub cards: Vec<Card>,
    pub problems: Vec<String>,
}

/// Fill the timing's holds: the intro (background, title card, hit) in the first hold and the
/// end screen (background, music fading out) in the tail.
pub fn extras(fc: &mut Client, project: &Project, style: &Style, timing: &Timing, title: &str, tb: Timebase, size: [u32; 2]) -> Extras {
    let mut out = Extras::default();
    let fps = f64::from(tb.num) / f64::from(tb.den);
    let mut pictures = Vec::new();
    if let (Some(i), Some(&(at, len))) = (&style.edit.intro, timing.holds().first()) {
        if let Some(p) = &i.picture {
            match place(fc, project, p, "V2", at, len, tb) {
                Ok(clip) => pictures.push((clip, at, len)),
                Err(e) => out.problems.push(format!("intro picture: {e}")),
            }
        }
        if let Some(text) = &i.card {
            let text = text.replace("{title}", &title.to_uppercase());
            let secs = len as f64 / fps;
            match card(fc, style, &text, secs, at, tb, size) {
                Ok(bounds) => out.cards.push(Card { from: "intro".into(), frame: at, seconds: secs, text, bounds }),
                Err(e) => out.problems.push(format!("intro card: {e}")),
            }
        }
        if let Some(name) = &i.sfx
            && let Err(e) = sfx(fc, project, &mut BTreeMap::new(), name, style.mix.sfx_db, at)
        {
            out.problems.push(format!("intro sfx: {e}"));
        }
    }
    if let (Some(e), Some((at, len))) = (&style.edit.end_screen, timing.tail()) {
        if let Some(p) = &e.picture {
            match place(fc, project, p, "V2", at, len, tb) {
                Ok(clip) => pictures.push((clip, at, len)),
                Err(err) => out.problems.push(format!("end screen picture: {err}")),
            }
        }
        if let Some(m) = &e.music {
            let fade = (2.0 * fps) as u64;
            let r = place(fc, project, m, "A2", at, len, tb).and_then(|clip| {
                fc.command("clip.audioGain", json!({"clips": [clip], "mode": "set", "db": e.music_db}))
                    .and_then(|_| fc.command("sequence.applyAudioTransition", json!({"clip": clip, "edge": "out", "frames": fade})))
                    .map(drop)
                    .map_err(|e| e.to_string())
            });
            if let Err(err) = r {
                out.problems.push(format!("end screen music: {err}"));
            }
        }
    }
    if !pictures.is_empty() {
        let ids: Vec<u64> = pictures.iter().map(|p| p.0).collect();
        let _ = fc.command("timeline.select", json!({"clips": ids, "add": false, "toggle": false}));
        let _ = fc.command("clip.scaleToFrameSize", json!({}));
        let _ = fc.command("edit.deselectAll", json!({}));
    }
    out.pictures = pictures.into_iter().map(|p| (p.1, p.2)).collect();
    out
}

/// Import a project file and place `len` frames of it from its start on `track` at `at`.
fn place(fc: &mut Client, project: &Project, rel: &str, track: &str, at: u64, len: u64, tb: Timebase) -> Result<u64, String> {
    let file = project.root.join(rel);
    if !file.is_file() {
        return Err(format!("no {rel} in the project"));
    }
    let abs = std::path::absolute(&file).map_err(|e| e.to_string())?;
    let r = fc.tool("media_import", json!({"paths": [abs.to_string_lossy()]})).map_err(|e| e.to_string())?;
    let item = r["items"][0].as_u64().ok_or("FilmCraft could not import it")?;
    let mut p = json!({"item": item, "track": track, "frame": at, "sourceIn": 0, "duration": ticks(len, tb)});
    if track.starts_with('V') {
        p["audio"] = json!(false);
    }
    let placed = fc.command("timeline.place", p).map_err(|e| e.to_string())?;
    placed["clips"][0].as_u64().ok_or_else(|| "FilmCraft placed nothing".into())
}

/// Layout and timing checks on what was placed, worked out from the plan and measured card
/// boxes, so only flagged times need looking at. Each line starts with the time and the
/// edit line it came from. `extra` pictures (intro, end screen) count as picture.
pub fn checks(cards: &[Card], plan: &TimelinePlan, extra: &[(u64, u64)], ops: &[Op], style: &Style, size: [u32; 2], tb: Timebase) -> Vec<(u64, String)> {
    let fps = f64::from(tb.num) / f64::from(tb.den);
    let (w, h) = (f64::from(size[0]), f64::from(size[1]));
    // Dips run from the first picture at or after their frame for their length.
    let dips: Vec<(u64, u64)> = ops
        .iter()
        .filter_map(|o| match &o.action {
            Action::Transition { effect, seconds } if effect.starts_with("dip") => picture_from(plan, o.frame)
                .map(|i| (i.start_frame, i.start_frame + (seconds * fps).round() as u64)),
            _ => None,
        })
        .collect();
    // shortcut: a one-line caption block is taken as 1.3 × its font size tall, since FilmCraft
    // doesn't report caption bounds; measure it if captions ever wrap to two lines.
    let caption_band = style.captions.enabled.then(|| {
        let bottom = h * (1.0 - style.captions.bottom_margin());
        (bottom - style.captions.size * 1.3, bottom)
    });
    let mut out = Vec::new();
    for c in cards {
        let at = format!("{} {}", clock(c.frame, tb), c.from);
        if let Some([x0, y0, x1, y1]) = c.bounds {
            if x0 < 0.0 || y0 < 0.0 || x1 > w || y1 > h {
                out.push((c.frame, format!(
                    "{at}: card runs off the frame ({:.0}×{:.0} px in {w:.0}×{h:.0}); break or shorten its TEXT lines",
                    x1 - x0,
                    y1 - y0
                )));
            }
            if let Some((top, bottom)) = caption_band
                && y1 > top
                && y0 < bottom
            {
                out.push((c.frame, format!("{at}: card reaches into the captions (bottom at {y1:.0} px, captions from {top:.0} px); use fewer lines")));
            }
        }
        let end = c.frame + (c.seconds * fps).round() as u64;
        let dark = (c.frame..end)
            .filter(|&f| (picture_at(plan, f).is_none() && !extra.iter().any(|&(a, n)| a <= f && f < a + n)) || dips.iter().any(|&(a, b)| a <= f && f < b))
            .count();
        if end > c.frame && dark * 2 > (end - c.frame) as usize {
            out.push((c.frame, format!("{at}: card is over black for {:.1} of its {:.1} s; link a picture there or move the card", dark as f64 / fps, c.seconds)));
        }
        // Big card text is skimmed: about 240 words a minute, plus half a second to notice it.
        let need = 0.5 + c.text.split_whitespace().count() as f64 / 4.0;
        if c.seconds + 0.05 < need {
            out.push((c.frame, format!("{at}: card shows {:.1} s, about {need:.1} s needed to read it; shorten it or give it room", c.seconds)));
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

/// Place a card; returns its measured frame-space box (`graphics.list` quad).
fn card(fc: &mut Client, style: &Style, text: &str, seconds: f64, frame: u64, tb: Timebase, size: [u32; 2]) -> Result<Option<[f64; 4]>, String> {
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
    .map_err(|e| e.to_string())?;
    // Measuring is best effort: a card FilmCraft won't measure is placed, just unchecked.
    let listed = fc.command("graphics.list", json!({"clip": clip})).ok();
    let quad = listed.as_ref().and_then(|l| l["layers"][0]["quad"].as_array().cloned()).unwrap_or_default();
    let pts: Vec<(f64, f64)> = quad.iter().filter_map(|p| Some((p[0].as_f64()?, p[1].as_f64()?))).collect();
    Ok((!pts.is_empty()).then(|| {
        let (xs, ys) = (pts.iter().map(|p| p.0), pts.iter().map(|p| p.1));
        [xs.clone().fold(f64::MAX, f64::min), ys.clone().fold(f64::MAX, f64::min),
         xs.fold(f64::MIN, f64::max), ys.fold(f64::MIN, f64::max)]
    }))
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
    for (n, (title, start, end)) in sections.iter().enumerate() {
        let name = format!("{:02}-{}", n + 1, slug(title));
        let at: Vec<u64> = (0..frames)
            .map(|k| start + (end.saturating_sub(*start)) * (2 * k + 1) / (2 * frames))
            .collect();
        sheets.extend(sheet(fc, dir, &name, &at, 3, tb, &mut problems));
    }
    (sheets, problems)
}

/// One sheet of the frames where cards and zooms land (in edit.txt order), so the decisions
/// edit.md made can be checked without opening the timeline.
pub fn edit_frames(fc: &mut Client, dir: &std::path::Path, ops: &[Op], tb: Timebase) -> (Option<PathBuf>, Vec<String>) {
    // Most of a second in, so the card is up and the zoom has moved.
    let settle = u64::from(tb.num) * 3 / (4 * u64::from(tb.den));
    let at: Vec<u64> = ops
        .iter()
        .filter(|o| matches!(o.action, Action::Card { .. } | Action::Zoom(_)))
        .map(|o| o.frame + settle)
        .collect();
    let mut problems = Vec::new();
    if at.is_empty() {
        return (None, problems);
    }
    (sheet(fc, dir, "edits", &at, 5, tb, &mut problems), problems)
}

/// Render `at` into `dir/<name>/NN.png` and tile them `cols` wide into `dir/<name>.jpg`
/// (the PNG folder when ffmpeg can't).
fn sheet(fc: &mut Client, dir: &std::path::Path, name: &str, at: &[u64], cols: usize, tb: Timebase, problems: &mut Vec<String>) -> Option<PathBuf> {
    let frames_dir = dir.join(name);
    if let Err(e) = std::fs::create_dir_all(&frames_dir) {
        problems.push(format!("filmstrip {name}: {e}"));
        return None;
    }
    for (k, f) in at.iter().enumerate() {
        match fc.frame_png(*f as f64 * f64::from(tb.den) / f64::from(tb.num), 480) {
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
        .args(["-frames:v", "1", "-vf", &format!("tile={cols}x{}", at.len().div_ceil(cols))])
        .arg(&sheet)
        .status()
        .is_ok_and(|s| s.success());
    Some(if tiled { sheet } else { frames_dir })
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

#[cfg(test)]
mod tests {
    use super::*;
    use spearfia_core::project::Timebase;

    fn card(frame: u64, seconds: f64, text: &str, bounds: [f64; 4]) -> Card {
        Card { from: format!("edit.md:{frame}"), frame, seconds, text: text.into(), bounds: Some(bounds) }
    }

    #[test]
    fn checks_flag_overflow_captions_black_and_short_cards() {
        let tb = Timebase::FPS_30;
        let plan: TimelinePlan = serde_json::from_value(json!({"$schema": "x", "timebase": tb, "items": [
            {"cue_id": "p1", "kind": "clip", "track": "V2", "start_frame": 60, "duration_frames": 540}]}))
        .unwrap();
        let ops = [Op { frame: 50, action: Action::Transition { effect: "dip_to_black".into(), seconds: 0.5 }, from: "rule section".into() }];
        let style: Style = serde_json::from_value(json!({"captions": {"enabled": true, "size": 72, "position": [0.5, 0.82]}})).unwrap();
        let fine = [700.0, 400.0, 1200.0, 600.0];
        let cards = [
            card(300, 3.0, "a b c", [-1028.0, 419.0, 2948.0, 712.0]),
            card(310, 3.0, "a b c", [700.0, 700.0, 1200.0, 850.0]),
            card(0, 3.0, "ROUND ONE", fine),
            card(400, 1.0, "one two three four five six", fine),
            card(450, 3.0, "a b c", fine),
        ];
        let out = checks(&cards, &plan, &[], &ops, &style, [1920, 1080], tb);
        let says = |frame: u64, what: &str| out.iter().any(|(f, m)| *f == frame && m.contains(what));
        assert!(says(300, "runs off the frame (3976×293 px"), "{out:?}");
        assert!(says(310, "reaches into the captions"), "{out:?}");
        assert!(says(0, "over black for 2.5 of its 3.0 s"), "{out:?}");
        assert!(says(400, "about 2.0 s needed"), "{out:?}");
        assert_eq!(out.len(), 4, "{out:?}");
        // An intro picture under the first card means it isn't over black.
        let out = checks(&cards[2..3], &plan, &[(0, 60)], &ops, &style, [1920, 1080], tb);
        assert!(out.is_empty(), "{out:?}");
    }
}
