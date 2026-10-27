// SPDX-License-Identifier: MIT OR Apache-2.0
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
#[test]
fn cli_initialization_is_non_destructive_and_uses_shared_history() {
    let root = std::env::temp_dir().join(format!(
        "spearfia-cli-production-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("project.json"),
        r#"{"name":"Test","spec_version":1,"adapter":"filmcraft"}"#,
    )
    .unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_spearfia"))
            .arg("--project")
            .arg(&root)
            .args(args)
            .output()
            .unwrap()
    };
    let init = invoke(&["pipeline", "init"]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let config: serde_json::Value = serde_json::from_slice(&init.stdout).unwrap();
    assert_eq!(config["stages"].as_array().unwrap().len(), 11);
    let saved = fs::read(root.join("production/pipeline.json")).unwrap();
    assert!(!invoke(&["pipeline", "init"]).status.success());
    assert_eq!(
        fs::read(root.join("production/pipeline.json")).unwrap(),
        saved
    );
    let show = invoke(&["pipeline", "show"]);
    assert!(show.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&show.stdout).unwrap(),
        config
    );
    assert_eq!(
        String::from_utf8(invoke(&["pipeline", "status"]).stdout)
            .unwrap()
            .trim(),
        "null"
    );
    assert!(invoke(&["undo"]).status.success());
    assert!(
        fs::read(root.join("production/pipeline.json"))
            .unwrap()
            .is_empty()
    );
    assert!(invoke(&["redo"]).status.success());
    assert_eq!(
        fs::read(root.join("production/pipeline.json")).unwrap(),
        saved
    );
}
