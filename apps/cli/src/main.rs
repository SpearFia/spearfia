//! `spearfia`: command-line interface. Calls the same command layer (`spearfia-ops`) as the
//! desktop app, so every change is checked against the file on disk and can be undone from
//! either.
//!
//! Every read prints the script's version; pass it back with `--expect` so a write fails
//! instead of landing on a script that changed in between. `--dry-run` shows the diff and
//! writes nothing.
//!
//! `mcp` and `api` serve the agent tools (see docs/agents.md); `channels`, `project` and
//! `visuals` are read-only; `push filmcraft` writes only new files under `sync/`. Production handoff needs the creator's approval in the desktop.

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
    /// Plan, run, inspect and recover production artifact jobs.
    Jobs {
        #[command(subcommand)]
        op: JobsOp,
    },
    /// Validate or save channel-specific executable style.
    Style {
        #[command(subcommand)]
        op: StyleOp,
    },
    /// Build animated-caption instructions from measured word timing.
    Captions,
    /// Configure and run the saved CLI-agent production workflow.
    Pipeline {
        #[command(subcommand)]
        op: PipelineOp,
    },
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
    /// Undo the most recent change made by the app, the CLI or an agent.
    Undo,
    /// Reapply the most recently undone change.
    Redo,
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
    /// Build the linked media into a new FilmCraft review project. Needs the creator's
    /// approval of the saved script; prints the review path, counts and issues as JSON.
    Push {
        #[arg(value_enum)]
        target: PushTarget,
        /// Project folder; defaults to `--project`.
        folder: Option<PathBuf>,
    },
    /// List the script's `##` sections with their VO lines, as JSON (for VO generation).
    Sections {
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

#[derive(Subcommand)]
enum JobsOp {
    /// Plan one durable narration job per VO section with an explicit voice profile.
    Voiceover {
        settings: PathBuf,
    },
    /// Plan measured word transcription for every registered narration take.
    Alignment {
        #[arg(long, default_value = "http://127.0.0.1:3900")]
        url: String,
        #[arg(long, default_value = "auto")]
        language: String,
    },
    Plan {
        file: PathBuf,
        #[arg(long)]
        expect: Option<String>,
    },
    Status,
    Run {
        #[arg(long)]
        retry: bool,
        #[arg(long, default_value = "")]
        reply: String,
        #[arg(long)]
        stage: Option<String>,
    },
    Cancel,
    /// Allow a fresh attempt after checking an uncertain service submission.
    Resolve {
        id: String,
    },
}
#[derive(Subcommand)]
enum StyleOp {
    Init,
    Show,
    Validate {
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum PipelineOp {
    /// Save the default editable stage assignments (refuses an existing config).
    Init,
    /// Print saved stage assignments.
    Show,
    /// Print the current checkpoint, including reports and log folders.
    Status,
    /// Start a new run, archiving the previous checkpoint.
    Start,
    /// Continue after approval, creator review, an agent question or a failure.
    Continue {
        #[arg(long)]
        accept_review: bool,
        #[arg(long)]
        retry: bool,
        #[arg(long, default_value = "")]
        reply: String,
    },
    /// Ask the active coordinator to stop its CLI agent and descendants.
    Stop,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum ProjectAction {
    Brief,
    Handoff,
    Status,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PushTarget {
    Filmcraft,
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
        Cmd::Jobs { op } => {
            use spearfia_ops::production::jobs;
            let project = open(&None)?;
            let error = |e: spearfia_ops::OpError| e.to_string();
            match op {
                JobsOp::Voiceover { settings } => {
                    let settings = spearfia_core::channels::read_limited(settings, 1024 * 1024)
                        .map_err(|e| e.to_string())?;
                    let settings = serde_json::from_str(&settings)
                        .map_err(|e| format!("Invalid voice settings: {e}"))?;
                    let plan = spearfia_ops::production::planning::voiceover(&project, &settings)
                        .map_err(error)?;
                    let current = spearfia_ops::read(&project, jobs::PLAN).map_err(error)?;
                    let revision = jobs::save_plan(
                        &project,
                        &plan,
                        &spearfia_ops::version(&current),
                        Origin::Cli,
                    )
                    .map_err(error)?;
                    print_json(serde_json::json!({"revision":revision,"plan":plan}));
                }
                JobsOp::Alignment { url, language } => {
                    let plan =
                        spearfia_ops::production::planning::alignment(&project, url, language)
                            .map_err(error)?;
                    let current = spearfia_ops::read(&project, jobs::PLAN).map_err(error)?;
                    let revision = jobs::save_plan(
                        &project,
                        &plan,
                        &spearfia_ops::version(&current),
                        Origin::Cli,
                    )
                    .map_err(error)?;
                    print_json(serde_json::json!({"revision":revision,"plan":plan}));
                }
                JobsOp::Plan { file, expect } => {
                    let source = spearfia_core::channels::read_limited(file, 8 * 1024 * 1024)
                        .map_err(|e| e.to_string())?;
                    let plan: jobs::Plan =
                        serde_json::from_str(&source).map_err(|e| e.to_string())?;
                    let current = spearfia_ops::read(&project, jobs::PLAN).map_err(error)?;
                    let expected = expect
                        .clone()
                        .unwrap_or_else(|| spearfia_ops::version(&current));
                    let revision =
                        jobs::save_plan(&project, &plan, &expected, Origin::Cli).map_err(error)?;
                    print_json(serde_json::json!({"revision":revision,"plan":plan}));
                }
                JobsOp::Status => {
                    print_json(serde_json::json!(jobs::ledger(&project).map_err(error)?))
                }
                JobsOp::Run {
                    retry,
                    reply,
                    stage,
                } => {
                    let state = jobs::run_stage(
                        &project,
                        &std::env::current_exe().map_err(|e| e.to_string())?,
                        *retry,
                        reply,
                        stage.as_deref(),
                    )
                    .map_err(error)?;
                    let plan = jobs::load_plan(&project).map_err(error)?.0;
                    let unfinished = state.jobs.iter().any(|r| {
                        r.state != jobs::Status::Complete
                            && plan.jobs.iter().any(|j| {
                                j.id == r.id && stage.as_ref().is_none_or(|s| s == &j.stage)
                            })
                    });
                    print_json(serde_json::json!(state));
                    if unfinished {
                        return Err("Artifact jobs remain unfinished; inspect jobs status.".into());
                    }
                }
                JobsOp::Cancel => jobs::cancel(&project).map_err(error)?,
                JobsOp::Resolve { id } => jobs::retry_uncertain(&project, id).map_err(error)?,
            }
            return Ok(());
        }
        Cmd::Style { op } => {
            use spearfia_ops::production::style;
            let project = open(&None)?;
            let error = |e: spearfia_ops::OpError| e.to_string();
            let result = match op {
                StyleOp::Init => {
                    if project.root.join(style::FILE).exists() {
                        return Err("Project execution style already exists.".into());
                    }
                    let mut style = style::Style::default();
                    style.captions.enabled = true;
                    style.mix.enabled = true;
                    style::save(&project, &style, &spearfia_ops::version(""), Origin::Cli)
                        .map_err(error)?;
                    style
                }
                StyleOp::Show => style::load_style(&project).map_err(|e| e.to_string())?,
                StyleOp::Validate { file } => {
                    if let Some(file) = file {
                        style::Style::parse(
                            &spearfia_core::channels::read_limited(file, 1024 * 1024)
                                .map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?
                    } else {
                        style::load_style(&project).map_err(|e| e.to_string())?
                    }
                }
            };
            print_json(serde_json::json!(result));
            return Ok(());
        }
        Cmd::Captions => {
            let project = open(&None)?;
            print_json(
                spearfia_ops::production::style::build_captions(&project)
                    .map_err(|e| e.to_string())?,
            );
            return Ok(());
        }
        Cmd::Pipeline { op } => {
            use spearfia_ops::production::{self, Action, State};
            let project = open(&None)?;
            let error = |e: spearfia_ops::OpError| e.to_string();
            match op {
                PipelineOp::Init => {
                    if project.root.join(production::CONFIG).exists() {
                        return Err("Pipeline exists; edit its assignments in Production or production/pipeline.json.".into());
                    }
                    let config = production::defaults();
                    production::save_pipeline(
                        &project,
                        &config,
                        &spearfia_ops::version(""),
                        Origin::Cli,
                    )
                    .map_err(error)?;
                    print_json(serde_json::json!(config));
                }
                PipelineOp::Show => print_json(serde_json::json!(
                    production::load_pipeline(&project).map_err(error)?.0
                )),
                PipelineOp::Status => print_json(serde_json::json!(
                    production::load_run(&project).map_err(error)?
                )),
                PipelineOp::Stop => {
                    production::request_stop(&project).map_err(error)?;
                    println!("Stop requested.");
                }
                operation => {
                    let action = match operation {
                        PipelineOp::Start => Action {
                            start: true,
                            ..Default::default()
                        },
                        PipelineOp::Continue {
                            accept_review,
                            retry,
                            reply,
                        } => Action {
                            start: false,
                            accept_review: *accept_review,
                            retry: *retry,
                            reply: reply.clone(),
                        },
                        _ => unreachable!(),
                    };
                    let project =
                        Project::open(project.root.canonicalize().map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?;
                    let run = production::execute(
                        &project,
                        &std::env::current_exe().map_err(|e| e.to_string())?,
                        action,
                    )
                    .map_err(error)?;
                    let failed = matches!(run.state, State::Failed | State::Cancelled);
                    print_json(serde_json::json!(run));
                    if failed {
                        return Err(
                            "Production stopped. Inspect pipeline status and logs before retrying."
                                .into(),
                        );
                    }
                }
            }
            return Ok(());
        }
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
        Cmd::Push {
            target: PushTarget::Filmcraft,
            folder,
        } => {
            let project = open(folder)?;
            let fc = spearfia_adapter_filmcraft::find_cli()
                .and_then(|cli| spearfia_adapter_filmcraft::push(&project, &cli))
                .map_err(|e| e.to_string())?;
            print_json(fc.summary());
            return Ok(());
        }
        Cmd::Sections { folder } => {
            let doc = Doc::main(&open(folder)?).map_err(|e| e.to_string())?;
            print_json(
                serde_json::json!({ "version": doc.version, "sections": sections(&doc.script) }),
            );
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
        Cmd::Redo => {
            let e = spearfia_ops::redo(&project).map_err(err)?;
            if cli.json {
                print_json(serde_json::json!({ "redone": e.summary, "doc": e.doc }));
            } else {
                println!("Redid: {}", e.summary);
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
        Cmd::Visuals { .. }
        | Cmd::Push { .. }
        | Cmd::Sections { .. }
        | Cmd::Pipeline { .. }
        | Cmd::Jobs { .. }
        | Cmd::Style { .. }
        | Cmd::Captions => {}
    }
    Ok(())
}

/// Each `##` section (and any VO before the first one, as section 0) with its VO lines:
/// zero-based line number, the words as written and their count. `LIVE:` lines are left
/// out: they're recorded on camera, not generated.
fn sections(script: &spearfia_script::Script) -> Vec<serde_json::Value> {
    use spearfia_script::{Block, LineKind, format::line_words};
    let mut out = vec![serde_json::json!({"index": 0, "title": "Intro", "line": 0, "lines": []})];
    for b in &script.blocks {
        match b {
            Block::Heading {
                level, text, line, ..
            } if *level >= 2 => {
                let index = out.len();
                out.push(
                    serde_json::json!({"index": index, "title": text, "line": line, "lines": []}),
                );
            }
            Block::Line {
                kind: LineKind::Spoken,
                segments,
                line,
            } => {
                let words = line_words(segments);
                if !words.is_empty() {
                    let last = out.len() - 1;
                    out[last]["lines"]
                        .as_array_mut()
                        .expect("lines is an array")
                        .push(serde_json::json!({"line": line, "text": words.join(" "), "words": words.len()}));
                }
            }
            _ => {}
        }
    }
    for s in &mut out {
        let words: u64 = s["lines"]
            .as_array()
            .map_or(0, |l| l.iter().filter_map(|l| l["words"].as_u64()).sum());
        s["words"] = words.into();
        s["est_seconds"] = spearfia_script::estimate_seconds(words as usize).into();
    }
    out.retain(|s| s["index"] != 0 || s["words"] != 0);
    out
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
            "spearfia redo",
            "spearfia mcp --project p",
            "spearfia api --project p --port 0",
            "spearfia channels list",
            "spearfia channels read science-channel",
            "spearfia project handoff p",
            "spearfia project status",
            "spearfia visuals cue-sheet p",
            "spearfia sections p",
            "spearfia push filmcraft p",
        ] {
            Cli::try_parse_from(args.split(' ')).unwrap_or_else(|e| panic!("{args}: {e}"));
        }
        for args in [
            "spearfia channels",
            "spearfia project approve p",
            "spearfia visuals render p",
            "spearfia push resolve p",
        ] {
            assert!(Cli::try_parse_from(args.split(' ')).is_err(), "{args}");
        }
    }

    #[test]
    fn sections_list_vo_lines_and_skip_live_and_empty_intro() {
        let s = spearfia_script::parse(
            "# Title\n## Cold open\nVO: Could **Harry** beat Thanos?\nLIVE: Hi.\n## Round one\nVO: The wand.\n",
        );
        let out = sections(&s);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["title"], "Cold open");
        assert_eq!(out[0]["lines"][0]["line"], 2);
        assert_eq!(out[0]["lines"][0]["text"], "Could Harry beat Thanos?");
        assert_eq!(out[0]["words"], 4);
        assert_eq!(out[1]["index"], 2);
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
