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
