use super::*;
use spearfia_core::links::LinkMedia;

const SCRIPT: &str = "## Cold Open
You'd think it would help.
[ON SCREEN: words slam]
and it almost made me quit. :transition[dissolve]{#t1 duration=0.5}
## Chapter One :transition[smash cut]{#t2}
I'm a circuit racer.
:transition[spin]{#t3}
";

const TB: Timebase = Timebase::FPS_30;

fn link(
    id: &str,
    quote: &str,
    line: usize,
    path: &str,
    role: MediaRole,
    inp: Option<&str>,
) -> Link {
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
        in_point: inp.map(Into::into),
        out_point: None,
    }
}

fn clip_at(cue: &str, track: &str, start: u64, dur: u64, path: &str, in_frame: u64) -> Item {
    Item {
        cue_id: cue.into(),
        kind: Kind::Clip,
        track: track.into(),
        start_frame: start,
        duration_frames: dur,
        source: Some(Source {
            path: path.into(),
            in_frame,
        }),
        params: None,
        intent: None,
    }
}

#[test]
fn voiceover_sets_timing_and_pictures_fill_it() {
    use MediaRole::*;
    let links = Links {
        links: vec![
            link(
                "k1",
                "think it would help.",
                1,
                "vo/one.wav",
                Voiceover,
                None,
            ),
            link(
                "k2",
                "almost made me quit.",
                3,
                "broll/track.mp4",
                BRoll,
                Some("1"),
            ),
            link(
                "k3",
                "I'm a circuit racer.",
                5,
                "vo/two.wav",
                Voiceover,
                None,
            ),
            link("k4", "words slam]", 2, "screens/title.png", Screen, None),
            link("k5", "circuit racer.", 5, "broll/short.mp4", BRoll, None),
            link("k6", "not in the script", 2, "broll/track.mp4", BRoll, None),
            link("k7", "help.", 1, "broll/gone.mp4", BRoll, None),
            link("k8", "You'd", 1, "broll/track.mp4", BRoll, None),
            link("k9", "help.", 1, "broll/track.mp4", BRoll, Some("1:xx")),
        ],
        next_id: 10,
        unreadable: Vec::new(),
    };
    let media = BTreeMap::from([
        ("vo/one.wav".to_string(), Some(300)),
        ("vo/two.wav".to_string(), Some(150)),
        ("broll/track.mp4".to_string(), Some(1000)),
        ("broll/short.mp4".to_string(), Some(50)),
        ("screens/title.png".to_string(), None),
    ]);
    let (plan, issues) = compile(&spearfia_script::parse(SCRIPT), &links, &media, TB);

    // Span 1: k1 owns the 10 spoken words from "think" to "quit." (the on-screen line takes
    // no time), 300 frames. Span 2: k3, 4 words, 150 frames.
    let mut short = clip_at("k5", "V2", 375, 50, "broll/short.mp4", 0);
    short.intent = Some(Intent {
        start_frame: 375,
        duration_frames: 75,
    });
    let transition = Item {
        cue_id: "t1".into(),
        kind: Kind::Transition,
        track: "V2".into(),
        start_frame: 375,
        duration_frames: 15,
        source: None,
        params: Some(json!({"effect": "cross_dissolve", "attach": "k5", "edge": "in"})),
        intent: None,
    };
    assert_eq!(
        plan.items,
        vec![
            clip_at("k1", "A1", 0, 300, "vo/one.wav", 0),
            clip_at("k3", "A1", 300, 150, "vo/two.wav", 0),
            // the direction sits before "and", 4 of 10 words in; a still fills its slot
            clip_at("k4", "V1", 120, 180, "screens/title.png", 0),
            // "almost" is 6 words in; in at 1 s; runs to the end of span 1
            clip_at("k2", "V2", 180, 120, "broll/track.mp4", 30),
            // "circuit" is 2 of 4 words into span 2; the file is 25 frames short
            short,
            transition,
        ]
    );
    assert_eq!(
        issues,
        vec![
            Issue::Lost { link: "k6".into() },
            Issue::Missing { link: "k7".into() },
            Issue::BadPoint { link: "k9".into() },
            Issue::Unplaced { link: "k8".into() },
            Issue::Short {
                link: "k5".into(),
                frames: 25
            },
            Issue::Unmapped {
                cue: Some("t3".into()),
                label: Some("spin".into())
            },
        ]
    );

    let v = serde_json::to_value(&plan).unwrap();
    assert_eq!(v["$schema"], SCHEMA_URL);
    assert_eq!(v["timebase"], json!({"num": 30, "den": 1}));
    assert_eq!(v["items"][5]["kind"], "transition");
    assert!(v.get("plan_id").is_none() && v["items"][0].get("intent").is_none());
}

#[test]
fn voiceover_without_length_or_room_is_reported() {
    let mut late = link(
        "k2",
        "help.",
        1,
        "vo/one.wav",
        MediaRole::Voiceover,
        Some("20"),
    );
    late.out_point = Some("30".into());
    let links = Links {
        links: vec![
            link(
                "k1",
                "You'd think",
                1,
                "vo/still.png",
                MediaRole::Voiceover,
                None,
            ),
            late,
        ],
        next_id: 3,
        unreadable: Vec::new(),
    };
    let media = BTreeMap::from([
        ("vo/one.wav".to_string(), Some(300)),
        ("vo/still.png".to_string(), None),
    ]);
    let (plan, issues) = compile(
        &spearfia_script::parse("## Cold Open\nYou'd think it would help.\n"),
        &links,
        &media,
        TB,
    );
    assert!(plan.items.is_empty());
    assert_eq!(
        issues,
        vec![
            Issue::Untimed { link: "k1".into() },
            Issue::Empty { link: "k2".into() }
        ]
    );
}

#[test]
fn points_parse_as_timecode_or_seconds() {
    let p = |s: &str| point(&Some(s.into()), TB);
    assert_eq!(point(&None, TB), Some(None));
    assert_eq!(p("00:01:12"), Some(Some(2160)));
    assert_eq!(p("1:30"), Some(Some(2700)));
    assert_eq!(p("78.5"), Some(Some(2355)));
    assert_eq!(
        point(&Some("1".into()), Timebase::FPS_29_97),
        Some(Some(30))
    );
    for bad in ["", "1:xx", "1:2:3:4", ":5", "1.5:00", "-1", "1.2.3"] {
        assert_eq!(p(bad), None, "{bad}");
    }
}

#[test]
fn crate_name_is_namespaced() {
    assert_eq!(NAME, "spearfia-sync");
}

#[test]
fn music_and_sfx_get_their_own_tracks_and_leave_voiceover_alone() {
    use MediaRole::*;
    let links = Links {
        links: vec![
            link(
                "k1",
                "think it would help.",
                1,
                "vo/one.wav",
                Voiceover,
                None,
            ),
            link(
                "k2",
                "I'm a circuit racer.",
                5,
                "vo/two.wav",
                Voiceover,
                None,
            ),
            link(
                "k3",
                "think it would help.",
                1,
                "music/bed.wav",
                Music,
                None,
            ),
            link("k4", "almost made me quit.", 3, "sfx/hit.wav", Sfx, None),
        ],
        ..Links::default()
    };
    let media = BTreeMap::from([
        ("vo/one.wav".to_owned(), Some(90)),
        ("vo/two.wav".to_owned(), Some(60)),
        ("music/bed.wav".to_owned(), Some(1000)),
        ("sfx/hit.wav".to_owned(), Some(10)),
    ]);
    let (plan, issues) = compile(&spearfia_script::parse(SCRIPT), &links, &media, TB);
    let on = |track: &str| -> Vec<(u64, u64)> {
        plan.items
            .iter()
            .filter(|i| i.track == track)
            .map(|i| (i.start_frame, i.duration_frames))
            .collect()
    };
    // VO stays back to back on A1, untouched by the music and SFX links.
    assert_eq!(on("A1"), vec![(0, 90), (90, 60)]);
    // The bed runs from its words to the end of all VO, trimmed to nothing longer.
    assert_eq!(on("A2"), vec![(0, 150)]);
    // The hit plays once, at its words, for its own length, without a Short issue.
    let hit = on("A3");
    assert_eq!(hit.len(), 1);
    assert!(hit[0].0 > 0 && hit[0].0 < 90 && hit[0].1 == 10, "{hit:?}");
    // (Its transitions have no pictures to attach to here.)
    assert!(
        issues.iter().all(|i| matches!(i, Issue::Unmapped { .. })),
        "{issues:?}"
    );
}

#[test]
fn pictures_on_the_same_words_split_their_stretch() {
    use MediaRole::*;
    let links = Links {
        links: vec![
            link("k1", "think it would help.", 1, "vo/one.wav", Voiceover, None),
            link("k2", "almost made me quit.", 3, "broll/a.mp4", BRoll, None),
            link("k3", "almost made me quit.", 3, "broll/b.mp4", BRoll, None),
        ],
        next_id: 4,
        unreadable: Vec::new(),
    };
    let media = BTreeMap::from([
        ("vo/one.wav".to_string(), Some(300)),
        ("broll/a.mp4".to_string(), Some(1000)),
        ("broll/b.mp4".to_string(), Some(1000)),
    ]);
    let (plan, issues) = compile(&spearfia_script::parse(SCRIPT), &links, &media, TB);
    // With one voiceover link, its 300 frames cover all 14 words; "almost" is 6 in
    // (frame 128) and the 172 frames to the end split evenly instead of k2 covering k3.
    assert_eq!(
        plan.items[1..3],
        [
            clip_at("k2", "V2", 128, 86, "broll/a.mp4", 0),
            clip_at("k3", "V2", 214, 86, "broll/b.mp4", 0),
        ]
    );
    assert!(!issues.iter().any(|i| matches!(i, Issue::Empty { .. } | Issue::Short { .. })), "{issues:?}");
}

#[test]
fn heard_words_set_exact_times_and_unmatched_words_fill_between() {
    let heard = vec![("Hello,".into(), 0.0), ("word".into(), 0.5), ("again".into(), 1.0)];
    assert_eq!(align(&["hello", "world", "again"], &heard, 0, 60, TB), [0, 15, 30]);
    // Played from frame 6: file time 1.0 s (frame 30) is span frame 24.
    assert_eq!(align(&["hello", "world", "again"], &heard, 6, 60, TB), [0, 12, 24]);
    // Nothing heard: even spread to the end.
    assert_eq!(align(&["a", "b"], &[], 0, 60, TB), [20, 40]);
}

#[test]
fn heard_words_move_pictures_and_timing_reports_them() {
    use MediaRole::*;
    let links = Links {
        links: vec![
            link("k1", "think it would help.", 1, "vo/one.wav", Voiceover, None),
            link("k2", "almost made me quit.", 3, "broll/a.mp4", BRoll, None),
        ],
        next_id: 3,
        unreadable: Vec::new(),
    };
    let media = BTreeMap::from([
        ("vo/one.wav".to_string(), Some(300)),
        ("broll/a.mp4".to_string(), Some(1000)),
    ]);
    let script = spearfia_script::parse(SCRIPT);
    // "almost" heard at 8 s (frame 240) instead of the linear estimate (frame 128).
    let words = "think it would help and it almost made me quit I'm a circuit racer";
    let heard: Heard = BTreeMap::from([(
        "vo/one.wav".to_string(),
        words
            .split(' ')
            .enumerate()
            .map(|(i, w)| (w.to_string(), if i < 6 { i as f64 * 0.5 } else { 8.0 + (i - 6) as f64 * 0.25 }))
            .collect(),
    )]);
    let (plan, _) = compile_heard(&script, &links, &media, TB, &heard);
    assert_eq!(plan.items[1], clip_at("k2", "V2", 240, 60, "broll/a.mp4", 0));
    let t = Timing::new(&script, &links, &media, TB, &heard);
    assert_eq!(t.frame_of("almost made me quit.", 3), Some(240));
    assert_eq!(t.frame_of("ALMOST made me quit", 3), Some(240));
    let w = t.words();
    assert_eq!((w[6].text.as_str(), w[6].start, w[6].end), ("almost", 240, 248));
    assert_eq!(t.sections(&script).iter().map(|s| s.1).collect::<Vec<_>>(), [0, 270]);
    assert_eq!(t.end(), 300);
}
