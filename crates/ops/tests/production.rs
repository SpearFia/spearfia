// SPDX-License-Identifier: MIT OR Apache-2.0
use spearfia_core::project::Project;
use spearfia_ops::{
    Origin,
    production::{self, Action, Agent, Check, Pipeline, Provider, Stage, State},
};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

fn dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "spearfia-production-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn fixture() -> &'static PathBuf {
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| {
        let output = dir("fixture").join(format!("mock-agent{}", std::env::consts::EXE_SUFFIX));
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent.rs");
        assert!(
            Command::new("rustc")
                .arg(source)
                .arg("-o")
                .arg(&output)
                .status()
                .unwrap()
                .success()
        );
        output
    })
}
fn project() -> Project {
    let root = dir("project with spaces");
    fs::create_dir_all(root.join("script")).unwrap();
    fs::write(root.join("project.json"), r#"{"name":"Test","spec_version":1,"adapter":"filmcraft","library_roots":[],"script":"script/script.md","vision":"vision.yaml","settings":{"timebase":{"num":30,"den":1},"resolution":[1280,720],"sample_rate":48000}}"#).unwrap();
    fs::write(
        root.join("script/script.md"),
        "## First\nVO: One two three.\n",
    )
    .unwrap();
    fs::write(root.join("vision.yaml"), "name: test\n").unwrap();
    let p = Project::open(root).unwrap();
    p.set_channel_brief(None, "# Test channel\nMake useful videos.")
        .unwrap();
    p
}
fn stage(id: &str, provider: Provider, production: bool, review: bool, prompt: &str) -> Stage {
    let mut agent = Agent::new(provider);
    agent.executable = fixture().to_string_lossy().into_owned();
    Stage {
        id: id.into(),
        title: id.into(),
        agent,
        prompt: prompt.into(),
        production,
        review,
        checks: vec![Check::File {
            path: "production/out.md".into(),
        }],
    }
}
fn configure(p: &Project, stages: Vec<Stage>) {
    let mut config = production::defaults();
    config.stages = stages;
    production::save_pipeline(p, &config, &spearfia_ops::version(""), Origin::Cli).unwrap();
}
fn start(p: &Project) -> production::Run {
    production::execute(
        p,
        fixture(),
        Action {
            start: true,
            ..Default::default()
        },
    )
    .unwrap()
}
fn resume(p: &Project, action: Action) -> production::Run {
    production::execute(p, fixture(), action).unwrap()
}

#[test]
fn gates_survive_restarts_and_one_acknowledgement_accepts_one_gate() {
    let p = project();
    configure(
        &p,
        vec![
            stage("one", Provider::Codex, false, true, "FAKE_SUCCESS"),
            stage("two", Provider::Claude, false, true, "FAKE_SUCCESS"),
        ],
    );
    let run = start(&p);
    assert_eq!(run.state, State::Review);
    assert_eq!(run.steps[1].attempts, 0);
    assert_eq!(
        production::load_run(&p).unwrap().unwrap().steps[0].state,
        State::Review
    );
    let run = resume(
        &p,
        Action {
            accept_review: true,
            reply: "Choose idea two".into(),
            ..Default::default()
        },
    );
    assert_eq!(run.steps[0].state, State::Complete);
    assert_eq!(run.steps[1].state, State::Review);
    assert_eq!(run.steps[0].reply, "Choose idea two");
    let run = resume(
        &p,
        Action {
            accept_review: true,
            ..Default::default()
        },
    );
    assert_eq!(run.state, State::Complete);
    assert_eq!(run.steps[0].attempts, 1);
}
#[test]
fn production_waits_for_real_script_approval() {
    let p = project();
    configure(
        &p,
        vec![stage("one", Provider::Codex, true, false, "FAKE_SUCCESS")],
    );
    let run = start(&p);
    assert_eq!(run.state, State::Review);
    assert_eq!(run.steps[0].attempts, 0);
    p.approve_script(&p.read_script().unwrap()).unwrap();
    assert_eq!(resume(&p, Action::default()).state, State::Complete);
    fs::write(p.script_path(), "## Changed\nVO: Different narration.\n").unwrap();
    assert_eq!(resume(&p, Action::default()).state, State::Failed);
}
#[test]
fn missing_artifact_defeats_agent_success_and_retry_is_explicit() {
    let p = project();
    configure(
        &p,
        vec![stage("one", Provider::Codex, false, false, "FAKE_MISSING")],
    );
    let run = start(&p);
    assert_eq!(run.state, State::Failed);
    assert!(run.message.contains("verification"));
    assert!(production::execute(&p, fixture(), Action::default()).is_err());
}
#[test]
fn agent_question_resumes_same_stage_with_saved_reply() {
    let p = project();
    configure(
        &p,
        vec![stage(
            "one",
            Provider::Claude,
            false,
            false,
            "FAKE_QUESTION",
        )],
    );
    assert_eq!(start(&p).state, State::NeedsInput);
    assert!(production::execute(&p, fixture(), Action::default()).is_err());
    let run = resume(
        &p,
        Action {
            reply: "yes".into(),
            ..Default::default()
        },
    );
    assert_eq!(run.state, State::Complete);
    assert_eq!(run.steps[0].attempts, 2);
}
#[test]
fn provider_errors_and_permission_denials_outrank_success() {
    for (provider, prompt, expected) in [
        (Provider::Codex, "FAKE_PROVIDER_ERROR", State::Failed),
        (Provider::Codex, "FAKE_FAIL", State::Failed),
        (Provider::Claude, "FAKE_DENIAL", State::NeedsInput),
    ] {
        let p = project();
        configure(&p, vec![stage("one", provider, false, false, prompt)]);
        assert_eq!(start(&p).state, expected);
    }
}
#[test]
fn changed_artifacts_invalidate_successors_without_rerunning() {
    let p = project();
    configure(
        &p,
        vec![
            stage("one", Provider::Codex, false, false, "FAKE_SUCCESS"),
            stage("two", Provider::Codex, false, true, "FAKE_SUCCESS"),
        ],
    );
    start(&p);
    fs::write(p.root.join("production/out.md"), "changed").unwrap();
    let run = resume(
        &p,
        Action {
            accept_review: true,
            ..Default::default()
        },
    );
    assert_eq!(run.steps[0].state, State::Failed);
    assert_eq!(run.steps[1].state, State::Pending);
    assert_eq!(run.steps[0].attempts, 1);
    assert_eq!(
        resume(
            &p,
            Action {
                retry: true,
                ..Default::default()
            }
        )
        .state,
        State::Review
    );
}
#[test]
fn pipeline_changes_require_a_new_run_and_stale_config_saves_fail() {
    let p = project();
    configure(
        &p,
        vec![stage("one", Provider::Codex, false, true, "FAKE_SUCCESS")],
    );
    start(&p);
    let (mut config, version) = production::load_pipeline(&p).unwrap();
    config.stages[0].agent.model = "explicit-model".into();
    production::save_pipeline(&p, &config, &version, Origin::App).unwrap();
    assert!(production::save_pipeline(&p, &config, &version, Origin::App).is_err());
    assert!(
        production::execute(
            &p,
            fixture(),
            Action {
                accept_review: true,
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[test]
fn later_verified_document_refinement_does_not_restart_earlier_stages() {
    let p = project();
    configure(
        &p,
        vec![
            stage("draft", Provider::Codex, false, false, "FAKE_SUCCESS"),
            stage("refine", Provider::Codex, false, true, "FAKE_REVISED"),
        ],
    );
    let review = start(&p);
    assert_eq!(review.state, State::Review);
    assert_eq!(review.steps[0].artifacts, review.steps[1].artifacts);
    let complete = resume(
        &p,
        Action {
            accept_review: true,
            ..Default::default()
        },
    );
    assert_eq!(complete.state, State::Complete);
    assert!(complete.steps.iter().all(|s| s.attempts == 1));
}
#[test]
fn interrupted_running_state_never_silently_restarts() {
    let p = project();
    configure(
        &p,
        vec![stage("one", Provider::Codex, false, true, "FAKE_SUCCESS")],
    );
    let mut run = start(&p);
    run.steps[0].state = State::Running;
    run.state = State::Running;
    fs::write(
        p.root.join("sync/production/run.json"),
        serde_json::to_string(&run).unwrap(),
    )
    .unwrap();
    let recovered = resume(&p, Action::default());
    assert_eq!(recovered.state, State::Failed);
    assert_eq!(recovered.steps[0].attempts, 1);
}
#[test]
fn run_lock_excludes_other_coordinators() {
    let p = project();
    configure(
        &p,
        vec![stage("one", Provider::Codex, false, false, "FAKE_SUCCESS")],
    );
    fs::create_dir_all(p.root.join("sync/production")).unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(p.root.join("sync/production/run.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(
        production::execute(
            &p,
            fixture(),
            Action {
                start: true,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn timeout_and_stop_save_recoverable_failures() {
    let p = project();
    let mut s = stage("one", Provider::Codex, false, false, "FAKE_WAIT");
    s.agent.timeout_seconds = 1;
    configure(&p, vec![s]);
    assert_eq!(start(&p).state, State::Failed);
    let p = project();
    configure(
        &p,
        vec![stage("one", Provider::Codex, false, false, "FAKE_WAIT")],
    );
    let copy = p.clone();
    let thread = std::thread::spawn(move || start(&copy));
    for _ in 0..100 {
        if production::load_run(&p)
            .unwrap()
            .is_some_and(|r| r.state == State::Running)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    production::request_stop(&p).unwrap();
    assert_eq!(thread.join().unwrap().state, State::Cancelled);
}
#[test]
fn configuration_refuses_duplicate_ids_unsafe_outputs_and_late_planning() {
    let mut c = production::defaults();
    assert!(c.validate().is_ok());
    c.stages[1].id = c.stages[0].id.clone();
    assert!(c.validate().is_err());
    let mut c = production::defaults();
    c.stages[0].checks = vec![Check::File {
        path: "sync/approved-script.md".into(),
    }];
    assert!(c.validate().is_err());
    let mut c = production::defaults();
    c.stages[0].production = true;
    assert!(c.validate().is_err());
    let p = project();
    let c: Pipeline = production::defaults();
    assert!(production::verify(&p, &c.stages[2]).is_ok());
    fs::write(p.script_path(), "## Empty\n").unwrap();
    assert!(production::verify(&p, &c.stages[2]).is_err());
}

#[test]
fn agent_undo_cannot_change_creator_assignments() {
    let p = project();
    configure(
        &p,
        vec![stage("one", Provider::Codex, false, false, "FAKE_SUCCESS")],
    );
    assert!(spearfia_ops::agent_undo(&p, false).is_err());
    spearfia_ops::undo(&p).unwrap();
    assert!(spearfia_ops::agent_undo(&p, true).is_err());
    spearfia_ops::redo(&p).unwrap();
    assert!(production::load_pipeline(&p).is_ok());
}

#[test]
fn assembly_requires_a_new_review_even_when_old_artifacts_are_valid() {
    let p = project();
    fs::write(p.root.join("links.json"), "{\"links\":[]}").unwrap();
    for dir in ["sync/plans", "sync/snapshots"] {
        fs::create_dir_all(p.root.join(dir)).unwrap();
    }
    fs::write(p.root.join("sync/review-plan-1.fcproj"), "existing review").unwrap();
    fs::write(
        p.root.join("sync/plans/plan-1.json"),
        r#"{"plan_id":"plan-1","items":[{"kind":"clip"}]}"#,
    )
    .unwrap();
    fs::write(p.root.join("sync/snapshots/plan-1.json"), "{}").unwrap();
    let mut s = stage("assembly", Provider::Codex, false, false, "FAKE_SUCCESS");
    s.checks = vec![Check::Filmcraft];
    assert!(production::verify(&p, &s).is_ok());
    configure(&p, vec![s]);
    let run = start(&p);
    assert_eq!(run.state, State::Failed);
    assert!(run.message.contains("new FilmCraft"));
}

#[test]
fn voiceover_checks_measure_audio_cover_narration_and_preserve_prior_links() {
    use spearfia_core::{
        links::{Link, LinkKind, LinkMedia, Links},
        project::MediaRole,
    };
    let p = project();
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend(16036u32.to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(8000u32.to_le_bytes());
    wav.extend(16000u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(16000u32.to_le_bytes());
    wav.resize(16044, 0);
    fs::write(p.root.join("voice.wav"), &wav).unwrap();
    let mut links = Links::default();
    links.add(Link {
        id: String::new(),
        quote: "One two three.".into(),
        line: 1,
        media: LinkMedia {
            path: "voice.wav".into(),
            hash: None,
        },
        role: MediaRole::Voiceover,
        kind: LinkKind::Audio,
        in_point: None,
        out_point: None,
    });
    links.save(&p).unwrap();
    let mut s = stage("voiceover", Provider::Codex, false, false, "FAKE_SUCCESS");
    s.checks = vec![Check::Links {
        role: MediaRole::Voiceover,
        minimum: 1,
        coverage: true,
    }];
    assert!(production::verify(&p, &s).is_ok());
    configure(&p, vec![s.clone()]);
    assert_eq!(start(&p).state, State::Complete);
    links.add(Link {
        id: String::new(),
        quote: "One two three.".into(),
        line: 1,
        media: LinkMedia {
            path: "voice.wav".into(),
            hash: None,
        },
        role: MediaRole::Music,
        kind: LinkKind::Audio,
        in_point: None,
        out_point: None,
    });
    links.save(&p).unwrap();
    assert_eq!(resume(&p, Action::default()).state, State::Complete);
    fs::write(
        p.script_path(),
        "## First\nVO: One two three.\n## Second\nVO: Four five.\n",
    )
    .unwrap();
    assert!(production::verify(&p, &s).is_err());
    fs::write(
        p.script_path(),
        "## First\nVO: One two three.\nLIVE: A camera line.\n",
    )
    .unwrap();
    assert!(production::verify(&p, &s).is_ok());
    links.links[0].out_point = Some("999".into());
    links.save(&p).unwrap();
    assert!(production::verify(&p, &s).is_err());
}

#[test]
fn revoking_approval_stops_an_active_production_agent() {
    let p = project();
    configure(
        &p,
        vec![stage("one", Provider::Codex, true, false, "FAKE_WAIT")],
    );
    p.approve_script(&p.read_script().unwrap()).unwrap();
    let copy = p.clone();
    let thread = std::thread::spawn(move || start(&copy));
    for _ in 0..100 {
        if production::load_run(&p)
            .unwrap()
            .is_some_and(|r| r.state == State::Running)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    p.revoke_script_approval().unwrap();
    let run = thread.join().unwrap();
    assert_eq!(run.state, State::Failed);
    assert!(run.message.contains("approval"));
    assert!(!production::is_running(&p).unwrap());
}
