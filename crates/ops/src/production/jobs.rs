// SPDX-License-Identifier: MIT OR Apache-2.0
//! Durable artifact jobs, dependency scheduling and service reconciliation.
use super::{Agent, Check, Stage, invalid};
use crate::{Origin, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use spearfia_analysis::{hash_file, hash_text};
use spearfia_core::{
    links::{Link, LinkKind, LinkMedia},
    project::{MediaRole, Project, write_json},
};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub const PLAN: &str = "production/jobs.json";
const STATE: &str = "sync/production/jobs.json";
const CANCEL: &str = "sync/production/jobs-cancel";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anchor {
    pub line: usize,
    pub quote: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    pub path: String,
    pub kind: String,
    #[serde(default)]
    pub role: Option<MediaRole>,
    #[serde(default)]
    pub anchor: Option<Anchor>,
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub index: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "adapter", rename_all = "kebab-case")]
pub enum Task {
    Comfyui {
        url: String,
        workflow: Value,
        outputs: Vec<Output>,
    },
    Voiceover {
        url: String,
        text: String,
        profile_id: String,
        instruct: String,
        language: String,
        speed: f64,
        steps: u32,
        outputs: Vec<Output>,
    },
    Alignment {
        url: String,
        audio: String,
        lines: Vec<usize>,
        language: String,
        outputs: Vec<Output>,
    },
    Command {
        program: String,
        args: Vec<String>,
        outputs: Vec<Output>,
    },
    Agent {
        agent: Agent,
        prompt: String,
    },
}
impl Task {
    fn outputs(&self) -> &[Output] {
        match self {
            Self::Comfyui { outputs, .. }
            | Self::Voiceover { outputs, .. }
            | Self::Alignment { outputs, .. }
            | Self::Command { outputs, .. } => outputs,
            Self::Agent { .. } => &[],
        }
    }
    fn outputs_mut(&mut self) -> Option<&mut Vec<Output>> {
        match self {
            Self::Comfyui { outputs, .. }
            | Self::Voiceover { outputs, .. }
            | Self::Alignment { outputs, .. }
            | Self::Command { outputs, .. } => Some(outputs),
            Self::Agent { .. } => None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub title: String,
    pub stage: String,
    pub depends_on: Vec<String>,
    pub inputs: Vec<String>,
    pub task: Task,
    pub checks: Vec<Check>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub spec_version: u32,
    pub jobs: Vec<Job>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Pending,
    Waiting,
    Submitting,
    Running,
    Complete,
    Failed,
    Uncertain,
    Cancelled,
    NeedsInput,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    pub path: String,
    pub hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub key: String,
    pub state: Status,
    pub provider_id: String,
    pub message: String,
    pub attempts: u32,
    pub seed: u32,
    pub started: f64,
    pub outputs: Vec<Proof>,
    pub link_ids: Vec<String>,
    pub reply: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ledger {
    pub spec_version: u32,
    pub jobs: Vec<Record>,
}
fn proofs_valid(project: &Project, proofs: &[Proof]) -> bool {
    proofs.iter().all(|p| {
        if p.path.starts_with('@') {
            super::verify::current_stamp(project, &p.path)
                .ok()
                .as_deref()
                == Some(&p.hash)
        } else {
            hash_file(&project.resolve(&p.path)).ok().as_deref() == Some(&p.hash)
        }
    })
}
fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.as_bytes()[0].is_ascii_lowercase()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
impl Plan {
    pub fn validate(&self) -> Result<()> {
        if self.spec_version != 1 || self.jobs.len() > 5000 {
            return Err(invalid("A v1 job plan supports at most 5000 jobs."));
        }
        let mut ids = BTreeSet::new();
        for job in &self.jobs {
            if !id_ok(&job.id)
                || !ids.insert(job.id.clone())
                || job.title.trim().is_empty()
                || job.title.len() > 4096
                || job.depends_on.len() > 5000
                || job.inputs.len() > 1000
                || job.checks.len() > 32
            {
                return Err(invalid(
                    "Invalid/duplicate job ID, title or dependency/input limits.",
                ));
            }
            if job.task.outputs().is_empty() && job.checks.is_empty() {
                return Err(invalid(
                    "Each job requires verifiable outputs or artifact checks.",
                ));
            }
            for input in job.inputs.iter().chain(match &job.task {
                Task::Alignment { audio, .. } => Some(audio),
                _ => None,
            }) {
                if let Some(reference) = input.strip_prefix("job:") {
                    let (id, output) = reference
                        .rsplit_once(':')
                        .ok_or_else(|| invalid("Use job:<id>:<output-index>."))?;
                    if !job.depends_on.iter().any(|d| d == id) || output.parse::<usize>().is_err() {
                        return Err(invalid(
                            "Job output references require an explicit dependency and numeric index.",
                        ));
                    }
                }
            }
            if job
                .checks
                .iter()
                .any(|check| matches!(check, Check::Jobs { .. }))
            {
                return Err(invalid(
                    "Artifact jobs cannot recursively verify a job stage.",
                ));
            }
            for output in job.task.outputs() {
                super::verify::relative_output(&output.path)?;
                if !["audio", "video", "image", "text", "json"].contains(&output.kind.as_str())
                    || output.role.is_some() != output.anchor.is_some()
                {
                    return Err(invalid(
                        "Outputs need a supported kind and both a role and narration anchor when linking.",
                    ));
                }
                if let Some(anchor) = &output.anchor
                    && anchor.quote.trim().is_empty()
                {
                    return Err(invalid("Output narration quote cannot be empty."));
                }
            }
            match &job.task {
                Task::Comfyui {
                    url,
                    workflow,
                    outputs,
                } => {
                    service_url(url)?;
                    if !workflow.is_object()
                        || outputs.is_empty()
                        || outputs
                            .iter()
                            .any(|o| !["audio", "video", "image"].contains(&o.kind.as_str()))
                    {
                        return Err(invalid(
                            "ComfyUI requires an API-format workflow and media output selectors.",
                        ));
                    }
                }
                Task::Voiceover {
                    url,
                    text,
                    profile_id,
                    speed,
                    steps,
                    outputs,
                    ..
                } => {
                    service_url(url)?;
                    if text.trim().is_empty()
                        || text.len() > 65536
                        || profile_id.is_empty()
                        || !speed.is_finite()
                        || *speed <= 0.0
                        || *steps == 0
                        || outputs.len() != 1
                        || outputs[0].kind != "audio"
                    {
                        return Err(invalid(
                            "Voiceover requires text, an explicit voice profile, valid speed/steps and one audio output.",
                        ));
                    }
                }
                Task::Alignment { url, outputs, .. } => {
                    service_url(url)?;
                    if outputs.len() != 1
                        || outputs[0].kind != "json"
                        || !outputs[0].path.ends_with(".alignment.json")
                    {
                        return Err(invalid("Alignment needs one .alignment.json output."));
                    }
                }
                Task::Command { program, args, .. } => {
                    if program.is_empty()
                        || args.len() > 1000
                        || [".cmd", ".bat", ".ps1"]
                            .iter()
                            .any(|s| program.to_ascii_lowercase().ends_with(s))
                    {
                        return Err(invalid(
                            "Command jobs use native programs and explicit argv, without shell shims.",
                        ));
                    }
                }
                Task::Agent { agent, prompt } => {
                    agent.validate()?;
                    if prompt.trim().is_empty() || prompt.len() > 65536 {
                        return Err(invalid("Agent job needs bounded instructions."));
                    }
                }
            }
        }
        let mut done = BTreeSet::new();
        while done.len() < ids.len() {
            let before = done.len();
            for job in &self.jobs {
                if job.depends_on.iter().all(|d| done.contains(d)) {
                    done.insert(job.id.clone());
                }
            }
            if before == done.len() {
                return Err(invalid(
                    "Job dependencies contain a cycle or refer to a missing job.",
                ));
            }
        }
        Ok(())
    }
}
fn service_url(url: &str) -> Result<()> {
    if !(url.starts_with("http://") || url.starts_with("https://"))
        || url.contains(['\r', '\n', '\0'])
    {
        return Err(invalid("Service URL must use HTTP(S)."));
    }
    Ok(())
}
pub fn load_plan(project: &Project) -> Result<(Plan, String)> {
    let text = spearfia_core::channels::read_limited(&project.root.join(PLAN), 8 * 1024 * 1024)?;
    let plan: Plan =
        serde_json::from_str(&text).map_err(|e| invalid(format!("Invalid job plan: {e}")))?;
    plan.validate()?;
    Ok((plan, crate::version(&text)))
}
pub fn save_plan(project: &Project, plan: &Plan, expected: &str, origin: Origin) -> Result<String> {
    plan.validate()?;
    let _runner_lock = runner_lock(project)?;
    let unresolved = ledger(project)?;
    if project.root.join(PLAN).exists() {
        let old = load_plan(project)?.0;
        for record in unresolved.jobs.iter().filter(|r| {
            matches!(
                r.state,
                Status::Running | Status::Submitting | Status::Uncertain
            )
        }) {
            let previous = old
                .jobs
                .iter()
                .find(|j| j.id == record.id)
                .map(serde_json::to_value)
                .transpose()
                .map_err(|e| invalid(e.to_string()))?;
            let proposed = plan
                .jobs
                .iter()
                .find(|j| j.id == record.id)
                .map(serde_json::to_value)
                .transpose()
                .map_err(|e| invalid(e.to_string()))?;
            if previous != proposed {
                return Err(invalid(format!(
                    "Resolve service request {} before changing or removing its definition.",
                    record.id
                )));
            }
        }
    }
    let text = serde_json::to_string_pretty(plan).map_err(|e| invalid(e.to_string()))? + "\n";
    let doc = crate::Doc::from_text(PLAN, crate::read(project, PLAN)?);
    doc.expect(Some(expected))?;
    crate::apply(
        project,
        &doc.plan(text, "Plan production artifact jobs"),
        origin,
    )
}
pub fn ledger(project: &Project) -> Result<Ledger> {
    if !project.root.join(STATE).exists() {
        return Ok(Ledger {
            spec_version: 1,
            jobs: vec![],
        });
    }
    let text = spearfia_core::channels::read_limited(&project.root.join(STATE), 16 * 1024 * 1024)?;
    let ledger: Ledger = serde_json::from_str(&text)
        .map_err(|e| invalid(format!("Invalid jobs checkpoint: {e}")))?;
    if ledger.spec_version != 1 || ledger.jobs.len() > 5000 {
        return Err(invalid("Unsupported jobs checkpoint."));
    }
    Ok(ledger)
}
fn save(project: &Project, state: &Ledger) -> Result<()> {
    let _lock = project.edit_lock()?;
    write_json(&project.root.join(STATE), state)?;
    Ok(())
}
pub fn cancel(project: &Project) -> Result<()> {
    fs::create_dir_all(project.root.join("sync/production")).map_err(|e| invalid(e.to_string()))?;
    fs::write(project.root.join(CANCEL), b"stop").map_err(|e| invalid(e.to_string()))
}
pub fn is_running(project: &Project) -> Result<bool> {
    match OpenOptions::new()
        .read(true)
        .write(true)
        .open(project.root.join("sync/production/jobs.lock"))
    {
        Ok(file) => match file.try_lock() {
            Ok(()) => Ok(false),
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Err(error) => Err(invalid(error.to_string())),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(invalid(e.to_string())),
    }
}
pub fn verified_stage(project: &Project, stage: &str) -> Result<()> {
    let (plan, _) = load_plan(project)?;
    let state = ledger(project)?;
    let selected: Vec<_> = plan.jobs.iter().filter(|j| j.stage == stage).collect();
    if selected.is_empty() {
        return Err(invalid(format!(
            "No artifact jobs are planned for stage {stage}."
        )));
    }
    for job in selected {
        let record = state
            .jobs
            .iter()
            .find(|r| r.id == job.id)
            .ok_or_else(|| invalid("Artifact job has no checkpoint."))?;
        if record.state != Status::Complete
            || record.key != fingerprint(project, job, &state)?
            || !proofs_valid(project, &record.outputs)
        {
            return Err(invalid(format!(
                "Job {} is unfinished or has changed outputs.",
                job.id
            )));
        }
    }
    Ok(())
}
/// Creator-only explicit resolution when the service cannot prove a lost submission.
pub fn retry_uncertain(project: &Project, id: &str) -> Result<()> {
    let _runner_lock = runner_lock(project)?;
    let mut state = ledger(project)?;
    let record = state
        .jobs
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| invalid("Unknown artifact job."))?;
    if !matches!(record.state, Status::Uncertain | Status::Cancelled) {
        return Err(invalid(
            "Only an uncertain job needs this explicit resolution.",
        ));
    }
    record.state = Status::Failed;
    record.provider_id.clear();
    record.message = "Creator checked the external service and allowed a new attempt.".into();
    save(project, &state)
}
fn resolved_input(project: &Project, value: &str, state: &Ledger) -> Result<PathBuf> {
    if let Some(reference) = value.strip_prefix("job:") {
        let (id, index) = reference
            .rsplit_once(':')
            .ok_or_else(|| invalid("Use job:<id>:<output-index> for a dependency output."))?;
        let index = index
            .parse::<usize>()
            .map_err(|_| invalid("Invalid dependency output index."))?;
        let record = state
            .jobs
            .iter()
            .find(|r| r.id == id && r.state == Status::Complete)
            .ok_or_else(|| invalid(format!("Dependency {id} is not complete.")))?;
        return record
            .outputs
            .get(index)
            .filter(|p| !p.path.starts_with('@'))
            .map(|p| project.resolve(&p.path))
            .ok_or_else(|| invalid("Dependency output index is missing."));
    }
    Ok(project.resolve(value))
}
fn fingerprint(project: &Project, job: &Job, state: &Ledger) -> Result<String> {
    let mut data = serde_json::to_string(job).map_err(|e| invalid(e.to_string()))?;
    data.push_str(&project.approved_script()?);
    data.push_str(project.channel_brief()?.as_deref().unwrap_or_default());
    for input in &job.inputs {
        data.push_str(&hash_file(&resolved_input(project, input, state)?)?);
    }
    if let Task::Alignment { audio, .. } = &job.task {
        data.push_str(&hash_file(&resolved_input(project, audio, state)?)?);
        data.push_str(
            &serde_json::to_string(&project.file.settings.timebase)
                .map_err(|e| invalid(e.to_string()))?,
        );
    }
    for dep in &job.depends_on {
        let record = state
            .jobs
            .iter()
            .find(|r| &r.id == dep)
            .ok_or_else(|| invalid("Dependency checkpoint is missing."))?;
        data.push_str(&serde_json::to_string(&record.outputs).map_err(|e| invalid(e.to_string()))?);
    }
    Ok(hash_text(&data))
}
fn owned_output(project: &Project, path: &str) -> Result<PathBuf> {
    super::verify::relative_output(path)?;
    let target = project.resolve(path);
    let mut existing = target.as_path();
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| invalid("Invalid output parent."))?;
    }
    let root = project
        .root
        .canonicalize()
        .map_err(|e| invalid(e.to_string()))?;
    if !existing
        .canonicalize()
        .map_err(|e| invalid(e.to_string()))?
        .starts_with(root)
    {
        return Err(invalid("Output path escapes the project through an alias."));
    }
    Ok(target)
}
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}
fn runner_lock(project: &Project) -> Result<File> {
    fs::create_dir_all(project.root.join("sync/production")).map_err(|e| invalid(e.to_string()))?;
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(project.root.join("sync/production/jobs.lock"))
        .map_err(|e| invalid(e.to_string()))?;
    lock.try_lock().map_err(|_|invalid("Another artifact-job runner is active; stop it before changing plans or resolving requests."))?;
    Ok(lock)
}
/// Execute ready jobs sequentially. Failed jobs do not stop unrelated ready work.
/// A known ComfyUI ID resumes automatically; ambiguous submissions never resubmit.
pub fn run(project: &Project, cli: &Path, retry: bool, reply: &str) -> Result<Ledger> {
    run_stage(project, cli, retry, reply, None)
}
pub fn run_stage(
    project: &Project,
    cli: &Path,
    retry: bool,
    reply: &str,
    stage: Option<&str>,
) -> Result<Ledger> {
    let project = Project::open(
        project
            .root
            .canonicalize()
            .map_err(|e| invalid(e.to_string()))?,
    )?;
    project.approved_script()?;
    let _runner_lock = runner_lock(&project)?;
    if project.root.join(CANCEL).exists() {
        fs::remove_file(project.root.join(CANCEL)).map_err(|e| invalid(e.to_string()))?;
    }
    let (plan, revision) = load_plan(&project)?;
    if stage.is_some_and(|s| !plan.jobs.iter().any(|j| j.stage == s)) {
        return Err(invalid("No jobs are planned for the requested stage."));
    }
    let mut state = ledger(&project)?;
    state
        .jobs
        .retain(|r| plan.jobs.iter().any(|j| j.id == r.id));
    for job in &plan.jobs {
        if !state.jobs.iter().any(|r| r.id == job.id) {
            state.jobs.push(Record {
                id: job.id.clone(),
                key: String::new(),
                state: Status::Pending,
                provider_id: String::new(),
                message: String::new(),
                attempts: 0,
                seed: 0,
                started: 0.0,
                outputs: vec![],
                link_ids: vec![],
                reply: String::new(),
            });
        }
    }
    // Validate old completions before considering any dependent, regardless of plan order.
    for job in &plan.jobs {
        let index = state.jobs.iter().position(|r| r.id == job.id).unwrap();
        if state.jobs[index].state == Status::Complete {
            let valid = fingerprint(&project, job, &state).ok().as_deref()
                == Some(&state.jobs[index].key)
                && proofs_valid(&project, &state.jobs[index].outputs);
            if !valid {
                state.jobs[index].state = Status::Pending;
                state.jobs[index].message = "Input or output fingerprint changed".into();
            }
        }
    }
    let mut considered = BTreeSet::new();
    loop {
        let mut progress = false;
        for job in &plan.jobs {
            if stage.is_some_and(|stage| stage != job.stage) {
                continue;
            }
            let index = state.jobs.iter().position(|r| r.id == job.id).unwrap();
            if considered.contains(&job.id) {
                continue;
            }
            if !job.depends_on.iter().all(|id| {
                state
                    .jobs
                    .iter()
                    .any(|r| &r.id == id && r.state == Status::Complete)
            }) {
                state.jobs[index].state = Status::Waiting;
                state.jobs[index].message = "Waiting for dependencies".into();
                continue;
            }
            let key = match fingerprint(&project, job, &state) {
                Ok(key) => key,
                Err(e) => {
                    state.jobs[index].state = Status::Failed;
                    state.jobs[index].message = e.to_string();
                    considered.insert(job.id.clone());
                    continue;
                }
            };
            let valid = state.jobs[index].state == Status::Complete
                && state.jobs[index].key == key
                && proofs_valid(&project, &state.jobs[index].outputs);
            if valid {
                let links = spearfia_core::links::Links::load(&project)?;
                if state.jobs[index]
                    .link_ids
                    .iter()
                    .any(|id| !links.links.iter().any(|l| &l.id == id))
                {
                    let mut task = job.task.clone();
                    version_outputs(&mut task, &key);
                    if let Err(e) = accept_outputs(&project, job, &task, index, &mut state) {
                        state.jobs[index].state = Status::Failed;
                        state.jobs[index].message = e.to_string();
                    }
                }
                considered.insert(job.id.clone());
                continue;
            }
            if !gate_ready(&project, &job.stage)? {
                state.jobs[index].state = Status::Waiting;
                state.jobs[index].message = "Waiting for the preceding creator review gate".into();
                continue;
            }
            let was = state.jobs[index].state;
            let same_key = state.jobs[index].key == key;
            if !same_key
                && matches!(
                    was,
                    Status::Submitting | Status::Running | Status::Uncertain
                )
            {
                state.jobs[index].state = Status::Uncertain;
                state.jobs[index].message="Inputs changed while a service request was unresolved. Check that request and explicitly resolve it before submitting new work.".into();
                considered.insert(job.id.clone());
                continue;
            }
            if state.jobs[index].key != key {
                state.jobs[index].key = key.clone();
                state.jobs[index].state = Status::Pending;
                state.jobs[index].provider_id.clear();
                state.jobs[index].outputs.clear();
            } else if (matches!(was, Status::Failed | Status::Cancelled) && !retry)
                || (was == Status::NeedsInput && reply.trim().is_empty())
            {
                considered.insert(job.id.clone());
                continue;
            }
            if !reply.is_empty() {
                state.jobs[index].reply = reply.into();
            }
            if project.root.join(CANCEL).exists()
                || project.root.join("sync/production/stop").exists()
            {
                save(&project, &state)?;
                return Ok(state);
            }
            project.approved_script()?;
            let recovering = same_key
                && matches!(
                    was,
                    Status::Submitting | Status::Running | Status::Uncertain | Status::Cancelled
                );
            if same_key
                && was == Status::Failed
                && retry
                && !job.task.outputs().is_empty()
                && state.jobs[index].outputs.len() >= job.task.outputs().len()
                && proofs_valid(&project, &state.jobs[index].outputs)
            {
                let mut task = job.task.clone();
                version_outputs(&mut task, &key);
                if let Err(e) = accept_outputs(&project, job, &task, index, &mut state) {
                    state.jobs[index].state = Status::Failed;
                    state.jobs[index].message = e.to_string();
                }
                save(&project, &state)?;
                considered.insert(job.id.clone());
                continue;
            }
            if !recovering {
                state.jobs[index].attempts += 1;
                state.jobs[index].seed =
                    ((now() * 1000.0) as u64 ^ (index as u64 * 2654435761)) as u32 & 0x7fff_ffff;
                state.jobs[index].started = now();
                state.jobs[index].provider_id.clear();
            }
            state.jobs[index].state = Status::Submitting;
            state.jobs[index].message = if recovering {
                "Reconciling saved service request"
            } else {
                "Preparing job"
            }
            .into();
            save(&project, &state)?;
            let result = execute_one(&project, cli, job, index, &mut state, recovering);
            if let Err(error) = result {
                if matches!(&job.task, Task::Agent { .. }) && project.root.join(CANCEL).exists() {
                    state.jobs[index].state = Status::Cancelled;
                }
                if state.jobs[index].state != Status::Uncertain
                    && state.jobs[index].state != Status::Cancelled
                    && state.jobs[index].state != Status::NeedsInput
                {
                    state.jobs[index].state = Status::Failed;
                }
                state.jobs[index].message = error.to_string();
            }
            save(&project, &state)?;
            considered.insert(job.id.clone());
            progress = true;
            if load_plan(&project)?.1 != revision {
                return Err(invalid(
                    "Job definitions changed during execution; inspect the saved results and run the updated plan.",
                ));
            }
        }
        if !progress {
            break;
        }
    }
    save(&project, &state)?;
    Ok(state)
}
fn gate_ready(project: &Project, stage: &str) -> Result<bool> {
    let Some(run) = super::load_run(project)? else {
        return Ok(true);
    };
    let Some(index) = run.steps.iter().position(|s| s.id == stage) else {
        return Ok(false);
    };
    Ok(run.steps[..index]
        .iter()
        .all(|s| s.state == super::State::Complete))
}
fn execute_one(
    project: &Project,
    cli: &Path,
    job: &Job,
    index: usize,
    state: &mut Ledger,
    recover: bool,
) -> Result<()> {
    let mut task = job.task.clone();
    version_outputs(&mut task, &state.jobs[index].key);
    for o in task.outputs() {
        let target = owned_output(project, &o.path)?;
        if target.exists() && state.jobs[index].attempts == 1 && !recover {
            return Err(invalid(
                "An unverified output already exists. Choose a versioned {key} output path instead of overwriting it.",
            ));
        }
    }
    if let Task::Alignment { audio, .. } = &mut task {
        *audio = resolved_input(project, audio, state)?
            .to_string_lossy()
            .into_owned();
    }
    let log = format!(
        "sync/production/jobs-logs/{}-{}",
        job.id, state.jobs[index].attempts
    );
    let directory = project.root.join(&log);
    fs::create_dir_all(&directory).map_err(|e| invalid(e.to_string()))?;
    match &task {
        Task::Agent { agent, prompt } => {
            let stage = Stage {
                id: job.id.clone(),
                title: job.title.clone(),
                agent: agent.clone(),
                prompt: prompt.clone(),
                production: true,
                review: false,
                checks: job.checks.clone(),
            };
            let instruction = format!(
                "{}\n\n{}\n\nCreator reply: {}\nRead existing artifacts before retrying. Do not recursively run artifact jobs. Return the structured production report.",
                prompt,
                project.agent_brief(true, None)?,
                state.jobs[index].reply
            );
            state.jobs[index].state = Status::Running;
            save(project, state)?;
            let report = super::runner::run_agent(project, &stage, cli, &instruction, &log, true)?;
            match report.status {
                spearfia_ai::ReportStatus::Completed => {}
                spearfia_ai::ReportStatus::NeedsInput => {
                    state.jobs[index].state = Status::NeedsInput;
                    return Err(invalid(format!("{}\n{}", report.summary, report.question)));
                }
                spearfia_ai::ReportStatus::Blocked => return Err(invalid(report.summary)),
            }
        }
        Task::Command { program, args, .. } => {
            let mut command = Command::new(program);
            for arg in args {
                let mut arg = arg.clone();
                for (i, o) in task.outputs().iter().enumerate() {
                    arg = arg.replace(
                        &format!("{{output{i}}}"),
                        &project.resolve(&o.path).to_string_lossy(),
                    );
                }
                for (i, input) in job.inputs.iter().enumerate() {
                    arg = arg.replace(
                        &format!("{{input{i}}}"),
                        &resolved_input(project, input, state)?.to_string_lossy(),
                    );
                }
                command.arg(arg);
            }
            launch(project, &mut command, &directory, index, state, false)?;
        }
        _ => {
            let adapter = project.root.join("cache/production/service_job.py");
            fs::create_dir_all(adapter.parent().unwrap()).map_err(|e| invalid(e.to_string()))?;
            fs::write(&adapter, include_str!("service_job.py"))
                .map_err(|e| invalid(e.to_string()))?;
            let mut request_task = task.clone();
            if let Some(outputs) = request_task.outputs_mut() {
                for o in outputs {
                    o.path = project.resolve(&o.path).to_string_lossy().into_owned();
                }
            }
            let request = directory.join("request.json");
            write_json(
                &request,
                &json!({"task":request_task,"provider_id":state.jobs[index].provider_id,"key":state.jobs[index].key,"seed":state.jobs[index].seed,"started":state.jobs[index].started,"recover_only":recover,"timeout_seconds":3600,"stop_file":project.root.join(CANCEL)}),
            )?;
            let mut command = Command::new(
                std::env::var_os("SPEARFIA_PYTHON").unwrap_or_else(|| "python".into()),
            );
            command.arg(adapter).arg(request);
            launch(project, &mut command, &directory, index, state, true)?;
            if let Task::Alignment {
                audio,
                lines,
                outputs,
                ..
            } = &task
            {
                let raw: Value = serde_json::from_str(
                    &fs::read_to_string(project.resolve(&outputs[0].path))
                        .map_err(|e| invalid(e.to_string()))?,
                )
                .map_err(|e| invalid(e.to_string()))?;
                let words: Vec<spearfia_analysis::MeasuredWord> =
                    serde_json::from_value(raw["words"].clone())
                        .map_err(|e| invalid(e.to_string()))?;
                let duration = crate::timing::audio_seconds_precise(Path::new(audio))
                    .ok_or_else(|| invalid("Cannot measure aligned source duration."))?;
                let audio_path = Path::new(audio)
                    .strip_prefix(&project.root)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| audio.clone());
                let selected = if lines.is_empty() {
                    spearfia_analysis::script_words(&project.read_script()?)
                        .into_iter()
                        .map(|w| w.line)
                        .collect()
                } else {
                    lines.clone()
                };
                let alignment = spearfia_analysis::Alignment::measured(
                    &project.approved_script()?,
                    audio_path,
                    hash_file(Path::new(audio))?,
                    &selected,
                    &words,
                    duration,
                    project.file.settings.timebase,
                )?;
                write_json(&project.resolve(&outputs[0].path), &alignment)?;
            }
        }
    }
    accept_outputs(project, job, &task, index, state)
}
fn version_outputs(task: &mut Task, key: &str) {
    let short = key
        .strip_prefix("sha256:")
        .unwrap_or(key)
        .chars()
        .take(16)
        .collect::<String>();
    if let Some(outputs) = task.outputs_mut() {
        for o in outputs {
            o.path = o.path.replace("{key}", &short);
        }
    }
}
fn accept_outputs(
    project: &Project,
    job: &Job,
    task: &Task,
    index: usize,
    state: &mut Ledger,
) -> Result<()> {
    project.approved_script()?;
    if fingerprint(project, job, state)? != state.jobs[index].key {
        return Err(invalid(
            "Production inputs changed during the job; outputs were not accepted or linked.",
        ));
    }
    let mut proofs = Vec::new();
    let mut links = Vec::new();
    for output in task.outputs() {
        let path = owned_output(project, &output.path)?;
        verify_output(&path, &output.kind)?;
        let hash = hash_file(&path)?;
        proofs.push(Proof {
            path: output.path.clone(),
            hash: hash.clone(),
        });
        if let (Some(role), Some(anchor)) = (output.role, &output.anchor) {
            let actual = project
                .file
                .media_folders
                .iter()
                .filter(|f| path.starts_with(project.resolve(&f.path)))
                .max_by_key(|f| f.path.len())
                .map(|f| f.role);
            if actual != Some(role) {
                return Err(invalid(
                    "Generated media must be inside a project media folder with the requested role.",
                ));
            }
            let script = spearfia_script::parse(&project.read_script()?);
            let lines: Vec<_> = script
                .blocks
                .iter()
                .filter_map(|b| {
                    if let spearfia_script::Block::Line { line, segments, .. } = b {
                        Some((*line, spearfia_script::format::line_words(segments)))
                    } else {
                        None
                    }
                })
                .collect();
            if spearfia_core::links::locate(&anchor.quote, anchor.line, &lines).is_none() {
                return Err(invalid("Generated output's narration anchor is missing."));
            }
            links.push(Link {
                id: String::new(),
                quote: anchor.quote.clone(),
                line: anchor.line,
                media: LinkMedia {
                    path: output.path.clone(),
                    hash: Some(hash),
                },
                role,
                kind: LinkKind::for_role(role),
                in_point: None,
                out_point: None,
            });
        }
    }
    // Save verified downloads before local registration/checks; retry those steps
    // without submitting another generation request.
    state.jobs[index].outputs = proofs.clone();
    save(project, state)?;
    if !links.is_empty() {
        let (_, ids) =
            crate::links::replace(project, &state.jobs[index].link_ids, links, Origin::Agent)?;
        state.jobs[index].link_ids = ids;
    }
    if !job.checks.is_empty() {
        let stage = Stage {
            id: job.id.clone(),
            title: job.title.clone(),
            agent: Agent::new(super::Provider::Codex),
            prompt: "Verify job outputs".into(),
            production: true,
            review: false,
            checks: job.checks.clone(),
        };
        for (path, _) in super::verify::verify(project, &stage)? {
            proofs.push(Proof {
                hash: if path.starts_with('@') {
                    super::verify::current_stamp(project, &path)?
                } else {
                    hash_file(&project.resolve(&path))?
                },
                path,
            });
        }
    }
    state.jobs[index].outputs = proofs;
    state.jobs[index].state = Status::Complete;
    state.jobs[index].message = "Verified outputs and registered media links".into();
    Ok(())
}
fn launch(
    project: &Project,
    command: &mut Command,
    dir: &Path,
    index: usize,
    state: &mut Ledger,
    events: bool,
) -> Result<()> {
    command
        .current_dir(&project.root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(
            File::create(dir.join("stderr.log")).map_err(|e| invalid(e.to_string()))?,
        ));
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
    let mut child = ChildGuard(
        command
            .spawn()
            .map_err(|e| invalid(format!("Cannot launch job adapter: {e}")))?,
    );
    let _tree = spearfia_ai::ProcessTree::attach(&child).map_err(|e| invalid(e.to_string()))?;
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(16);
    let log = dir.join("events.jsonl");
    std::thread::spawn(move || {
        let mut log = File::create(log).ok();
        let mut reader = BufReader::new(stdout);
        let mut logged = 0usize;
        loop {
            let mut line = String::new();
            let Ok(n) = (&mut reader).take(1024 * 1024 + 1).read_line(&mut line) else {
                break;
            };
            if n == 0 || line.len() > 1024 * 1024 {
                break;
            }
            logged = logged.saturating_add(line.len());
            if logged <= 64 * 1024 * 1024
                && let Some(log) = &mut log
            {
                use std::io::Write;
                let _ = writeln!(log, "{line}");
            }
            if events && tx.send(line).is_err() {
                break;
            }
        }
    });
    let started = Instant::now();
    let approved = project.approved_script()?;
    let mut stopping = None;
    let mut complete = !events;
    let mut error = None;
    state.jobs[index].state = Status::Running;
    save(project, state)?;
    loop {
        if project.root.join(CANCEL).exists()
            || project.root.join("sync/production/stop").exists()
            || project.approved_script().ok().as_ref() != Some(&approved)
            || started.elapsed() > Duration::from_secs(3600)
        {
            // Give the service adapter time to delete this queued ComfyUI prompt.
            // Synchronous VoiceStudio requests are killed locally after the grace period.
            if events {
                let stop = stopping.get_or_insert_with(Instant::now);
                let _ = fs::write(project.root.join(CANCEL), b"stop");
                if stop.elapsed() < Duration::from_secs(2) {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            }
            spearfia_ai::terminate(&mut child);
            state.jobs[index].state = if events && state.jobs[index].provider_id.is_empty() {
                Status::Uncertain
            } else {
                Status::Cancelled
            };
            save(project, state)?;
            return Err(invalid(
                "Job stopped. Reconcile an uncertain service request or resume its recorded provider ID.",
            ));
        }
        while let Ok(line) = rx.try_recv() {
            if events && let Ok(event) = serde_json::from_str::<Value>(&line) {
                if let Some(id) = event["provider_id"].as_str() {
                    state.jobs[index].provider_id = id.into();
                }
                if let Some(message) = event["message"].as_str() {
                    state.jobs[index].message = message.into();
                }
                match event["event"].as_str() {
                    Some("complete") => complete = true,
                    Some("error") => {
                        state.jobs[index].state = if event["uncertain"] == true {
                            Status::Uncertain
                        } else {
                            Status::Failed
                        };
                        error = Some(state.jobs[index].message.clone());
                    }
                    Some("cancelled") => {
                        state.jobs[index].state = Status::Cancelled;
                        error = Some(state.jobs[index].message.clone());
                    }
                    _ => {}
                }
                save(project, state)?;
            }
        }
        if let Some(status) = child.try_wait().map_err(|e| invalid(e.to_string()))? {
            // The reader may still have the final event in flight; drain until it closes.
            while let Ok(line) = rx.recv_timeout(Duration::from_millis(100)) {
                if events && let Ok(event) = serde_json::from_str::<Value>(&line) {
                    if let Some(id) = event["provider_id"].as_str() {
                        state.jobs[index].provider_id = id.into();
                    }
                    if event["event"] == "complete" {
                        complete = true;
                    }
                    if event["event"] == "error" {
                        state.jobs[index].state = if event["uncertain"] == true {
                            Status::Uncertain
                        } else {
                            Status::Failed
                        };
                        error = Some(event["message"].as_str().unwrap_or("Service error").into());
                    }
                    if event["event"] == "cancelled" {
                        state.jobs[index].state = Status::Cancelled;
                        error = Some(event["message"].as_str().unwrap_or("Cancelled").into());
                    }
                }
            }
            if let Some(error) = error {
                return Err(invalid(error));
            }
            if !status.success() || !complete {
                if events {
                    state.jobs[index].state = Status::Uncertain;
                }
                return Err(invalid(
                    "Adapter exited without a verified completion. Reconcile the service before retrying.",
                ));
            }
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
struct ChildGuard(std::process::Child);
impl std::ops::Deref for ChildGuard {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for ChildGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            spearfia_ai::terminate(&mut self.0);
        }
    }
}
fn verify_output(path: &Path, kind: &str) -> Result<()> {
    let meta = fs::metadata(path)
        .map_err(|e| invalid(format!("Missing output {}: {e}", path.display())))?;
    if !meta.is_file() || meta.len() == 0 {
        return Err(invalid("Generated artifact is empty."));
    }
    if ["text", "json"].contains(&kind) {
        let source = spearfia_core::channels::read_limited(path, 16 * 1024 * 1024)?;
        if source.trim().is_empty() {
            return Err(invalid("Generated document is empty."));
        }
        if kind == "json" {
            let _: Value = serde_json::from_str(&source).map_err(|e| invalid(e.to_string()))?;
        }
        return Ok(());
    }
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-of",
            "json",
            "-show_streams",
            "-show_format",
        ])
        .arg(path)
        .output()
        .map_err(|e| invalid(e.to_string()))?;
    if !output.status.success() {
        return Err(invalid("Generated media cannot be decoded by ffprobe."));
    }
    let data: Value = serde_json::from_slice(&output.stdout).map_err(|e| invalid(e.to_string()))?;
    let stream_kind = if kind == "audio" { "audio" } else { "video" };
    if !data["streams"]
        .as_array()
        .is_some_and(|v| v.iter().any(|s| s["codec_type"] == stream_kind))
    {
        return Err(invalid("Generated media has the wrong stream kind."));
    }
    if kind != "image"
        && !crate::timing::audio_seconds(path).is_some_and(|s| s.is_finite() && s > 0.0)
    {
        return Err(invalid("Generated timed media has no measurable duration."));
    }
    Ok(())
}
