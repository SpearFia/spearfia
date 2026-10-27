// SPDX-License-Identifier: MIT OR Apache-2.0
use spearfia_core::{links::Links, project::Project};
use spearfia_ops::{
    Origin,
    production::{jobs::*, planning},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
fn project() -> Project {
    let root = std::env::temp_dir().join(format!(
        "spearfia-jobs-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("script")).unwrap();
    fs::write(root.join("project.json"),r#"{"name":"Test","spec_version":1,"adapter":"filmcraft","script":"script/script.md","media_folders":[{"path":"A-roll/voiceover","role":"voiceover"},{"path":"Music","role":"music"}],"settings":{"timebase":{"num":30,"den":1},"resolution":[1280,720],"sample_rate":48000}}"#).unwrap();
    fs::write(
        root.join("script/script.md"),
        "## First\nVO: One two three.\n",
    )
    .unwrap();
    let p = Project::open(root).unwrap();
    p.set_channel_brief(None, "# Test channel\nOriginal educational videos.")
        .unwrap();
    p.approve_script(&p.read_script().unwrap()).unwrap();
    p
}
fn out(path: &str, kind: &str) -> Output {
    Output {
        path: path.into(),
        kind: kind.into(),
        role: None,
        anchor: None,
        node: None,
        index: 0,
    }
}
fn command(id: &str, deps: &[&str], input: Vec<String>, code: &str) -> Job {
    Job {
        id: id.into(),
        title: id.into(),
        stage: "test".into(),
        depends_on: deps.iter().map(|s| s.to_string()).collect(),
        inputs: input,
        checks: vec![],
        task: Task::Command {
            program: "python".into(),
            args: vec!["-c".into(), code.into(), "{output0}".into()],
            outputs: vec![out(&format!("production/{id}-{{key}}.txt"), "text")],
        },
    }
}
fn save(p: &Project, jobs: Vec<Job>) {
    let current = spearfia_ops::read(p, PLAN).unwrap();
    save_plan(
        p,
        &Plan {
            spec_version: 1,
            jobs,
        },
        &spearfia_ops::version(&current),
        Origin::Cli,
    )
    .unwrap();
}
const WRITE: &str = "import pathlib,sys;p=pathlib.Path(sys.argv[1]);p.parent.mkdir(parents=True,exist_ok=True);p.write_text('verified')";
#[test]
fn dependency_order_reuse_and_changed_sources() {
    let p = project();
    fs::write(p.root.join("input.txt"), "first").unwrap();
    let a = command("source", &[], vec!["input.txt".into()], WRITE);
    let b = command("dependent", &["source"], vec!["job:source:0".into()], WRITE);
    save(&p, vec![b, a]);
    let run = run(&p, Path::new("unused"), false, "").unwrap();
    assert!(run.jobs.iter().all(|r| r.state == Status::Complete));
    let again = run_stage(&p, Path::new("unused"), false, "", None).unwrap();
    assert!(again.jobs.iter().all(|r| r.attempts == 1));
    fs::write(p.root.join("input.txt"), "changed").unwrap();
    let again = run_stage(&p, Path::new("unused"), false, "", None).unwrap();
    assert!(
        again
            .jobs
            .iter()
            .all(|r| r.state == Status::Complete && r.attempts == 2)
    );
}
#[test]
fn failure_does_not_stop_unrelated_jobs_and_cycles_are_rejected() {
    let p = project();
    save(
        &p,
        vec![
            command("broken", &[], vec!["missing.txt".into()], WRITE),
            command("okay", &[], vec![], WRITE),
        ],
    );
    let state = run(&p, Path::new("unused"), false, "").unwrap();
    assert_eq!(state.jobs[0].state, Status::Failed);
    assert_eq!(state.jobs[1].state, Status::Complete);
    let plan = Plan {
        spec_version: 1,
        jobs: vec![
            command("a", &["b"], vec![], WRITE),
            command("b", &["a"], vec![], WRITE),
        ],
    };
    assert!(plan.validate().is_err());
}
struct Service(Child);
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn service(p: &Project) -> (Service, String) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/services.py");
    let mut cmd = Command::new("python");
    cmd.arg(source).arg(&p.root);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let child = Service(cmd.spawn().unwrap());
    for _ in 0..200 {
        if let Ok(port) = fs::read_to_string(p.root.join("service-port")) {
            return (child, format!("http://127.0.0.1:{}", port.trim()));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("Mock service did not start");
}
fn counts(p: &Project) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(p.root.join("service-counts.json")).unwrap()).unwrap()
}
#[test]
fn comfy_id_recovers_without_submission_and_repairs_removed_links() {
    let p = project();
    let (_service, url) = service(&p);
    let mut output = out("Music/bed-{key}.wav", "audio");
    output.role = Some(spearfia_core::project::MediaRole::Music);
    output.anchor = Some(Anchor {
        line: 1,
        quote: "One two".into(),
    });
    output.node = Some("7".into());
    save(
        &p,
        vec![Job {
            id: "bed".into(),
            title: "Bed".into(),
            stage: "test".into(),
            depends_on: vec![],
            inputs: vec![],
            checks: vec![],
            task: Task::Comfyui {
                url,
                workflow: serde_json::json!({"7":{"class_type":"SaveAudio","inputs":{}}}),
                outputs: vec![output],
            },
        }],
    );
    let first = run(&p, Path::new("unused"), false, "").unwrap();
    assert_eq!(
        first.jobs[0].state,
        Status::Complete,
        "{}",
        first.jobs[0].message
    );
    assert_eq!(counts(&p)["prompt"], 1);
    let link = first.jobs[0].link_ids[0].clone();
    spearfia_ops::links::remove(&p, &link, Origin::Cli).unwrap();
    let repaired = run(&p, Path::new("unused"), false, "").unwrap();
    assert_eq!(repaired.jobs[0].attempts, 1);
    assert_eq!(Links::load(&p).unwrap().links.len(), 1);
    assert_eq!(counts(&p)["prompt"], 1);
    let mut saved = repaired;
    saved.jobs[0].state = Status::Running;
    fs::remove_file(p.resolve(&saved.jobs[0].outputs[0].path)).unwrap();
    spearfia_core::project::write_json(&p.root.join("sync/production/jobs.json"), &saved).unwrap();
    let recovered = run(&p, Path::new("unused"), false, "").unwrap();
    assert_eq!(
        recovered.jobs[0].state,
        Status::Complete,
        "{}",
        recovered.jobs[0].message
    );
    assert_eq!(counts(&p)["prompt"], 1);
}
#[test]
fn voiceover_and_measured_alignment_use_real_contracts() {
    let p = project();
    let (_service, url) = service(&p);
    let settings = planning::VoiceSettings {
        url: format!("{url}/voice"),
        profile_id: "explicit-test-profile".into(),
        instruct: "neutral".into(),
        language: "en".into(),
        speed: 1.0,
        steps: 16,
    };
    let plan = planning::voiceover(&p, &settings).unwrap();
    save_plan(&p, &plan, &spearfia_ops::version(""), Origin::Cli).unwrap();
    let state = run_stage(&p, Path::new("unused"), false, "", Some("voiceover-sample")).unwrap();
    assert_eq!(
        state.jobs[0].state,
        Status::Complete,
        "{}",
        state.jobs[0].message
    );
    let plan = planning::alignment(&p, &settings.url, "en").unwrap();
    let current = load_plan(&p).unwrap().1;
    save_plan(&p, &plan, &current, Origin::Cli).unwrap();
    let state = run_stage(&p, Path::new("unused"), false, "", Some("word-alignment")).unwrap();
    assert!(
        state.jobs.iter().all(|r| r.state == Status::Complete),
        "{state:?}"
    );
    let words =
        spearfia_analysis::timeline_words(&p, &Links::load(&p).unwrap(), p.file.settings.timebase)
            .unwrap();
    assert_eq!(words.len(), 3);
    assert_eq!(words[1].start_frame, 12);
    assert_eq!(counts(&p)["generate"], 1);
}

#[test]
fn uncertain_voice_requests_never_resubmit_and_definitions_are_protected() {
    let p = project();
    let (_server, url) = service(&p);
    let settings = planning::VoiceSettings {
        url: format!("{url}/voice"),
        profile_id: "test".into(),
        instruct: "".into(),
        language: "en".into(),
        speed: 1.0,
        steps: 16,
    };
    let plan = planning::voiceover(&p, &settings).unwrap();
    save_plan(&p, &plan, &spearfia_ops::version(""), Origin::Cli).unwrap();
    let mut first = run(&p, Path::new("unused"), false, "").unwrap();
    assert_eq!(first.jobs[0].state, Status::Complete);
    first.jobs[0].state = Status::Uncertain;
    first.jobs[0].provider_id.clear();
    first.jobs[0].seed += 1;
    spearfia_core::project::write_json(&p.root.join("sync/production/jobs.json"), &first).unwrap();
    let again = run(&p, Path::new("unused"), true, "").unwrap();
    assert_eq!(again.jobs[0].state, Status::Uncertain);
    assert_eq!(counts(&p)["generate"], 1);
    let mut changed = plan.clone();
    changed.jobs[0].title = "Changed definition".into();
    assert!(save_plan(&p, &changed, &load_plan(&p).unwrap().1, Origin::Agent).is_err());
    retry_uncertain(&p, &again.jobs[0].id).unwrap();
    let resolved = run(&p, Path::new("unused"), true, "").unwrap();
    assert_eq!(resolved.jobs[0].state, Status::Complete);
    assert_eq!(
        counts(&p)["generate"],
        1,
        "A verified download should finish locally without another generation"
    );
}
#[test]
fn cancellation_persists_and_a_second_runner_is_refused() {
    let p = project();
    save(
        &p,
        vec![command("slow", &[], vec![], "import time;time.sleep(20)")],
    );
    let active = p.clone();
    let worker = std::thread::spawn(move || run(&active, Path::new("unused"), false, "").unwrap());
    for _ in 0..200 {
        if ledger(&p)
            .unwrap()
            .jobs
            .iter()
            .any(|r| r.state == Status::Running)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(is_running(&p).unwrap());
    assert!(run(&p, Path::new("unused"), false, "").is_err());
    cancel(&p).unwrap();
    let stopped = worker.join().unwrap();
    assert_eq!(stopped.jobs[0].state, Status::Cancelled);
    assert!(!is_running(&p).unwrap());
}
#[test]
fn unapproved_scripts_cannot_start_artifact_work() {
    let p = project();
    save(&p, vec![command("work", &[], vec![], WRITE)]);
    fs::write(p.root.join("script/script.md"), "Changed narration.\n").unwrap();
    assert!(run(&p, Path::new("unused"), false, "").is_err());
    assert!(ledger(&p).unwrap().jobs.is_empty());
}
