//! Read-only channel context and approval-guarded project handoffs for agents.
use spearfia_core::{CoreError, app_state, channels, project::Project};
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

const HELP: &str = "Spear Fia\n\nspearfia channels list\nspearfia channels read <channel-id>\nspearfia project brief <project-folder>\nspearfia project handoff <project-folder>\nspearfia project status <project-folder>\nspearfia visuals read|cue-sheet|storyboard|prompts|handoff <project-folder>\n\nChannel commands use SPEARFIA_CONFIG_DIR or the normal app config folder.\nProduction handoff requires the creator's saved-script approval in the desktop app.\nSee docs/agents.md for the workflow; these commands do not run generators.\n";

fn version_line() -> String {
    format!("spearfia {}", env!("CARGO_PKG_VERSION"))
}

fn run(args: &[OsString], config: Option<&Path>) -> Result<String, CoreError> {
    let arg = |i: usize| args.get(i).and_then(|a| a.to_str());
    match (arg(0), arg(1), args.len()) {
        (None, _, 0) | (Some("--version"), _, 1) => Ok(version_line()),
        (Some("--help" | "-h"), _, 1) => Ok(HELP.into()),
        (Some("channels"), Some("list"), 2) => {
            let config = config.ok_or_else(|| {
                CoreError::Invalid(
                    "No app config folder is available; set SPEARFIA_CONFIG_DIR.".into(),
                )
            })?;
            let channels = channels::list(config)?;
            Ok(channels
                .iter()
                .map(|c| format!("{}\t{}\t{}", c.id, c.title, c.root.display()))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        (Some("channels"), Some("read"), 3) => {
            let config = config.ok_or_else(|| {
                CoreError::Invalid(
                    "No app config folder is available; set SPEARFIA_CONFIG_DIR.".into(),
                )
            })?;
            let id =
                arg(2).ok_or_else(|| CoreError::Invalid("The channel ID must be text.".into()))?;
            channels::load(config, id)?.brief()
        }
        (
            Some("visuals"),
            Some(action @ ("read" | "cue-sheet" | "storyboard" | "prompts" | "handoff")),
            3,
        ) => {
            let path = args.get(2).ok_or_else(|| CoreError::Invalid(HELP.into()))?;
            let project = Project::open(std::path::PathBuf::from(path))?;
            if action == "read" {
                project.read_visuals()
            } else {
                project.export_visuals(action)
            }
        }
        (Some("project"), Some(action @ ("brief" | "handoff" | "status")), 3) => {
            let path = args.get(2).ok_or_else(|| CoreError::Invalid(HELP.into()))?;
            let project = Project::open(std::path::PathBuf::from(path))?;
            if action == "status" {
                return Ok(project.script_approval(None)?.label().into());
            }
            project.agent_brief(action == "handoff", None)
        }
        _ => Err(CoreError::Invalid(format!(
            "Unknown or incomplete command.\n\n{HELP}"
        ))),
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match run(&args, app_state::config_dir().as_deref()) {
        Ok(output) => match writeln!(std::io::stdout().lock(), "{output}") {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
        Err(e) => {
            let _ = writeln!(std::io::stderr().lock(), "{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }
    #[test]
    fn version_line_names_the_binary() {
        assert!(run(&[], None).unwrap().starts_with("spearfia "));
    }
    #[test]
    fn unknown_missing_and_hostile_arguments_fail_without_panicking() {
        for words in [
            vec!["channels"],
            vec!["channels", "list", "extra"],
            vec!["project", "handoff"],
            vec!["project", "approve", "x"],
            vec!["channels", "read", "../outside"],
        ] {
            assert!(run(&args(&words), Some(&std::env::temp_dir())).is_err());
        }
        assert!(run(&args(&["channels", "list"]), None).is_err());
    }
    #[test]
    fn shared_channel_context_is_readable_through_cli() {
        let dir = std::env::temp_dir().join(format!(
            "spearfia-channel-cli-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        channels::create(&dir, "Science Channel").unwrap();
        assert!(
            run(&args(&["channels", "list"]), Some(&dir))
                .unwrap()
                .contains("science-channel")
        );
        let brief = run(&args(&["channels", "read", "science-channel"]), Some(&dir)).unwrap();
        assert!(
            brief.contains("lessons.md")
                && brief.contains("VoiceStudio")
                && brief.contains("creator's approval")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
