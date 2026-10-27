//! The one-way push (ADR 0005): compile `links.json` and the script into a timeline plan,
//! build it in a headless FilmCraft session and save it as a new review project.
//!
//! Nothing is written until FilmCraft has built every item; a failure leaves no files.

use crate::mcp::Client;
use serde_json::{Value, json};
use spearfia_core::CoreError;
use spearfia_core::error::Result;
use spearfia_core::links::Links;
use spearfia_core::project::{Project, Timebase, write_json};
use spearfia_sync::{Issue, Kind, TimelinePlan, compile};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

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
}

/// Push `project`'s links to a new FilmCraft review project with the `filmcraft-cli` at `cli`.
pub fn push(project: &Project, cli: &Path) -> Result<Pushed> {
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
    let script = spearfia_script::parse(&project.read_script()?);
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

    let (mut plan, issues) = compile(&script, &links, &media, tb);
    plan.plan_id = Some(plan_id.clone());

    fc.command(
        "file.newSequence",
        json!({"name": project.name(), "width": settings.resolution[0], "height": settings.resolution[1],
               "fps": f64::from(tb.num) / f64::from(tb.den), "sampleRate": settings.sample_rate, "video": 3, "audio": 1}),
    )?;
    let mut clips: BTreeMap<&str, u64> = BTreeMap::new();
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
    Ok(Pushed {
        plan_id,
        review,
        plan,
        issues,
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

fn ticks(frames: u64, tb: Timebase) -> i64 {
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
