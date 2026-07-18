#![cfg_attr(test, allow(dead_code))]

use std::{
    collections::{BTreeMap, HashMap},
    env, fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
#[cfg(not(test))]
use tauri::{Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::{mpsc, oneshot},
    time::{interval, Instant},
};
use uuid::Uuid;

use crate::{
    blueprint::{load_blueprint, validate_blueprint_path},
    error::AppError,
    logging::redact_text,
};

const MAX_HISTORY: usize = 100;
const MAX_OUTPUT_LINES: usize = 500;
const MAX_LINE_CHARS: usize = 4_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDefinition {
    pub id: String,
    pub label: String,
    pub description: String,
    pub executable: String,
    pub arguments: Vec<String>,
    pub timeout_seconds: u64,
    pub requires_package_script: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OutputStream {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutput {
    pub sequence: u64,
    pub stream: OutputStream,
    pub line: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTask {
    pub id: String,
    pub workflow_id: String,
    pub label: String,
    pub working_directory: String,
    pub status: WorkflowStatus,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub message: Option<String>,
    pub output: Vec<WorkflowOutput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowTaskEvent {
    task: WorkflowTask,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowOutputEvent {
    task_id: String,
    output: WorkflowOutput,
}

pub struct WorkflowState {
    history_path: PathBuf,
    tasks: Mutex<BTreeMap<String, WorkflowTask>>,
    cancellations: Mutex<HashMap<String, oneshot::Sender<()>>>,
    active_pids: Mutex<HashMap<String, u32>>,
}

impl WorkflowState {
    pub fn open(history_path: PathBuf) -> Result<Self, AppError> {
        if !history_path.is_absolute() {
            return Err(AppError::InvalidPath);
        }
        let mut tasks = read_history(&history_path)?;
        let now = Utc::now().to_rfc3339();
        for task in tasks.values_mut() {
            if task.status == WorkflowStatus::Running {
                task.status = WorkflowStatus::Failed;
                task.finished_at = Some(now.clone());
                task.message = Some("Interrupted by a previous application shutdown".into());
                task.output.push(WorkflowOutput {
                    sequence: next_sequence(task),
                    stream: OutputStream::System,
                    line: "Task was interrupted by a previous application shutdown".into(),
                    timestamp: now.clone(),
                });
            }
        }
        let state = Self {
            history_path,
            tasks: Mutex::new(tasks),
            cancellations: Mutex::new(HashMap::new()),
            active_pids: Mutex::new(HashMap::new()),
        };
        state.persist()?;
        Ok(state)
    }

    pub fn tasks(&self) -> Result<Vec<WorkflowTask>, AppError> {
        let mut tasks: Vec<_> = self.lock_tasks()?.values().cloned().collect();
        tasks.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        Ok(tasks)
    }

    fn lock_tasks(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, BTreeMap<String, WorkflowTask>>, AppError> {
        self.tasks.lock().map_err(|_| AppError::Internal)
    }

    fn persist(&self) -> Result<(), AppError> {
        let mut tasks: Vec<_> = self.lock_tasks()?.values().cloned().collect();
        tasks.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        tasks.truncate(MAX_HISTORY);
        write_history(&self.history_path, &tasks)
    }

    fn insert(&self, task: WorkflowTask) -> Result<(), AppError> {
        let mut tasks = self.lock_tasks()?;
        if tasks.values().any(|existing| {
            existing.status == WorkflowStatus::Running
                && existing.workflow_id == task.workflow_id
                && existing.working_directory == task.working_directory
        }) {
            return Err(AppError::Conflict);
        }
        tasks.insert(task.id.clone(), task);
        drop(tasks);
        self.persist()
    }

    fn append_output(
        &self,
        task_id: &str,
        stream: OutputStream,
        line: String,
    ) -> Result<WorkflowOutput, AppError> {
        let mut tasks = self.lock_tasks()?;
        let task = tasks.get_mut(task_id).ok_or(AppError::NotFound)?;
        let output = WorkflowOutput {
            sequence: next_sequence(task),
            stream,
            line: redact_text(&truncate_line(&line)),
            timestamp: Utc::now().to_rfc3339(),
        };
        task.output.push(output.clone());
        if task.output.len() > MAX_OUTPUT_LINES {
            task.output.drain(0..task.output.len() - MAX_OUTPUT_LINES);
        }
        Ok(output)
    }

    fn finish(&self, task_id: &str, outcome: ProcessOutcome) -> Result<WorkflowTask, AppError> {
        let mut tasks = self.lock_tasks()?;
        let task = tasks.get_mut(task_id).ok_or(AppError::NotFound)?;
        task.status = outcome.status;
        task.exit_code = outcome.exit_code;
        task.finished_at = Some(Utc::now().to_rfc3339());
        task.message = outcome.message.map(|value| redact_text(&value));
        let result = task.clone();
        drop(tasks);
        if let Ok(mut values) = self.cancellations.lock() {
            values.remove(task_id);
        }
        if let Ok(mut values) = self.active_pids.lock() {
            values.remove(task_id);
        }
        self.persist()?;
        Ok(result)
    }
}

impl Drop for WorkflowState {
    fn drop(&mut self) {
        if let Ok(pids) = self.active_pids.lock() {
            for pid in pids.values().copied() {
                terminate_tree_sync(pid);
            }
        }
    }
}

pub fn workflow_definitions() -> Vec<WorkflowDefinition> {
    vec![
        definition(
            "npm-install",
            "Install dependencies",
            "Install pinned dependencies declared by the target package",
            &["install"],
            900,
            None,
        ),
        definition(
            "npm-typecheck",
            "Typecheck target",
            "Run the registered typecheck package script",
            &["run", "typecheck"],
            300,
            Some("typecheck"),
        ),
        definition(
            "npm-test",
            "Test target",
            "Run the registered test package script",
            &["run", "test"],
            600,
            Some("test"),
        ),
        definition(
            "npm-build",
            "Build target",
            "Run the registered production build package script",
            &["run", "build"],
            600,
            Some("build"),
        ),
    ]
}

fn definition(
    id: &str,
    label: &str,
    description: &str,
    arguments: &[&str],
    timeout_seconds: u64,
    requires_package_script: Option<&str>,
) -> WorkflowDefinition {
    WorkflowDefinition {
        id: id.into(),
        label: label.into(),
        description: description.into(),
        executable: "npm".into(),
        arguments: arguments.iter().map(|value| (*value).into()).collect(),
        timeout_seconds,
        requires_package_script: requires_package_script.map(Into::into),
    }
}

#[cfg(not(test))]
pub fn start_workflow(
    app: AppHandle,
    project_path: String,
    workflow_id: String,
    requested_working_directory: Option<String>,
) -> Result<WorkflowTask, AppError> {
    let definition = workflow_definitions()
        .into_iter()
        .find(|value| value.id == workflow_id)
        .ok_or(AppError::NotFound)?;
    let working_directory = resolve_working_directory(
        Path::new(&project_path),
        requested_working_directory.as_deref(),
    )?;
    validate_package_workflow(&working_directory, &definition)?;
    let (executable, mut arguments) = resolve_npm_runtime(&working_directory)?;
    arguments.extend(definition.arguments.clone());
    let task = WorkflowTask {
        id: Uuid::new_v4().to_string(),
        workflow_id: definition.id.clone(),
        label: definition.label.clone(),
        working_directory: working_directory.to_string_lossy().into_owned(),
        status: WorkflowStatus::Running,
        started_at: Utc::now().to_rfc3339(),
        finished_at: None,
        exit_code: None,
        message: None,
        output: Vec::new(),
    };
    let state = app.state::<WorkflowState>();
    state.insert(task.clone())?;
    let (cancel_tx, cancel_rx) = oneshot::channel();
    state
        .cancellations
        .lock()
        .map_err(|_| AppError::Internal)?
        .insert(task.id.clone(), cancel_tx);
    let task_id = task.id.clone();
    let timeout = Duration::from_secs(definition.timeout_seconds);
    tauri::async_runtime::spawn(async move {
        run_managed_task(
            app,
            task_id,
            executable,
            arguments,
            working_directory,
            timeout,
            cancel_rx,
        )
        .await;
    });
    Ok(task)
}

#[cfg(test)]
pub fn start_workflow(
    _app: AppHandle,
    _project_path: String,
    _workflow_id: String,
    _requested_working_directory: Option<String>,
) -> Result<WorkflowTask, AppError> {
    Err(AppError::Internal)
}

pub fn cancel_workflow(state: &WorkflowState, task_id: &str) -> Result<(), AppError> {
    let sender = state
        .cancellations
        .lock()
        .map_err(|_| AppError::Internal)?
        .remove(task_id)
        .ok_or(AppError::NotFound)?;
    sender.send(()).map_err(|_| AppError::Conflict)
}

#[cfg(not(test))]
async fn run_managed_task(
    app: AppHandle,
    task_id: String,
    executable: PathBuf,
    arguments: Vec<String>,
    working_directory: PathBuf,
    timeout: Duration,
    cancel_rx: oneshot::Receiver<()>,
) {
    let (output_tx, mut output_rx) = mpsc::unbounded_channel();
    let process = tokio::spawn(execute_process(
        executable,
        arguments,
        working_directory,
        timeout,
        cancel_rx,
        output_tx,
        Some({
            let process_app = app.clone();
            let process_task_id = task_id.clone();
            Arc::new(move |pid| {
                if let Ok(mut values) = process_app.state::<WorkflowState>().active_pids.lock() {
                    values.insert(process_task_id.clone(), pid);
                }
            })
        }),
    ));
    tokio::pin!(process);
    let outcome = loop {
        tokio::select! {
            Some((stream, line)) = output_rx.recv() => {
                emit_output(&app, &task_id, stream, line);
            }
            result = &mut process => {
                break result.unwrap_or(ProcessOutcome::failed("Workflow worker stopped unexpectedly"));
            }
        }
    };
    while let Ok((stream, line)) = output_rx.try_recv() {
        emit_output(&app, &task_id, stream, line);
    }
    let state = app.state::<WorkflowState>();
    if let Ok(task) = state.finish(&task_id, outcome) {
        let _ = app.emit("workflow://task", WorkflowTaskEvent { task });
    }
}

#[cfg(not(test))]
fn emit_output(app: &AppHandle, task_id: &str, stream: OutputStream, line: String) {
    let state = app.state::<WorkflowState>();
    if let Ok(output) = state.append_output(task_id, stream, line) {
        let _ = app.emit(
            "workflow://output",
            WorkflowOutputEvent {
                task_id: task_id.into(),
                output,
            },
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessOutcome {
    status: WorkflowStatus,
    exit_code: Option<i32>,
    message: Option<String>,
}

impl ProcessOutcome {
    fn failed(message: &str) -> Self {
        Self {
            status: WorkflowStatus::Failed,
            exit_code: None,
            message: Some(message.into()),
        }
    }
}

async fn execute_process(
    executable: PathBuf,
    arguments: Vec<String>,
    working_directory: PathBuf,
    timeout: Duration,
    mut cancel_rx: oneshot::Receiver<()>,
    output_tx: mpsc::UnboundedSender<(OutputStream, String)>,
    process_started: Option<Arc<dyn Fn(u32) + Send + Sync>>,
) -> ProcessOutcome {
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .current_dir(working_directory)
        .env_clear()
        .envs(sanitized_environment())
        .env("CI", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    configure_process_group(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return ProcessOutcome::failed("Unable to start the allowlisted executable"),
    };
    let pid = child.id();
    if let (Some(pid), Some(callback)) = (pid, process_started.as_ref()) {
        callback(pid);
    }
    if let Some(stdout) = child.stdout.take() {
        let tx = output_tx.clone();
        tokio::spawn(read_output(stdout, OutputStream::Stdout, tx));
    }
    if let Some(stderr) = child.stderr.take() {
        let tx = output_tx.clone();
        tokio::spawn(read_output(stderr, OutputStream::Stderr, tx));
    }
    let started = Instant::now();
    let mut ticker = interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            _ = &mut cancel_rx => {
                terminate_process_tree(&mut child).await;
                return ProcessOutcome { status: WorkflowStatus::Cancelled, exit_code: None, message: Some("Cancelled by user".into()) };
            }
            _ = ticker.tick() => {
                if started.elapsed() >= timeout {
                    terminate_process_tree(&mut child).await;
                    return ProcessOutcome { status: WorkflowStatus::TimedOut, exit_code: None, message: Some("Workflow exceeded its timeout".into()) };
                }
                match child.try_wait() {
                    Ok(Some(status)) => return ProcessOutcome {
                        status: if status.success() { WorkflowStatus::Succeeded } else { WorkflowStatus::Failed },
                        exit_code: status.code(),
                        message: (!status.success()).then(|| "Workflow exited with a non-zero status".into()),
                    },
                    Ok(None) => {}
                    Err(_) => return ProcessOutcome::failed("Unable to read workflow process status"),
                }
            }
        }
    }
}

async fn read_output<R>(
    reader: R,
    stream: OutputStream,
    sender: mpsc::UnboundedSender<(OutputStream, String)>,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buffer)
                    .trim_end_matches(['\r', '\n'])
                    .to_string();
                let _ = sender.send((stream, line));
            }
        }
    }
}

fn resolve_working_directory(
    project_path: &Path,
    requested: Option<&str>,
) -> Result<PathBuf, AppError> {
    validate_blueprint_path(project_path)?;
    let project_file = project_path
        .canonicalize()
        .map_err(|_| AppError::InvalidPath)?;
    let blueprint = load_blueprint(&project_file)?;
    let project_root = project_file
        .parent()
        .ok_or(AppError::InvalidPath)?
        .to_path_buf();
    let target_root = blueprint
        .target_directory
        .as_deref()
        .map(PathBuf::from)
        .map(|path| {
            if !path.is_absolute() {
                return Err(AppError::InvalidPath);
            }
            path.canonicalize().map_err(|_| AppError::InvalidPath)
        })
        .transpose()?;
    let candidate = requested
        .map(PathBuf::from)
        .or_else(|| target_root.clone())
        .unwrap_or_else(|| project_root.clone());
    if !candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(AppError::InvalidPath);
    }
    let candidate = candidate
        .canonicalize()
        .map_err(|_| AppError::InvalidPath)?;
    if !candidate.is_dir()
        || (!candidate.starts_with(&project_root)
            && !target_root
                .as_ref()
                .is_some_and(|root| candidate.starts_with(root)))
    {
        return Err(AppError::InvalidPath);
    }
    Ok(candidate)
}

fn validate_package_workflow(
    working_directory: &Path,
    definition: &WorkflowDefinition,
) -> Result<(), AppError> {
    let package_path = working_directory.join("package.json");
    let metadata = fs::metadata(&package_path).map_err(|_| AppError::NotFound)?;
    if !metadata.is_file() || metadata.len() > 2_097_152 {
        return Err(AppError::Validation);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(package_path)?).map_err(|_| AppError::Validation)?;
    if let Some(script) = &definition.requires_package_script {
        if !value
            .get("scripts")
            .and_then(|scripts| scripts.get(script))
            .and_then(|value| value.as_str())
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(AppError::Validation);
        }
    }
    Ok(())
}

fn resolve_npm_runtime(working_directory: &Path) -> Result<(PathBuf, Vec<String>), AppError> {
    let node = resolve_path_executable(node_file_name(), working_directory)?;
    let npm_cli = npm_cli_candidates(&node)
        .into_iter()
        .find(|candidate| candidate.is_file())
        .or_else(|| resolve_npm_cli_from_path(working_directory))
        .ok_or(AppError::NotFound)?;
    let npm_cli = npm_cli.canonicalize().map_err(|_| AppError::InvalidPath)?;
    if npm_cli.starts_with(working_directory) {
        return Err(AppError::InvalidPath);
    }
    Ok((node, vec![npm_cli.to_string_lossy().into_owned()]))
}

fn resolve_path_executable(name: &str, working_directory: &Path) -> Result<PathBuf, AppError> {
    let path = env::var_os("PATH").ok_or(AppError::NotFound)?;
    for directory in env::split_paths(&path) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            let candidate = candidate
                .canonicalize()
                .map_err(|_| AppError::InvalidPath)?;
            if !candidate.starts_with(working_directory) {
                return Ok(candidate);
            }
        }
    }
    Err(AppError::NotFound)
}

#[cfg(windows)]
fn node_file_name() -> &'static str {
    "node.exe"
}

#[cfg(not(windows))]
fn node_file_name() -> &'static str {
    "node"
}

fn npm_cli_candidates(node: &Path) -> Vec<PathBuf> {
    let parent = node.parent().unwrap_or(Path::new(""));
    vec![
        parent.join("node_modules/npm/bin/npm-cli.js"),
        parent.join("../lib/node_modules/npm/bin/npm-cli.js"),
        parent.join("../share/nodejs/npm/bin/npm-cli.js"),
    ]
}

fn resolve_npm_cli_from_path(working_directory: &Path) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        #[cfg(windows)]
        let candidate = directory.join("npm.cmd");
        #[cfg(not(windows))]
        let candidate = directory.join("npm");
        if candidate.exists() {
            if let Ok(canonical) = candidate.canonicalize() {
                if canonical.is_file()
                    && !canonical.starts_with(working_directory)
                    && canonical.extension().and_then(|value| value.to_str()) == Some("js")
                {
                    return Some(canonical);
                }
            }
        }
    }
    None
}

fn sanitized_environment() -> Vec<(String, String)> {
    const ALLOWED: &[&str] = &[
        "PATH",
        "Path",
        "PATHEXT",
        "SystemRoot",
        "WINDIR",
        "COMSPEC",
        "TEMP",
        "TMP",
        "TMPDIR",
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "LANG",
        "LC_ALL",
    ];
    ALLOWED
        .iter()
        .filter_map(|key| env::var(key).ok().map(|value| ((*key).into(), value)))
        .collect()
}

fn truncate_line(value: &str) -> String {
    let mut chars = value.chars();
    let output: String = chars.by_ref().take(MAX_LINE_CHARS).collect();
    if chars.next().is_some() {
        format!("{output}… [truncated]")
    } else {
        output
    }
}

fn next_sequence(task: &WorkflowTask) -> u64 {
    task.output.last().map_or(1, |value| value.sequence + 1)
}

fn read_history(path: &Path) -> Result<BTreeMap<String, WorkflowTask>, AppError> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let bytes = fs::read(path)?;
    let values: Vec<WorkflowTask> = match serde_json::from_slice(&bytes) {
        Ok(values) => values,
        Err(_) => {
            let archive = path.with_file_name(format!(
                "workflow-history.corrupt-{}.json",
                Utc::now().format("%Y%m%dT%H%M%S%.3fZ")
            ));
            let _ = fs::rename(path, archive);
            return Ok(BTreeMap::new());
        }
    };
    Ok(values
        .into_iter()
        .map(|task| (task.id.clone(), task))
        .collect())
}

fn write_history(path: &Path, tasks: &[WorkflowTask]) -> Result<(), AppError> {
    let parent = path.parent().ok_or(AppError::InvalidPath)?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".workflow-history.{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(tasks).map_err(|_| AppError::Internal)?;
    let mut file = fs::File::create(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    let previous = parent.join(format!(".workflow-history.{}.previous", Uuid::new_v4()));
    let had_previous = path.exists();
    if had_previous {
        fs::rename(path, &previous)?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        if had_previous {
            let _ = fs::rename(&previous, path);
        }
        let _ = fs::remove_file(&temp);
        return Err(error.into());
    }
    if had_previous {
        let _ = fs::remove_file(previous);
    }
    Ok(())
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.as_std_mut().process_group(0);
}

#[cfg(windows)]
fn configure_process_group(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
}

async fn terminate_process_tree(child: &mut Child) {
    if let Some(pid) = child.id() {
        terminate_tree_async(pid).await;
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[cfg(windows)]
async fn terminate_tree_async(pid: u32) {
    let root = env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let executable = PathBuf::from(root).join("System32/taskkill.exe");
    let _ = Command::new(executable)
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

#[cfg(unix)]
async fn terminate_tree_async(pid: u32) {
    let group = format!("-{pid}");
    let _ = Command::new("/bin/kill")
        .args(["-TERM", &group])
        .status()
        .await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    let _ = Command::new("/bin/kill")
        .args(["-KILL", &group])
        .status()
        .await;
}

#[cfg(windows)]
fn terminate_tree_sync(pid: u32) {
    let root = env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let executable = PathBuf::from(root).join("System32/taskkill.exe");
    let _ = std::process::Command::new(executable)
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(unix)]
fn terminate_tree_sync(pid: u32) {
    let _ = std::process::Command::new("/bin/kill")
        .args(["-KILL", &format!("-{pid}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: WorkflowStatus) -> WorkflowTask {
        WorkflowTask {
            id: id.into(),
            workflow_id: "npm-build".into(),
            label: "Build".into(),
            working_directory: "[fixture]".into(),
            status,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            exit_code: None,
            message: None,
            output: Vec::new(),
        }
    }

    #[test]
    fn definitions_never_expose_shell_strings() {
        for workflow in workflow_definitions() {
            assert_eq!(workflow.executable, "npm");
            assert!(!workflow.arguments.is_empty());
            assert!(workflow.arguments.iter().all(|value| {
                !value.contains("&&") && !value.contains(';') && !value.contains('|')
            }));
        }
    }

    #[test]
    fn package_script_and_working_directory_are_validated() {
        let project = tempfile::tempdir().unwrap();
        let target = project.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(
            target.join("package.json"),
            r#"{"scripts":{"build":"vite build"}}"#,
        )
        .unwrap();
        let build = workflow_definitions()
            .into_iter()
            .find(|value| value.id == "npm-build")
            .unwrap();
        let test = workflow_definitions()
            .into_iter()
            .find(|value| value.id == "npm-test")
            .unwrap();
        assert!(validate_package_workflow(&target, &build).is_ok());
        assert!(matches!(
            validate_package_workflow(&target, &test),
            Err(AppError::Validation)
        ));
    }

    #[test]
    fn history_is_replaced_transactionally_and_corruption_is_archived() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workflow-history.json");
        write_history(&path, &[task("first", WorkflowStatus::Succeeded)]).unwrap();
        write_history(&path, &[task("second", WorkflowStatus::Failed)]).unwrap();
        let history = read_history(&path).unwrap();
        assert_eq!(history.len(), 1);
        assert!(history.contains_key("second"));

        fs::write(&path, b"{not-json").unwrap();
        assert!(read_history(&path).unwrap().is_empty());
        assert!(!path.exists());
        assert!(fs::read_dir(directory.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("workflow-history.corrupt-")
        }));
    }

    #[tokio::test]
    async fn process_output_is_streamed_and_timeout_cleans_up() {
        let directory = tempfile::tempdir().unwrap();
        let node = resolve_path_executable(node_file_name(), directory.path()).unwrap();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let (_cancel_tx, cancel_rx) = oneshot::channel();
        let outcome = execute_process(
            node,
            vec![
                "-e".into(),
                "console.log('token:visible-secret');setTimeout(()=>{},5000)".into(),
            ],
            directory.path().to_path_buf(),
            Duration::from_millis(250),
            cancel_rx,
            sender,
            None,
        )
        .await;
        assert_eq!(outcome.status, WorkflowStatus::TimedOut);
        let lines: Vec<_> = std::iter::from_fn(|| receiver.try_recv().ok()).collect();
        assert!(lines
            .iter()
            .any(|(_, line)| line.contains("visible-secret")));
        assert!(!redact_text(&lines[0].1).contains("visible-secret"));
    }

    #[tokio::test]
    async fn cancellation_returns_cancelled_state() {
        let directory = tempfile::tempdir().unwrap();
        let node = resolve_path_executable(node_file_name(), directory.path()).unwrap();
        let (sender, _receiver) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            let _ = cancel_tx.send(());
        });
        let outcome = execute_process(
            node,
            vec!["-e".into(), "setTimeout(()=>{},5000)".into()],
            directory.path().to_path_buf(),
            Duration::from_secs(5),
            cancel_rx,
            sender,
            None,
        )
        .await;
        assert_eq!(outcome.status, WorkflowStatus::Cancelled);
    }
}
