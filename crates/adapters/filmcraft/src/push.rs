//! The one-way push (ADR 0005): compile `links.json` and the script into a timeline plan,
//! build it in a headless FilmCraft session and save it as a new review project.
//!
//! Runs only on the creator-approved script. Nothing is written until FilmCraft has built
//! every item; a failure leaves no files.

use crate::edit::{self as apply, Applied, clock};
use crate::mcp::Client;
use serde_json::{Value, json};
use spearfia_core::CoreError;
use spearfia_core::error::Result;
use spearfia_core::links::Links;
use spearfia_core::project::{Project, Timebase, write_json};
use spearfia_core::visuals::Plan;
use spearfia_sync::edit::{self, GeneratedCue};
use spearfia_sync::{Heard, Issue, Kind, TimelinePlan, Timing, compile_heard};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Push choices beyond the defaults.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Render one contact sheet per section into `sync/filmstrips/<plan_id>/`.
    pub filmstrips: bool,
}

/// One `##` section of the pushed timeline, for a one-line-per-section report.
#[derive(Debug, Clone)]
pub struct Section {
    pub title: String,
    pub start: u64,
    pub end: u64,
    /// Frames of the section with no picture on V2.
    pub picture_gap: u64,
    /// Edit operations placed in it.
    pub edits: usize,
    /// Spoken words timed by proportion because their voiceover file has no heard words.
    pub untimed_words: usize,
    /// Layout and timing checks that flagged something in it.
    pub checks: usize,
}

/// FilmCraft's time unit.
const TICKS_PER_SECOND: i128 = 254_016_000_000;

/// What a push made.
#[derive(Debug)]
pub struct Pushed {
    pub plan_id: String,
    /// The new FilmCraft project, `sync/review-<plan_id>.fcproj`.
    pub review: PathBuf,
    pub plan: TimelinePlan,
    /// What compile skipped or shortened.
    pub issues: Vec<Issue>,
    /// Edit script, channel rules, captions and mix as applied.
    pub edit: Applied,
    /// Voiceover files placed, and those with no heard words (timed linearly inside the file).
    pub voice_files: usize,
    pub untimed_voice: Vec<String>,
    /// Style settings asked for but not applied: unsupported keys, or levels with nothing to set.
    pub not_applied: Vec<String>,
    /// Layout and timing problems found without looking at frames (`m:ss <edit line>: …`).
    pub checks: Vec<String>,
    pub sections: Vec<Section>,
    pub filmstrips: Vec<PathBuf>,
    pub timebase: Timebase,
}

impl Pushed {
    /// What the CLI and the MCP tool report: the review path, counts, one line per section and
    /// every issue, never the timeline itself.
    pub fn summary(&self) -> Value {
        let count = |k: Kind| self.plan.items.iter().filter(|i| i.kind == k).count();
        let tb = self.timebase;
        let secs = |f: u64| (f * u64::from(tb.den)) as f64 / f64::from(tb.num);
        let sections: Vec<String> = self
            .sections
            .iter()
            .map(|s| {
                let mut line = format!(
                    "{} {} ({:.0} s): {} edits, {:.0} s without picture",
                    clock(s.start, tb),
                    s.title,
                    secs(s.end - s.start),
                    s.edits,
                    secs(s.picture_gap)
                );
                if s.untimed_words > 0 {
                    line += &format!(", {} words not word-timed", s.untimed_words);
                }
                if s.checks > 0 {
                    line += &format!(", {} checks flagged", s.checks);
                }
                line
            })
            .collect();
        let timed = self.voice_files - self.untimed_voice.len();
        json!({"plan_id": self.plan_id, "review": self.review, "clips": count(Kind::Clip),
               "transitions": count(Kind::Transition),
               "word_timing": format!("{timed}/{} voiceover files", self.voice_files),
               "untimed_voice": self.untimed_voice, "edits": self.edit.ops, "captions": self.edit.captions,
               "not_applied": self.not_applied, "checks": self.checks, "sections": sections, "edit_problems": self.edit.problems,
               "filmstrips": self.filmstrips, "issues": self.issues})
    }
}

/// Heard words from `media/vo-words.json` (written by the voiceover skill's `vo_words.py`).
fn heard(project: &Project) -> Heard {
    let Ok(text) = std::fs::read_to_string(project.root.join("media").join("vo-words.json")) else {
        return Heard::new();
    };
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    v.as_object()
        .into_iter()
        .flatten()
        .map(|(path, f)| {
            let words = f["words"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|w| Some((w[0].as_str()?.to_string(), w[1].as_f64()?)))
                .collect();
            (path.clone(), words)
        })
        .collect()
}

/// Push `project`'s links to a new FilmCraft review project with the `filmcraft-cli` at `cli`.
pub fn push(project: &Project, cli: &Path) -> Result<Pushed> {
    push_with(project, cli, &Options::default())
}

/// [`push`] with options.
pub fn push_with(project: &Project, cli: &Path, opts: &Options) -> Result<Pushed> {
    let plan_id = format!(
        "plan-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    );
    let sync = absolute(&project.root.join("sync"))?;
    let review = sync.join(format!("review-{plan_id}.fcproj"));
    // Never overwrite a review: the creator may have edited it in FilmCraft.
    if review.exists() {
        return Err(CoreError::Invalid(format!(
            "{} already exists; push again in a second.",
            review.display()
        )));
    }
    // The creator's approval gates every entry point (CLI, MCP, desktop) here.
    let script = spearfia_script::parse(&project.approved_script()?);
    let links = Links::load(project)?;
    let settings = &project.file.settings;
    let tb = settings.timebase;

    let mut fc = Client::start(cli)?;

    // Import each linked file on its own, so a file FilmCraft can't read stays out of `media`
    // (compile reports it as missing) instead of shifting the other ids.
    let mut items: BTreeMap<String, u64> = BTreeMap::new();
    for path in links
        .links
        .iter()
        .map(|l| &l.media.path)
        .collect::<BTreeSet<_>>()
    {
        let abs = absolute(&project.resolve(path))?;
        let Some(abs_str) = abs.to_str().filter(|_| abs.is_file()) else {
            continue;
        };
        if let Ok(r) = fc.tool("media_import", json!({"paths": [abs_str]}))
            && let Some(id) = r["items"][0].as_u64()
        {
            items.insert(path.clone(), id);
        }
    }
    let lengths = item_lengths(&fc.tool("project_inspect", json!({}))?["root"]);
    let media = items
        .iter()
        .filter_map(|(path, id)| {
            let (kind, ticks) = lengths.get(id)?;
            let frames = (kind != "Still Image").then(|| frames_of(*ticks, tb));
            Some((path.clone(), frames))
        })
        .collect();

    let heard = heard(project);
    let edit_md = std::fs::read_to_string(project.root.join("edit").join("edit.md")).ok();
    let brief = project.channel_brief()?;
    let (style, _) = edit::merged_style(brief.as_deref(), edit_md.as_deref()).map_err(CoreError::Invalid)?;
    // The intro and end screen need time with no narration.
    let holds = style.edit.holds(tb);
    let (mut plan, issues) = compile_heard(&script, &links, &media, tb, &heard, &holds);
    plan.plan_id = Some(plan_id.clone());

    let picture_sound = style.mix.picture_db.is_some();
    fc.command(
        "file.newSequence",
        json!({"name": project.name(), "width": settings.resolution[0], "height": settings.resolution[1],
               "fps": f64::from(tb.num) / f64::from(tb.den), "sampleRate": settings.sample_rate,
               "video": if style.edit.blur_strip.is_some() { 4 } else { 3 },
               "audio": if picture_sound { 4 } else { 3 }}),
    )?;
    let mut clips: BTreeMap<&str, u64> = BTreeMap::new();
    let mut sounds: Vec<u64> = Vec::new();
    for it in plan.items.iter().filter(|i| i.kind == Kind::Clip) {
        let Some(src) = &it.source else { continue };
        let item = items[&src.path];
        let mut p = json!({"item": item, "track": it.track, "frame": it.start_frame,
                           "sourceIn": ticks(src.in_frame, tb), "duration": ticks(it.duration_frames, tb)});
        if it.track.starts_with('V') {
            p["audio"] = json!(false);
        }
        let placed = fc.command("timeline.place", p)?;
        let clip = placed["clips"][0].as_u64().ok_or_else(|| {
            CoreError::Invalid(format!("FilmCraft placed nothing for {}", it.cue_id))
        })?;
        let file = Path::new(&src.path)
            .file_name()
            .map_or(src.path.clone(), |n| n.to_string_lossy().into_owned());
        // shortcut: the cue ID rides in the clip name until FilmCraft clips carry metadata (ADR 0005).
        fc.command(
            "clip.rename",
            json!({"clip": clip, "name": format!("{file} {{#{}}}", it.cue_id)}),
        )?;
        clips.insert(&it.cue_id, clip);
        // The same span's own sound on A4 (an audio track takes a video file's sound alone),
        // so it gets its own fader; stills have no length and no sound.
        if picture_sound && it.track.starts_with('V') && media.get(&src.path).is_some_and(Option::is_some) {
            let placed = fc.command(
                "timeline.place",
                json!({"item": item, "track": "A4", "frame": it.start_frame,
                       "sourceIn": ticks(src.in_frame, tb), "duration": ticks(it.duration_frames, tb)}),
            )?;
            if let Some(sound) = placed["clips"][0].as_u64() {
                fc.command(
                    "clip.rename",
                    json!({"clip": sound, "name": format!("{file} sound {{#{}}}", it.cue_id)}),
                )?;
                sounds.push(sound);
            }
        }
    }
    // Footage of any size fills the frame (FilmCraft's Scale to Frame Size on every picture).
    let pictures: Vec<u64> = plan
        .items
        .iter()
        .filter(|i| i.kind == Kind::Clip && i.track.starts_with('V'))
        .filter_map(|i| clips.get(i.cue_id.as_str()).copied())
        .collect();
    if !pictures.is_empty() {
        fc.command("timeline.select", json!({"clips": pictures, "add": false, "toggle": false}))?;
        fc.command("clip.scaleToFrameSize", json!({}))?;
        fc.command("edit.deselectAll", json!({}))?;
    }
    for it in plan.items.iter().filter(|i| i.kind == Kind::Transition) {
        let params = it.params.clone().unwrap_or_default();
        let Some(&clip) = params["attach"].as_str().and_then(|a| clips.get(a)) else {
            return Err(CoreError::Invalid(format!(
                "transition {} attaches to no placed clip",
                it.cue_id
            )));
        };
        let mut p = json!({"clip": clip, "edge": params["edge"].as_str().unwrap_or("in"), "effect": params["effect"]});
        if it.duration_frames > 0 {
            p["frames"] = json!(it.duration_frames);
        }
        fc.command("sequence.applyVideoTransition", p)?;
    }

    // The edit script and the channel's rules, captions and mix, all applied here.
    let timing = Timing::new(&script, &links, &media, tb, &heard, &holds);
    let mut applied = Applied::default();
    let (lines, mut problems) = edit::parse(edit_md.as_deref().unwrap_or_default());
    let generated: Vec<GeneratedCue> = project
        .read_visuals()
        .ok()
        .and_then(|src| Plan::parse(&src).ok())
        .map(|p| {
            p.cues
                .iter()
                .filter(|c| c.meta.method.starts_with("Generated"))
                .map(|c| GeneratedCue {
                    id: c.meta.id.clone(),
                    quote: c.meta.anchor.quote.clone(),
                    line: c.meta.anchor.line,
                    text: edit::cue_text(p.source.get(c.body.clone()).unwrap_or_default()),
                })
                .collect()
        })
        .unwrap_or_default();
    let (ops, resolve_problems) = edit::resolve(&lines, &style, &generated, &timing, &script);
    problems.extend(resolve_problems);
    // Every operation with its time, for revising edit.md without reading the timeline.
    let listing: String = ops
        .iter()
        .map(|o| format!("{}  {:<28} {:?}
", clock(o.frame, tb), o.from, o.action))
        .collect();
    let size = [settings.resolution[0], settings.resolution[1]];
    let mut done = apply::apply(&mut fc, project, &plan, &clips, &style, &ops, &timing.holds(), tb, size);
    applied.ops = done.ops;
    problems.append(&mut done.problems);
    let mut extras = apply::extras(&mut fc, project, &style, &timing, &project.name(), tb, size);
    problems.append(&mut extras.problems);
    done.cards.append(&mut extras.cards);
    let video_end = timing.tail().map_or(timing.end(), |(at, len)| at + len);
    if let Err(e) = apply::blur_strip(&mut fc, &style, video_end, tb, size) {
        problems.push(format!("blur strip: {e}"));
    }
    let checks = apply::checks(&done.cards, &plan, &extras.pictures, &ops, &style, size, tb);
    if style.captions.enabled {
        match apply::captions(&mut fc, &items, &timing.words(), &style, tb) {
            Ok(n) => applied.captions = n,
            Err(e) => problems.push(format!("captions: {e}")),
        }
    }
    if style.mix.enabled {
        problems.extend(apply::mix(&mut fc, &plan, &clips, &style));
        if let Some(db) = style.mix.picture_db
            && db != 0.0
            && !sounds.is_empty()
            && let Err(e) = fc.command("clip.audioGain", json!({"clips": sounds, "mode": "set", "db": db}))
        {
            problems.push(format!("mix A4: {e}"));
        }
    }
    applied.problems = problems;

    let mut not_applied = style.not_applied.clone();
    if style.mix.enabled {
        // `sfx` edit lines and the intro's sfx also land on A3, outside the plan.
        let edit_sfx = ops.iter().any(|o| matches!(o.action, edit::Action::Sfx { .. }))
            || style.edit.intro.as_ref().is_some_and(|i| i.sfx.is_some());
        for (track, key, db) in [("A2", "mix.music_db", style.mix.music_db), ("A3", "mix.sfx_db", style.mix.sfx_db)] {
            if db != 0.0
                && !(track == "A3" && edit_sfx)
                && !plan.items.iter().any(|i| i.kind == Kind::Clip && i.track == track)
            {
                not_applied.push(format!("{key} (nothing on {track})"));
            }
        }
    }
    let words = timing.words();
    let untimed = |p: &str| heard.get(p).is_none_or(Vec::is_empty);
    let voice: BTreeSet<&str> = words.iter().map(|w| w.path.as_str()).collect();
    let untimed_voice: Vec<String> = voice.iter().filter(|p| untimed(p)).map(|p| p.to_string()).collect();

    let mut starts = timing.sections(&script);
    let tail = timing.tail();
    if let Some((at, _)) = tail {
        starts.push(("End screen".into(), at));
    }
    let last = tail.map_or(timing.end(), |(at, len)| at + len);
    let sections: Vec<Section> = starts
        .iter()
        .enumerate()
        .map(|(n, (title, start))| {
            let end = starts.get(n + 1).map_or(last, |s| s.1).max(*start);
            let covered: u64 = plan
                .items
                .iter()
                .filter(|i| i.kind == Kind::Clip && i.track == "V2")
                .map(|i| (i.start_frame, i.duration_frames))
                .chain(extras.pictures.iter().copied())
                .map(|(a, n)| (a + n).min(end).saturating_sub(a.max(*start)))
                .sum();
            Section {
                title: title.clone(),
                start: *start,
                end,
                picture_gap: (end - start).saturating_sub(covered),
                edits: ops.iter().filter(|o| o.frame >= *start && o.frame < end).count(),
                untimed_words: words
                    .iter()
                    .filter(|w| w.start >= *start && w.start < end && untimed(&w.path))
                    .count(),
                checks: checks.iter().filter(|(f, _)| f >= start && *f < end).count(),
            }
        })
        .collect();

    std::fs::create_dir_all(&sync).map_err(|source| CoreError::Io {
        path: sync.clone(),
        source,
    })?;
    let review_str = review
        .to_str()
        .ok_or_else(|| CoreError::Invalid("the project path is not UTF-8".into()))?;
    fc.command("file.saveAs", json!({"path": review_str}))?;
    let snapshot = fc.tool("sequence_inspect", json!({}))?;
    write_json(&sync.join("plans").join(format!("{plan_id}.json")), &plan)?;
    write_json(
        &sync.join("snapshots").join(format!("{plan_id}.json")),
        &snapshot,
    )?;
    let _ = std::fs::write(sync.join("plans").join(format!("{plan_id}.edit.txt")), listing);
    let mut filmstrips = Vec::new();
    if opts.filmstrips {
        let spans: Vec<_> = sections.iter().map(|s| (s.title.clone(), s.start, s.end)).collect();
        let dir = sync.join("filmstrips").join(&plan_id);
        let (sheets, mut p) = apply::filmstrips(&mut fc, &dir, &spans, 6, tb);
        filmstrips = sheets;
        applied.problems.append(&mut p);
        let (edits, mut p) = apply::edit_frames(&mut fc, &dir, &ops, tb);
        filmstrips.extend(edits);
        applied.problems.append(&mut p);
    }
    Ok(Pushed {
        plan_id,
        review,
        plan,
        issues,
        edit: applied,
        voice_files: voice.len(),
        untimed_voice,
        not_applied,
        checks: checks.into_iter().map(|(_, m)| m).collect(),
        sections,
        filmstrips,
        timebase: tb,
    })
}

fn absolute(p: &Path) -> Result<PathBuf> {
    std::path::absolute(p).map_err(|source| CoreError::Io {
        path: p.into(),
        source,
    })
}

/// Every item in a `project_inspect` bin tree: id → (type, duration in ticks).
fn item_lengths(bin: &Value) -> BTreeMap<u64, (String, i64)> {
    let mut out = BTreeMap::new();
    for c in bin["children"].as_array().into_iter().flatten() {
        if let Some(id) = c["item"].as_u64() {
            let kind = c["type"].as_str().unwrap_or_default().to_owned();
            out.insert(id, (kind, c["duration"].as_i64().unwrap_or(0)));
        } else {
            out.extend(item_lengths(c));
        }
    }
    out
}

pub(crate) fn ticks(frames: u64, tb: Timebase) -> i64 {
    (i128::from(frames) * TICKS_PER_SECOND * i128::from(tb.den) / i128::from(tb.num)) as i64
}

/// Whole frames in `ticks` (rounded down, so a clip never asks for more media than exists).
fn frames_of(ticks: i64, tb: Timebase) -> u64 {
    (i128::from(ticks.max(0)) * i128::from(tb.num) / (TICKS_PER_SECOND * i128::from(tb.den))) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_ticks_round_trip_at_ntsc_rates() {
        for tb in [
            Timebase::FPS_29_97,
            Timebase::FPS_59_94,
            Timebase::FPS_23_976,
            Timebase::FPS_25,
        ] {
            for f in [0, 1, 1001, 123_456] {
                assert_eq!(frames_of(ticks(f, tb), tb), f, "{tb:?} {f}");
            }
        }
        assert_eq!(ticks(30, Timebase::FPS_30), TICKS_PER_SECOND as i64);
    }

    #[test]
    fn finds_items_in_nested_bins() {
        let root = json!({"children": [
            {"item": 1, "type": "Audio", "duration": 5},
            {"id": 9, "children": [{"item": 2, "type": "Still Image", "duration": 7}]},
        ]});
        let m = item_lengths(&root);
        assert_eq!(m[&1], ("Audio".to_owned(), 5));
        assert_eq!(m[&2], ("Still Image".to_owned(), 7));
    }
}
