// SPDX-License-Identifier: MIT OR Apache-2.0
//! CLI agent adapters. No API keys or global CLI configuration are written here.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spearfia_core::{CoreError, error::Result};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
mod job;
pub use job::ProcessTree;

pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const REPORT_SCHEMA: &str = include_str!("../../../schemas/production-report.schema.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub provider: Provider,
    pub executable: String,
    pub model: String,
    pub effort: String,
    pub timeout_seconds: u64,
    /// Let the agent's commands reach the network (Codex's sandbox blocks it otherwise).
    #[serde(default)]
    pub network: bool,
    /// Absolute folders outside the project the agent may write (a shared media library,
    /// a research folder). The project folder is always writable.
    #[serde(default)]
    pub writable_dirs: Vec<String>,
}
impl Agent {
    pub fn new(provider: Provider) -> Self {
        Self {
            provider,
            executable: String::new(),
            model: String::new(),
            effort: String::new(),
            timeout_seconds: 3600,
            network: false,
            writable_dirs: Vec::new(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if !(1..=86400).contains(&self.timeout_seconds)
            || [&self.model, &self.effort, &self.executable]
                .iter()
                .any(|s| s.len() > 4096 || s.contains(['\0', '\r', '\n']))
        {
            return Err(CoreError::Invalid(
                "Invalid agent model, effort, executable or timeout (1–86400 seconds).".into(),
            ));
        }
        if self.writable_dirs.len() > 16
            || self.writable_dirs.iter().any(|d| {
                d.len() > 4096
                    || d.contains(['\0', '\r', '\n'])
                    || !Path::new(d).is_absolute()
                    || Path::new(d).parent().is_none()
            })
        {
            return Err(CoreError::Invalid(
                "Writable folders must be up to 16 absolute paths below a drive or root.".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Completed,
    NeedsInput,
    Blocked,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub status: ReportStatus,
    pub summary: String,
    pub question: String,
}

/// Explicit argv, never shell source. Custom executables must be native programs.
pub fn command(
    agent: &Agent,
    project: &Path,
    cli: &Path,
    schema: &Path,
    report: &Path,
) -> Result<Command> {
    agent.validate()?;
    let executable = if agent.executable.is_empty() {
        discover(agent.provider)?
    } else {
        PathBuf::from(&agent.executable)
    };
    if executable.extension().is_some_and(|e| {
        ["cmd", "bat", "ps1"]
            .iter()
            .any(|s| e.eq_ignore_ascii_case(s))
    }) {
        return Err(CoreError::Invalid(
            "Choose a native CLI executable, not a shell shim.".into(),
        ));
    }
    let mut command = Command::new(executable);
    match agent.provider {
        Provider::Codex => {
            command
                .args([
                    "exec",
                    "--skip-git-repo-check",
                    "--sandbox",
                    "workspace-write",
                    "--json",
                ])
                .args(["-c", "approval_policy=\"never\""])
                .args([
                    "-c",
                    &format!(
                        "mcp_servers.spearfia={{command={},args={},required=true}}",
                        json!(cli),
                        json!(["mcp", "--project", project.to_string_lossy().as_ref()])
                    ),
                ])
                .args(
                    agent
                        .network
                        .then_some(["-c", "sandbox_workspace_write.network_access=true"])
                        .into_iter()
                        .flatten(),
                )
                .arg("--output-schema")
                .arg(schema)
                .arg("--output-last-message")
                .arg(report);
            if !agent.model.is_empty() {
                command.arg("--model").arg(&agent.model);
            }
            if !agent.effort.is_empty() {
                command.args([
                    "-c",
                    &format!("model_reasoning_effort={}", json!(agent.effort)),
                ]);
            }
            command.arg("-");
        }
        Provider::Claude => {
            command.args(["--print", "--verbose", "--output-format", "stream-json", "--permission-mode", "acceptEdits", "--permission-prompts", "none"])
                .args(["--allowedTools", "Read,Glob,Grep,Skill,Bash,mcp__spearfia__*"])
                .arg("--mcp-config").arg(json!({"mcpServers":{"spearfia":{"command":cli,"args":["mcp","--project",project]}}}).to_string())
                .arg("--json-schema").arg(REPORT_SCHEMA);
            if !agent.model.is_empty() {
                command.arg("--model").arg(&agent.model);
            }
            if !agent.effort.is_empty() {
                command.arg("--effort").arg(&agent.effort);
            }
        }
    }
    for dir in &agent.writable_dirs {
        // Both CLIs refuse a missing folder; a library or research root is created on demand.
        std::fs::create_dir_all(dir)
            .map_err(|e| CoreError::Invalid(format!("Can't create writable folder {dir}: {e}")))?;
        command.arg("--add-dir").arg(dir);
    }
    command.current_dir(project).stdin(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    Ok(command)
}

pub fn discover(provider: Provider) -> Result<PathBuf> {
    let name = match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    };
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let direct = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        if direct.is_file() {
            return Ok(direct);
        }
        if cfg!(windows) && provider == Provider::Codex {
            for arch in ["x64", "arm64"] {
                let triple = if arch == "x64" { "x86_64" } else { "aarch64" };
                let base = dir.join(format!("node_modules/@openai/codex/node_modules/@openai/codex-win32-{arch}/vendor/{triple}-pc-windows-msvc"));
                for relative in ["bin/codex.exe", "codex/codex.exe"] {
                    let candidate = base.join(relative);
                    if candidate.is_file() {
                        return Ok(candidate);
                    }
                }
            }
        }
    }
    Err(CoreError::Invalid(format!(
        "{name} was not found. Install/login to the CLI and restart Spear Fia, or set its native executable in the stage assignment."
    )))
}

/// Provider errors/permission denials outrank the agent's final report.
pub fn report(provider: Provider, stdout: &Path, answer: &Path) -> Result<Report> {
    let output = spearfia_core::channels::read_limited(stdout, 32 * 1024 * 1024)?;
    let mut finished = false;
    let mut structured = None;
    for line in output.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = event["type"].as_str().unwrap_or_default();
        if kind == "result"
            && event["permission_denials"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
        {
            return Ok(Report {
                status: ReportStatus::NeedsInput,
                summary: "The CLI denied a required tool.".into(),
                question: "Review the agent log and CLI tool permissions, then retry this stage."
                    .into(),
            });
        }
        if matches!(kind, "error" | "turn.failed")
            || (kind == "result" && event["is_error"] == true)
        {
            return Err(CoreError::Invalid(format!(
                "Agent reported a failure: {}",
                event.to_string().chars().take(2000).collect::<String>()
            )));
        }
        if kind == "turn.completed" || kind == "result" {
            finished = true;
        }
        if kind == "result" {
            structured = event.get("structured_output").cloned();
        }
    }
    if !finished {
        return Err(CoreError::Invalid(
            "Agent exited without a completed turn/result. Inspect its logs before retrying."
                .into(),
        ));
    }
    let text = match provider {
        Provider::Codex => spearfia_core::channels::read_limited(answer, 128 * 1024)?,
        Provider::Claude => structured
            .ok_or_else(|| {
                CoreError::Invalid("Claude returned no structured production report.".into())
            })?
            .to_string(),
    };
    let report: Report = serde_json::from_str(&text)
        .map_err(|e| CoreError::Invalid(format!("Invalid agent report: {e}")))?;
    if report.summary.trim().is_empty()
        || report.summary.len() > 65536
        || report.question.len() > 65536
    {
        return Err(CoreError::Invalid(
            "Agent report is empty or too large.".into(),
        ));
    }
    Ok(report)
}

/// Kill descendants as well as the agent, then reap the immediate child.
pub fn terminate(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-TERM", "--", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argv_keeps_paths_out_of_shells() {
        let mut a = Agent::new(Provider::Codex);
        a.executable = "native-agent".into();
        a.model = "chosen-model".into();
        a.effort = "xhigh".into();
        let c = command(
            &a,
            Path::new("project with spaces"),
            Path::new("spearfia.exe"),
            Path::new("schema.json"),
            Path::new("answer.json"),
        )
        .unwrap();
        let args: Vec<_> = c.get_args().map(|s| s.to_string_lossy()).collect();
        assert!(args.contains(&"workspace-write".into()));
        assert!(args.contains(&"chosen-model".into()));
        assert!(args.iter().any(|s| s.contains("project with spaces")));
        assert!(!args.iter().any(|s| s.contains("bypass")));
        assert!(!args.iter().any(|s| s.contains("network_access")));
    }
    #[test]
    fn network_and_writable_folders_are_opt_in_per_agent() {
        let dir = std::env::temp_dir().join(format!("spearfia-ai-lib-{}", std::process::id()));
        let lib = dir.to_string_lossy().into_owned();
        for provider in [Provider::Codex, Provider::Claude] {
            let mut a = Agent::new(provider);
            a.executable = "native-agent".into();
            a.network = true;
            a.writable_dirs = vec![lib.clone()];
            let c = command(
                &a,
                Path::new("p"),
                Path::new("s"),
                Path::new("x"),
                Path::new("y"),
            )
            .unwrap();
            let args: Vec<_> = c.get_args().map(|s| s.to_string_lossy()).collect();
            let at = args.iter().position(|s| s == "--add-dir").unwrap();
            assert_eq!(args[at + 1], lib);
            assert_eq!(
                args.iter()
                    .any(|s| s == "sandbox_workspace_write.network_access=true"),
                provider == Provider::Codex
            );
        }
        assert!(dir.is_dir());
        let mut bad = Agent::new(Provider::Codex);
        bad.writable_dirs = vec!["relative/library".into()];
        assert!(bad.validate().is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
