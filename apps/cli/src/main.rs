//! `spearfia`: command-line interface. Calls the same command layer (`spearfia-ops`) as the
//! desktop app, so every change is checked against the file on disk and can be undone from
//! either.
//!
//! Every read prints the script's version; pass it back with `--expect` so a write fails
//! instead of landing on a script that changed in between. `--dry-run` shows the diff and
//! writes nothing.
//!
//! `mcp` and `api` serve the agent tools (see docs/agents.md); `channels`, `project` and
//! `visuals` are read-only. Production handoff needs the creator's approval in the desktop.

use clap::{Parser, Subcommand};
use spearfia_core::{app_state, channels, project::Project};
use spearfia_ops::{Doc, Origin, Plan, transitions};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "spearfia", version, about = "Spear Fia from the command line")]
struct Cli {
    /// Project folder (the one holding project.json). Default: the current folder.
    #[arg(long, global = true, default_value = ".")]
    project: PathBuf,
    /// Print JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List every cut with its transition.
    Cuts,
    /// List problems to fix (for now: section changes with no transition).
    Check,
    /// Change transitions.
    Transition {
        #[command(subcommand)]
        op: TransitionOp,
        /// Only apply if the script still has this version (from `cuts` or `check`).
        #[arg(long, global = true)]
        expect: Option<String>,
        /// Show the diff without writing anything.
        #[arg(long, global = true)]
        dry_run: bool,
    },
    /// Undo the most recent change made by the app or the CLI.
    Undo,
    /// List recorded changes, newest last.
    History,
    /// Serve the agent tools over MCP stdio for `--project`.
    Mcp,
    /// Serve the agent tools over authenticated loopback HTTP. Needs SPEARFIA_API_TOKEN.
    Api {
        /// 0 picks a free port and prints the URL to stderr.
        #[arg(long, default_value_t = 9877)]
        port: u16,
    },
    /// Read shared channel guides (SPEARFIA_CONFIG_DIR or the app config folder).
    Channels {
        #[command(subcommand)]
        op: ChannelOp,
    },
    /// Print the drafting brief, the approved production handoff, or approval status.
    #[command(name = "project")]
    ProjectInfo {
        #[arg(value_enum)]
        action: ProjectAction,
        /// Project folder; defaults to `--project`.
        folder: Option<PathBuf>,
    },
    /// Read or export the Visual Script.
    Visuals {
        #[arg(value_enum)]
        action: VisualsAction,
        /// Project folder; defaults to `--project`.
        folder: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ChannelOp {
    List,
    Read { id: String },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum ProjectAction {
    Brief,
    Handoff,
    Status,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum VisualsAction {
    Read,
    CueSheet,
    Storyboard,
    Prompts,
    Handoff,
}

#[derive(Subcommand)]
enum TransitionOp {
    /// Use a transition at a cut: `spearfia transition set 3 whip-pan`.
    Set { cut: usize, label: String },
    /// Make a cut a plain cut again.
    Clear { cut: usize },
    /// Set a transition's duration in seconds, or `none` to remove it.
    Duration { cut: usize, seconds: String },
    /// Use one transition at every section change that has none.
    Fill { label: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if cli.json {
                println!("{}", serde_json::json!({ "error": e }));
            } else {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<(), String> {
    let open = |folder: &Option<PathBuf>| {
        Project::open(folder.as_deref().unwrap_or(&cli.project)).map_err(|e| e.to_string())
    };
    match &cli.cmd {
        Cmd::Mcp => return serve_mcp(&cli.project),
        Cmd::Api { port } => return serve_api(&cli.project, *port),
        Cmd::Channels { op } => {
            let config = app_state::config_dir()
                .ok_or("No app config folder is available; set SPEARFIA_CONFIG_DIR.")?;
            println!("{}", read_channels(op, &config)?);
            return Ok(());
        }
        Cmd::ProjectInfo { action, folder } => {
            let project = open(folder)?;
            let text = match action {
                ProjectAction::Status => project.script_approval(None).map(|a| a.label().into()),
                a => project.agent_brief(matches!(a, ProjectAction::Handoff), None),
            };
            println!("{}", text.map_err(|e| e.to_string())?);
            return Ok(());
        }
        Cmd::Visuals { action, folder } => {
            let project = open(folder)?;
            let text = match action {
                VisualsAction::Read => project.read_visuals(),
                VisualsAction::CueSheet => project.export_visuals("cue-sheet"),
                VisualsAction::Storyboard => project.export_visuals("storyboard"),
                VisualsAction::Prompts => project.export_visuals("prompts"),
                VisualsAction::Handoff => project.export_visuals("handoff"),
            };
            println!("{}", text.map_err(|e| e.to_string())?);
            return Ok(());
        }
        _ => {}
    }
    let project = open(&None)?;
    let err = |e: spearfia_ops::OpError| e.to_string();
    match &cli.cmd {
        Cmd::Cuts => {
            let doc = Doc::main(&project).map_err(err)?;
            let cuts = transitions::list(&doc);
            if cli.json {
                print_json(serde_json::json!({ "version": doc.version, "cuts": cuts }));
            } else {
                println!("{} · {}", doc.rel, doc.version);
                for c in &cuts {
                    let mark = if c.unset { "  unset" } else { "" };
                    println!(
                        "{:>3}  {:>6.1}s  {:<14} {}{mark}",
                        c.cut,
                        c.at,
                        c.transition,
                        match (&c.from_section, &c.to_section, c.section_change) {
                            (Some(a), Some(b), true) => format!("{a} → {b}"),
                            _ => format!("\"{}\" → \"{}\"", short(&c.from_line), short(&c.to_line)),
                        }
                    );
                }
            }
        }
        Cmd::Check => {
            let doc = Doc::main(&project).map_err(err)?;
            let checks = transitions::checks(&doc);
            if cli.json {
                print_json(serde_json::json!({ "version": doc.version, "checks": checks }));
            } else if checks.is_empty() {
                println!("No problems found ({}).", doc.version);
            } else {
                for c in &checks {
                    println!("{} · cut {} · {}", c.stage, c.cut, c.message);
                }
            }
        }
        Cmd::Transition {
            op,
            expect,
            dry_run,
        } => {
            let doc = Doc::main(&project).map_err(err)?;
            doc.expect(expect.as_deref()).map_err(err)?;
            let plan = match op {
                TransitionOp::Set { cut, label } => transitions::set(&doc, *cut, label),
                TransitionOp::Clear { cut } => transitions::clear(&doc, *cut),
                TransitionOp::Duration { cut, seconds } => {
                    let s = match seconds.as_str() {
                        "none" | "off" => None,
                        v => Some(
                            v.parse::<f32>()
                                .map_err(|_| format!("\"{v}\" isn't a number of seconds"))?,
                        ),
                    };
                    transitions::set_duration(&doc, *cut, s)
                }
                TransitionOp::Fill { label } => transitions::fill_unset(&doc, label),
            }
            .map_err(err)?;
            write(cli, &project, &plan, *dry_run)?;
        }
        Cmd::Undo => {
            let e = spearfia_ops::undo(&project).map_err(err)?;
            if cli.json {
                print_json(serde_json::json!({ "undone": e.summary, "doc": e.doc }));
            } else {
                println!("Undid: {}", e.summary);
            }
        }
        Cmd::History => {
            let log = spearfia_ops::history(&project).map_err(err)?;
            if cli.json {
                let items: Vec<_> = log
                    .iter()
                    .map(|e| serde_json::json!({ "summary": e.summary, "origin": e.origin, "doc": e.doc, "at": e.at }))
                    .collect();
                print_json(serde_json::json!({ "history": items }));
            } else {
                for e in &log {
                    println!("{:?}\t{}\t{}", e.origin, e.doc, e.summary);
                }
            }
        }
        Cmd::Mcp | Cmd::Api { .. } | Cmd::Channels { .. } | Cmd::ProjectInfo { .. } => {}
        Cmd::Visuals { .. } => {}
    }
    Ok(())
}

fn read_channels(op: &ChannelOp, config: &Path) -> Result<String, String> {
    match op {
        ChannelOp::List => Ok(channels::list(config)
            .map_err(|e| e.to_string())?
            .iter()
            .map(|c| format!("{}\t{}\t{}", c.id, c.title, c.root.display()))
            .collect::<Vec<_>>()
            .join("\n")),
        ChannelOp::Read { id } => channels::load(config, id)
            .and_then(|c| c.brief())
            .map_err(|e| e.to_string()),
    }
}

fn serve_mcp(root: &Path) -> Result<(), String> {
    let mut server = spearfia_mcp::Server::open(root).map_err(|e| e.to_string())?;
    match spearfia_mcp::serve_stdio(
        &mut server,
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    ) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

fn serve_api(root: &Path, port: u16) -> Result<(), String> {
    let token = std::env::var("SPEARFIA_API_TOKEN").map_err(|_| "Set SPEARFIA_API_TOKEN to a random secret of at least 32 characters before starting the local API.".to_owned())?;
    if token.len() < 32 || token.len() > 256 || !token.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("Use 32–256 random printable ASCII characters for SPEARFIA_API_TOKEN.".into());
    }
    let mut server = spearfia_mcp::Server::open(root).map_err(|e| e.to_string())?;
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    eprintln!(
        "Spear Fia API listening at http://{}/v1/tools; bearer authentication required.",
        listener.local_addr().map_err(|e| e.to_string())?
    );
    spearfia_mcp::http::serve(&mut server, listener, &token).map_err(|e| e.to_string())
}

/// Show or apply a plan.
fn write(cli: &Cli, project: &Project, plan: &Plan, dry_run: bool) -> Result<(), String> {
    if dry_run || !plan.changes() {
        if cli.json {
            print_json(serde_json::json!({
                "summary": plan.summary, "diff": plan.diff(), "version": plan.base, "applied": false
            }));
        } else {
            println!("{} (dry run, from {})", plan.summary, plan.base);
            print!("{}", plan.diff());
        }
        return Ok(());
    }
    let v = spearfia_ops::apply(project, plan, Origin::Cli).map_err(|e| e.to_string())?;
    if cli.json {
        print_json(serde_json::json!({
            "summary": plan.summary, "diff": plan.diff(), "version": v, "applied": true
        }));
    } else {
        println!("{} · now {v} · undo with `spearfia undo`", plan.summary);
    }
    Ok(())
}

fn print_json(v: serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
}

fn short(s: &str) -> String {
    if s.chars().count() <= 28 {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(27).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_commands() {
        for args in [
            "spearfia cuts --json",
            "spearfia --project p transition set 3 whip-pan --expect v1 --dry-run",
            "spearfia transition duration 2 none",
            "spearfia transition fill whip-pan",
            "spearfia undo",
            "spearfia mcp --project p",
            "spearfia api --project p --port 0",
            "spearfia channels list",
            "spearfia channels read science-channel",
            "spearfia project handoff p",
            "spearfia project status",
            "spearfia visuals cue-sheet p",
        ] {
            Cli::try_parse_from(args.split(' ')).unwrap_or_else(|e| panic!("{args}: {e}"));
        }
        for args in [
            "spearfia channels",
            "spearfia project approve p",
            "spearfia visuals render p",
        ] {
            assert!(Cli::try_parse_from(args.split(' ')).is_err(), "{args}");
        }
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
            read_channels(&ChannelOp::List, &dir)
                .unwrap()
                .contains("science-channel")
        );
        let brief = read_channels(
            &ChannelOp::Read {
                id: "science-channel".into(),
            },
            &dir,
        )
        .unwrap();
        assert!(
            brief.contains("lessons.md")
                && brief.contains("VoiceStudio")
                && brief.contains("creator's approval")
        );
        assert!(
            read_channels(
                &ChannelOp::Read {
                    id: "../outside".into()
                },
                &dir
            )
            .is_err()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
