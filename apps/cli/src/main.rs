//! `spearfia`: command-line interface. Calls the same command layer (`spearfia-ops`) as the
//! desktop app, so every change is checked against the file on disk and can be undone from
//! either.
//!
//! Every read prints the script's version; pass it back with `--expect` so a write fails
//! instead of landing on a script that changed in between. `--dry-run` shows the diff and
//! writes nothing.

use clap::{Parser, Subcommand};
use spearfia_core::project::Project;
use spearfia_ops::{Doc, Origin, Plan, transitions};
use std::path::PathBuf;
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
    let project = Project::open(&cli.project).map_err(|e| e.to_string())?;
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
    }
    Ok(())
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
        ] {
            Cli::try_parse_from(args.split(' ')).unwrap_or_else(|e| panic!("{args}: {e}"));
        }
    }
}
