//! CLI, MCP stdio and local HTTP entry points backed by shared core operations.
use spearfia_core::{CoreError, app_state, channels, project::Project};
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

const HELP: &str = "Spear Fia\n\nspearfia mcp --project <project-folder>\nspearfia api --project <project-folder> [--port <port>]\nspearfia channels list\nspearfia channels read <channel-id>\nspearfia project brief <project-folder>\nspearfia project handoff <project-folder>\nspearfia project status <project-folder>\nspearfia push filmcraft <project-folder>\nspearfia visuals read|cue-sheet|storyboard|prompts|handoff <project-folder>\n\nChannel commands use SPEARFIA_CONFIG_DIR or the normal app config folder.\nProduction handoff and push require the creator's saved-script approval in the desktop app.\nSee docs/agents.md for the workflow; these commands do not run generators.\n";

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
        (Some("push"), Some("filmcraft"), 3) => {
            let path = args.get(2).ok_or_else(|| CoreError::Invalid(HELP.into()))?;
            let project = Project::open(std::path::PathBuf::from(path))?;
            let cli = spearfia_adapter_filmcraft::find_cli()?;
            let pushed = spearfia_adapter_filmcraft::push(&project, &cli)?;
            serde_json::to_string_pretty(&pushed.summary())
                .map_err(|e| CoreError::Invalid(e.to_string()))
        }
        _ => Err(CoreError::Invalid(format!(
            "Unknown or incomplete command.\n\n{HELP}"
        ))),
    }
}

fn serve(args: &[OsString]) -> Result<(), String> {
    let arg = |i: usize| args.get(i).and_then(|a| a.to_str());
    if arg(1) != Some("--project") || !matches!(args.len(), 3 | 5) {
        return Err("Use spearfia mcp --project <folder> or spearfia api --project <folder> [--port <port>].".into());
    }
    let root = args
        .get(2)
        .ok_or_else(|| "Project folder is required.".to_owned())?;
    let mut server = spearfia_mcp::Server::open(Path::new(root)).map_err(|e| e.to_string())?;
    if arg(0) == Some("mcp") {
        if args.len() != 3 {
            return Err("MCP stdio takes only --project <folder>.".into());
        }
        match spearfia_mcp::serve_stdio(
            &mut server,
            std::io::stdin().lock(),
            std::io::stdout().lock(),
        ) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    } else {
        let port = if args.len() == 3 {
            9877
        } else if arg(3) == Some("--port") {
            arg(4)
                .and_then(|p| p.parse::<u16>().ok())
                .ok_or_else(|| "Use a port from 0 through 65535.".to_owned())?
        } else {
            return Err("Only --port <port> is supported.".into());
        };
        let token = std::env::var("SPEARFIA_API_TOKEN").map_err(|_| "Set SPEARFIA_API_TOKEN to a random secret of at least 32 characters before starting the local API.".to_owned())?;
        if token.len() < 32 || token.len() > 256 || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(
                "Use 32–256 random printable ASCII characters for SPEARFIA_API_TOKEN.".into(),
            );
        }
        let listener =
            std::net::TcpListener::bind(("127.0.0.1", port)).map_err(|e| e.to_string())?;
        eprintln!(
            "Spear Fia API listening at http://{}/v1/tools; bearer authentication required.",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        spearfia_mcp::http::serve(&mut server, listener, &token).map_err(|e| e.to_string())
    }
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "mcp" || a == "api") {
        return match serve(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                let _ = writeln!(std::io::stderr().lock(), "{error}");
                ExitCode::FAILURE
            }
        };
    }
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
            vec!["push", "filmcraft"],
            vec!["push", "resolve", "x"],
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
