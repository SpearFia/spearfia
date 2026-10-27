//! Pushes a small project through the real `filmcraft-cli mcp`, then reopens the saved review
//! project with `filmcraft-cli` to check that clips, cue IDs and the transition survived the
//! save. Skipped (passes with a note) when no FilmCraft CLI is installed. Media is generated
//! here: a silent WAV and a 4×4 BMP.

use serde_json::Value;
use spearfia_adapter_filmcraft::{detect, push};
use spearfia_core::links::{Link, LinkKind, LinkMedia, Links};
use spearfia_core::project::{
    Format, MediaFolder, MediaRole, NewProject, NewVision, Project, ScriptStart, Timebase,
    create_project,
};
use std::path::Path;
use std::process::Command;

const SCRIPT: &str = "## Cold Open
You'd think it would help.
and it almost made me quit. :transition[dip to black]{#t1 duration=0.2}
## Chapter One
I'm a circuit racer.
";

fn wav(path: &Path, seconds: u32) {
    let rate = 48_000u32;
    let data = rate * 2 * seconds;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    b.resize(b.len() + data as usize, 0);
    std::fs::write(path, b).unwrap();
}

fn bmp(path: &Path) {
    let pixels = 4 * 4 * 3; // 4×4, 24-bit, rows already 4-byte aligned
    let mut b = Vec::new();
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&(54u32 + pixels).to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&4i32.to_le_bytes());
    b.extend_from_slice(&4i32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&[0; 24]); // no compression, default sizes and palette
    b.resize(b.len() + pixels as usize, 0x80);
    std::fs::write(path, b).unwrap();
}

fn link(id: &str, quote: &str, line: usize, path: &str, role: MediaRole) -> Link {
    Link {
        id: id.into(),
        quote: quote.into(),
        line,
        media: LinkMedia {
            path: path.into(),
            hash: None,
        },
        role,
        kind: LinkKind::for_role(role),
        in_point: None,
        out_point: None,
    }
}

/// A project with linked, generated media; `approved` also records the creator's approval.
fn project(name: &str, approved: bool) -> Project {
    let root = std::env::temp_dir().join(format!("spearfia-fc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let p = create_project(&NewProject {
        name: "Push test".into(),
        series: None,
        root: root.clone(),
        format: Format::LongForm,
        resolution: [1920, 1080],
        timebase: Timebase::FPS_30,
        target_runtime_seconds: None,
        vision: NewVision {
            extends: None,
            goal: String::new(),
            avg_shot_seconds: None,
            cut_style: None,
            template: None,
        },
        script: ScriptStart::Sections,
        media_folders: MediaFolder::defaults(),
        filmcraft: None,
        channel_brief: Some("# Channel\nTest channel.".into()),
    })
    .unwrap();
    p.write_script(SCRIPT).unwrap();
    if approved {
        p.approve_script(SCRIPT).unwrap();
    }
    std::fs::create_dir_all(root.join("gen")).unwrap();
    wav(&root.join("gen/one.wav"), 2);
    wav(&root.join("gen/two.wav"), 1);
    bmp(&root.join("gen/still.bmp"));
    Links {
        links: vec![
            link(
                "k1",
                "You'd think it would help.",
                1,
                "gen/one.wav",
                MediaRole::Voiceover,
            ),
            link(
                "k2",
                "almost made me quit.",
                2,
                "gen/still.bmp",
                MediaRole::BRoll,
            ),
            link(
                "k3",
                "I'm a circuit racer.",
                4,
                "gen/two.wav",
                MediaRole::Voiceover,
            ),
            link("k4", "circuit racer.", 4, "gen/still.bmp", MediaRole::BRoll),
        ],
        next_id: 5,
        unreadable: Vec::new(),
    }
    .save(&p)
    .unwrap();
    p
}

/// The first object anywhere in `v` whose `name` ends with `suffix`.
fn named<'a>(v: &'a Value, suffix: &str) -> Option<&'a Value> {
    match v {
        Value::Object(m)
            if m.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| n.ends_with(suffix)) =>
        {
            Some(v)
        }
        Value::Object(m) => m.values().find_map(|c| named(c, suffix)),
        Value::Array(a) => a.iter().find_map(|c| named(c, suffix)),
        _ => None,
    }
}

#[test]
fn pushes_links_to_a_saved_review_project() {
    let Some(cli) = detect().and_then(|i| i.cli) else {
        eprintln!("skipped: no filmcraft-cli found (set SPEARFIA_FILMCRAFT)");
        return;
    };
    let p = project("push", true);
    let pushed = push(&p, &cli).unwrap();
    assert_eq!(pushed.issues, vec![]);
    let summary = pushed.summary();
    assert_eq!(
        (summary["clips"].as_u64(), summary["transitions"].as_u64()),
        (Some(4), Some(1))
    );
    assert_eq!(
        summary["review"].as_str().map(Path::new),
        Some(pushed.review.as_path())
    );
    let sync = p.root.join("sync");
    assert!(pushed.review.is_file());
    assert!(
        sync.join("plans")
            .join(format!("{}.json", pushed.plan_id))
            .is_file()
    );
    assert!(
        sync.join("snapshots")
            .join(format!("{}.json", pushed.plan_id))
            .is_file()
    );

    // Reopen what FilmCraft saved.
    let out = Command::new(&cli)
        .arg("--project")
        .arg(&pushed.review)
        .args(["inspect", "sequence"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let seq: Value = serde_json::from_slice(&out.stdout).unwrap();

    // 30 fps: VO one is 60 frames, two follows at 60. "almost" is 7 of 11 words into the
    // first span (60 * 7 / 11 = 38); "circuit" is 2 of 4 into the second (60 + 15).
    let start = |cue: &str| {
        named(&seq, &format!("{{#{cue}}}")).unwrap_or_else(|| panic!("no clip {cue}"))["startFrame"]
            .as_u64()
    };
    assert_eq!(start("k1"), Some(0));
    assert_eq!(start("k3"), Some(60));
    assert_eq!(start("k2"), Some(38));
    assert_eq!(start("k4"), Some(75));
    assert!(
        seq.to_string().contains("dip_to_black"),
        "no transition in {seq}"
    );

    let _ = std::fs::remove_dir_all(&p.root);
}

#[test]
fn refuses_to_push_an_unapproved_script() {
    let p = project("unapproved", false);
    // The gate runs before FilmCraft starts, so no CLI is needed.
    let e = push(&p, Path::new("no-such-filmcraft-cli"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("approval"), "{e}");
    let sync = p.root.join("sync");
    assert!(!sync.join("plans").exists() && !sync.join("snapshots").exists());
    let _ = std::fs::remove_dir_all(&p.root);
}

#[test]
fn native_captions_visual_treatments_and_mix_survive_save() {
    for tb in [Timebase::FPS_30, Timebase::FPS_29_97, Timebase::FPS_59_94] {
        native_execution(tb);
    }
}
fn native_execution(tb: Timebase) {
    use spearfia_analysis::{Alignment, MeasuredWord, hash_file, script_words};
    use spearfia_core::{project::write_json, visuals};
    let Some(cli) = detect().and_then(|i| i.cli) else {
        eprintln!("skipped: no FilmCraft CLI installed");
        return;
    };
    let mut p = project(&format!("execution-{}", tb.num), true);
    p.file.settings.timebase = tb;
    write_json(&p.root.join("project.json"), &p.file).unwrap();
    // Fixture measurements have deliberately uneven pauses; they test frame mapping,
    // not speech recognition. No generation service is contacted.
    for (file, lines, duration, spacing) in [
        ("gen/one.wav", vec![1, 2], 2.0, 0.15),
        ("gen/two.wav", vec![4], 1.0, 0.2),
    ] {
        let expected: Vec<_> = script_words(SCRIPT)
            .into_iter()
            .filter(|w| lines.contains(&w.line))
            .collect();
        let measured: Vec<_> = expected
            .iter()
            .enumerate()
            .map(|(i, w)| MeasuredWord {
                word: w.text.clone(),
                start: 0.05 + i as f64 * spacing,
                end: 0.15 + i as f64 * spacing,
            })
            .collect();
        let alignment = Alignment::measured(
            SCRIPT,
            file.into(),
            hash_file(&p.resolve(file)).unwrap(),
            &lines,
            &measured,
            duration,
            tb,
        )
        .unwrap();
        write_json(
            &p.root.join(format!("analysis/{}.alignment.json", lines[0])),
            &alignment,
        )
        .unwrap();
    }
    let mut style = spearfia_sync::render::Style::default();
    style.captions.enabled = true;
    style.captions.mode = "word".into();
    style.mix.enabled = true;
    write_json(&p.root.join("production/style.json"), &style).unwrap();
    let start = SCRIPT.find("You'd").unwrap();
    let end = SCRIPT.find("## Chapter").unwrap();
    let (source, id) = visuals::Plan::parse(visuals::EMPTY)
        .unwrap()
        .add(
            "Opening graphic",
            visuals::Anchor::new(SCRIPT, start..end).unwrap(),
        )
        .unwrap();
    let plan = visuals::Plan::parse(&source).unwrap();
    let mut meta = plan.cue(&id).unwrap().meta.clone();
    meta.details.insert("execution".into(),serde_json::json!({"layers":[{"kind":"text","text":"Measured timing","position":[0.5,0.25],"font_size":56},{"kind":"shape","shape":"rectangle","position":[0.5,0.3],"size":[0.1,0.02],"color":"#ffcc00","track":5},{"kind":"media","path":"gen/still.bmp","position":[0.85,0.2],"size":[0.08,0.08],"opacity":65,"track":6}],"camera":{"from":100,"to":110}}).to_string());
    let source = plan.update(&meta).unwrap();
    p.write_visuals(&p.read_visuals().unwrap(), &source)
        .unwrap();
    let mut links = Links::load(&p).unwrap();
    links
        .links
        .push(link("k5", "You'd", 1, "gen/one.wav", MediaRole::Music));
    links
        .links
        .push(link("k6", "think", 1, "gen/two.wav", MediaRole::Sfx));
    links.save(&p).unwrap();
    let pushed = push(&p, &cli).unwrap();
    let second = pushed.plan.items.iter().find(|i| i.cue_id == "k3").unwrap();
    let second_start = (2.0 * f64::from(tb.num) / f64::from(tb.den)).floor() as u64;
    assert_eq!(second.start_frame, second_start);
    let out = Command::new(&cli)
        .arg("--project")
        .arg(&pushed.review)
        .args(["inspect", "sequence"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let seq: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        named(&seq, "{#caption-0-0}").is_some(),
        "No native caption: {seq}"
    );
    assert!(
        named(&seq, &format!("{{#{id}-layer-0}}")).is_some(),
        "No visual layer: {seq}"
    );
    let voice = named(&seq, "{#k1}").unwrap();
    assert_eq!(voice["gainDb"], 0.0);
    let music = named(&seq, "{#k5}").unwrap();
    assert_eq!(music["gainDb"], -20.0);
    let sfx = named(&seq, "{#k6}").unwrap();
    assert_eq!(sfx["gainDb"], -10.0);
    assert!(
        music["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["effect"] == "volume"
                && e["params"]["level"]["keyframes"].as_u64().unwrap_or(0) >= 4)
    );
    let mixer = Command::new(&cli)
        .arg("--project")
        .arg(&pushed.review)
        .args(["exec", "mixer.inspect"])
        .output()
        .unwrap();
    assert!(mixer.status.success());
    let mixer: Value = serde_json::from_slice(&mixer.stdout).unwrap();
    let bed = mixer["strips"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["ref"] == "A2")
        .unwrap();
    assert_eq!(bed["mode"], "Read");
    assert!(
        bed["lanes"]["volume"]
            .as_array()
            .unwrap()
            .iter()
            .any(|key| key[1] == -8.0)
    );
    let execution: Value = serde_json::from_str(
        &std::fs::read_to_string(
            p.root
                .join(format!("sync/render-plans/{}.json", pushed.plan_id)),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(execution["captions"].as_array().unwrap().len(), 15);
    let second_caption = execution["captions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["text"].as_str().unwrap().starts_with("I'm"))
        .unwrap();
    assert_eq!(
        second_caption["start_frame"].as_u64(),
        Some(second_start + spearfia_analysis::frame(0.05, tb).unwrap()),
        "Alignment drifted at {tb:?}"
    );
    assert!(!execution["effects"].as_array().unwrap().is_empty());
    // Leave the synthetic review available for visual QA in the test temp folder.
    eprintln!("native execution review: {}", pushed.review.display());
}
