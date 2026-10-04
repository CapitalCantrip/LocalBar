use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use localbar_core::executable::{self, TargetOs, PATH_ENV_VAR};
use localbar_core::launcher::{
    self, DetectionHost, DetectionStep, DetectionStop, InstallProgram, InstallStep, MlxDetection, PythonCandidate, BREW_COMMAND,
    MLX_LM_IMPORT_CHECK_ARGS, OLLAMA_DOWNLOAD_URL, UV_COMMAND,
};
use localbar_core::types::ServerType;

use crate::exec_path;

const IMPORT_CHECK_TIMEOUT: Duration = Duration::from_secs(15);
const INSTALL_STEP_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const OUTPUT_DRAIN_GRACE: Duration = Duration::from_secs(2);
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(50);
const HOME_ENV_VAR: &str = "HOME";
const XCODE_SELECT: &str = "/usr/bin/xcode-select";
const XCODE_SELECT_PRINT_PATH_ARG: &str = "-p";
const DEVELOPER_TOOLS_CHECK_TIMEOUT: Duration = Duration::from_secs(3);

pub const DETECTION_PROGRESS_EVENT: &str = "mlx-detection-progress";

static INSTALL_RUNNING: AtomicBool = AtomicBool::new(false);
static DETECTIONS: DetectionCoordinator = DetectionCoordinator::new();

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum MlxDetectionKindDto {
    Found,
    NotFound,
    DeadlinePassed,
    Cancelled,
}

#[derive(serde::Serialize, Clone, Debug)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MlxDetectionDto {
    pub kind: MlxDetectionKindDto,
    pub executable: Option<String>,
    pub message: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MlxDetectionProgressDto {
    pub message: String,
}

fn home_dir() -> Option<String> {
    std::env::var(HOME_ENV_VAR).ok()
}

fn detection_dto(outcome: &MlxDetection) -> MlxDetectionDto {
    let kind = match outcome {
        MlxDetection::Found { .. } => MlxDetectionKindDto::Found,
        MlxDetection::NotFound => MlxDetectionKindDto::NotFound,
        MlxDetection::DeadlinePassed => MlxDetectionKindDto::DeadlinePassed,
        MlxDetection::Cancelled => MlxDetectionKindDto::Cancelled,
    };
    MlxDetectionDto {
        kind,
        executable: outcome.executable().map(str::to_string),
        message: launcher::detection_outcome_line(outcome, home_dir().as_deref()),
    }
}

type CancelToken = Arc<AtomicBool>;

struct CoordinatorState {
    current: Option<CancelToken>,
    cache: Option<MlxDetection>,
}

pub struct DetectionCoordinator {
    state: Mutex<CoordinatorState>,
}

impl DetectionCoordinator {
    const fn new() -> Self {
        DetectionCoordinator { state: Mutex::new(CoordinatorState { current: None, cache: None }) }
    }

    fn begin(&self) -> CancelToken {
        let token = CancelToken::default();
        let mut state = self.state.lock().unwrap();
        if let Some(previous) = state.current.replace(token.clone()) {
            previous.store(true, Ordering::SeqCst);
        }
        token
    }

    fn cancel(&self) {
        if let Some(current) = self.state.lock().unwrap().current.take() {
            current.store(true, Ordering::SeqCst);
        }
    }

    fn finish(&self, token: &CancelToken, outcome: &MlxDetection) {
        let mut state = self.state.lock().unwrap();
        if !state.current.as_ref().is_some_and(|c| Arc::ptr_eq(c, token)) {
            return;
        }
        state.current = None;
        if outcome.is_worth_caching() {
            state.cache = Some(outcome.clone());
        }
    }

    fn cached(&self) -> Option<MlxDetection> {
        self.state.lock().unwrap().cache.clone()
    }

    fn invalidate(&self) {
        self.state.lock().unwrap().cache = None;
    }

    fn detect(&self, force: bool, run: impl FnOnce(&AtomicBool) -> MlxDetection) -> MlxDetection {
        if !force {
            if let Some(cached) = self.cached() {
                return cached;
            }
        }
        let token = self.begin();
        let outcome = run(&token);
        self.finish(&token, &outcome);
        outcome
    }
}

#[derive(serde::Serialize, Clone)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct InstallPlanDto {
    pub commands: Vec<String>,
    pub manual_url: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StepFailure {
    pub command: String,
    pub output: String,
}

impl StepFailure {
    pub fn message(&self) -> String {
        format!("`{}` failed:\n{}", self.command, self.output.trim())
    }
}

fn spawn_reader(mut stream: impl Read + Send + 'static, tx: std::sync::mpsc::Sender<Vec<u8>>) {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stream.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
}

fn wait_or_kill(child: &mut std::process::Child, timeout: Duration, stop: &dyn Fn() -> bool) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline && !stop() => std::thread::sleep(CHILD_POLL_INTERVAL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn run_with_timeout(cmd: Command, timeout: Duration) -> Result<String, String> {
    run_until(cmd, timeout, &|| false)
}

fn run_until(mut cmd: Command, timeout: Duration, stop: &dyn Fn() -> bool) -> Result<String, String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start: {e}"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    let streams = usize::from(child.stdout.is_some()) + usize::from(child.stderr.is_some());
    if let Some(out) = child.stdout.take() { spawn_reader(out, tx.clone()); }
    if let Some(err) = child.stderr.take() { spawn_reader(err, tx); }
    let status = wait_or_kill(&mut child, timeout, stop);
    let drain_deadline = Instant::now() + OUTPUT_DRAIN_GRACE;
    let output = (0..streams)
        .map_while(|_| rx.recv_timeout(drain_deadline.saturating_duration_since(Instant::now())).ok())
        .map(|buf| String::from_utf8_lossy(&buf).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    match status {
        Some(s) if s.success() => Ok(output),
        Some(s) => Err(format!("exited with {s}\n{output}")),
        None => Err(format!("timed out after {} s and was stopped\n{output}", timeout.as_secs())),
    }
}

fn python_imports_mlx_lm(python: &str, path_env: &str, stop: &dyn Fn() -> bool) -> bool {
    let mut cmd = Command::new(python);
    cmd.args(MLX_LM_IMPORT_CHECK_ARGS).env(PATH_ENV_VAR, path_env);
    run_until(cmd, IMPORT_CHECK_TIMEOUT, stop).is_ok()
}

fn subdir_names(parent: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(parent) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect()
}

fn expanded_python_candidates() -> Vec<String> {
    launcher::python_candidates(home_dir().as_deref(), TargetOs::current())
        .into_iter()
        .flat_map(|candidate| match candidate {
            PythonCandidate::Fixed(path) => vec![path],
            PythonCandidate::EachSubdir { parent, name_prefix, child } => {
                launcher::expand_subdir_candidate(&parent, name_prefix.as_deref(), &child, subdir_names(&parent))
            }
        })
        .collect()
}

fn xcode_select_print_path() -> Command {
    let mut cmd = Command::new(XCODE_SELECT);
    cmd.arg(XCODE_SELECT_PRINT_PATH_ARG);
    cmd
}

fn developer_tools_installed(os: TargetOs) -> bool {
    os != TargetOs::MacOs || run_with_timeout(xcode_select_print_path(), DEVELOPER_TOOLS_CHECK_TIMEOUT).is_ok()
}

fn run_detection(cancel: &AtomicBool, mut on_progress: impl FnMut(String)) -> MlxDetection {
    let deadline = Instant::now() + launcher::MAX_DETECTION_TIME;
    let stop_reason = || {
        if cancel.load(Ordering::SeqCst) {
            Some(DetectionStop::Cancelled)
        } else if Instant::now() >= deadline {
            Some(DetectionStop::DeadlinePassed)
        } else {
            None
        }
    };
    let search = exec_path::search_path();
    let path_env = executable::join_path(&search);
    let os = TargetOs::current();
    let host = DetectionHost { os, developer_tools_installed: developer_tools_installed(os) };
    let home = home_dir();
    launcher::detect_best_mlx_launcher(
        &search,
        &expanded_python_candidates(),
        host,
        exec_path::is_executable_file,
        stop_reason,
        |python| python_imports_mlx_lm(python, &path_env, &|| stop_reason().is_some()),
        |step: &DetectionStep| on_progress(launcher::detection_progress_line(step, home.as_deref())),
    )
}

pub fn detect_mlx_launcher(force: bool, on_progress: impl FnMut(String)) -> MlxDetectionDto {
    detection_dto(&DETECTIONS.detect(force, |cancel| run_detection(cancel, on_progress)))
}

pub fn cancel_mlx_detection() {
    DETECTIONS.cancel();
}

fn resolve_absolute(command: &str) -> Option<String> {
    exec_path::resolve_command(command).ok().map(|r| r.program.to_string_lossy().into_owned())
}

fn mlx_install_steps() -> Vec<InstallStep> {
    launcher::mlx_install_plan(resolve_absolute(UV_COMMAND).as_deref(), resolve_absolute(BREW_COMMAND).as_deref())
}

pub fn install_plan(server_type: ServerType) -> InstallPlanDto {
    match server_type {
        ServerType::MlxLm => InstallPlanDto { commands: mlx_install_steps().iter().map(InstallStep::display).collect(), manual_url: None },
        ServerType::Ollama => InstallPlanDto { commands: Vec::new(), manual_url: Some(OLLAMA_DOWNLOAD_URL.to_string()) },
        ServerType::External => InstallPlanDto { commands: Vec::new(), manual_url: None },
    }
}

fn run_install_steps(
    steps: &[InstallStep],
    path_env: impl Fn() -> String,
    resolve_uv: impl Fn() -> Option<PathBuf>,
    timeout: Duration,
) -> Result<String, StepFailure> {
    let mut log = String::new();
    for step in steps {
        let command = step.display();
        let program = match &step.program {
            InstallProgram::Path(p) => PathBuf::from(p),
            InstallProgram::UvAfterInstall => resolve_uv().ok_or_else(|| StepFailure {
                command: command.clone(),
                output: format!("uv was installed but could not be found afterwards.\n{log}"),
            })?,
        };
        let mut cmd = Command::new(&program);
        cmd.args(&step.args).env(PATH_ENV_VAR, path_env());
        let output = run_with_timeout(cmd, timeout).map_err(|output| StepFailure { command: command.clone(), output })?;
        log.push_str(&output);
    }
    Ok(log)
}

struct InstallGuard;

impl InstallGuard {
    fn acquire() -> Option<Self> {
        INSTALL_RUNNING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).ok().map(|_| InstallGuard)
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALL_RUNNING.store(false, Ordering::SeqCst);
    }
}

pub fn install_mlx_lm(on_progress: impl FnMut(String)) -> Result<String, String> {
    let _guard = InstallGuard::acquire().ok_or("An install is already running.")?;
    let steps = mlx_install_steps();
    run_install_steps(
        &steps,
        || executable::join_path(&exec_path::search_path()),
        || resolve_absolute(UV_COMMAND).map(PathBuf::from),
        INSTALL_STEP_TIMEOUT,
    )
    .map_err(|f| f.message())?;
    DETECTIONS.invalidate();
    let found = detect_mlx_launcher(true, on_progress);
    found.executable.ok_or_else(|| format!("mlx-lm was installed, but no launcher could be found afterwards. {}", found.message))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn write_script(body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("localbar-fake-tool-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn step(program: InstallProgram, args: &[&str]) -> InstallStep {
        InstallStep { program, args: args.iter().map(|a| a.to_string()).collect() }
    }

    fn path_env() -> String {
        "/usr/bin:/bin".to_string()
    }

    #[test]
    fn steps_run_in_order_and_uv_is_resolved_only_after_the_uv_install_step() {
        let marker = std::env::temp_dir().join(format!("localbar-fake-uv-installed-{}", uuid::Uuid::new_v4()));
        let installer = write_script(&format!("touch '{}'; echo installed-uv", marker.display()));
        let uv = write_script("echo \"uv $*\"");
        let steps = [
            step(InstallProgram::Path(installer.to_string_lossy().into_owned()), &[]),
            step(InstallProgram::UvAfterInstall, &["tool", "install", "mlx-lm"]),
        ];
        let result = run_install_steps(&steps, path_env, || marker.exists().then(|| uv.clone()), Duration::from_secs(10));
        for p in [&installer, &uv, &marker] { std::fs::remove_file(p).ok(); }
        let log = result.unwrap();
        assert!(log.contains("installed-uv"), "{log}");
        assert!(log.contains("uv tool install mlx-lm"), "{log}");
    }

    #[test]
    fn a_failing_step_stops_the_plan_and_reports_its_output() {
        let failing = write_script("echo boom >&2; exit 3");
        let never = write_script("echo should-not-run");
        let steps = [
            step(InstallProgram::Path(failing.to_string_lossy().into_owned()), &[]),
            step(InstallProgram::Path(never.to_string_lossy().into_owned()), &[]),
        ];
        let result = run_install_steps(&steps, path_env, || None, Duration::from_secs(10));
        for p in [&failing, &never] { std::fs::remove_file(p).ok(); }
        let failure = result.unwrap_err();
        assert_eq!(failure.command, failing.to_string_lossy());
        assert!(failure.output.contains("boom"), "{}", failure.output);
        assert!(!failure.output.contains("should-not-run"));
        assert!(failure.message().contains("boom"));
    }

    #[test]
    fn uv_that_cannot_be_found_after_installing_fails_the_tool_install_step() {
        let steps = [step(InstallProgram::UvAfterInstall, &["tool", "install", "mlx-lm"])];
        let failure = run_install_steps(&steps, path_env, || None, Duration::from_secs(10)).unwrap_err();
        assert_eq!(failure.command, "uv tool install mlx-lm");
        assert!(failure.output.contains("could not be found afterwards"), "{}", failure.output);
    }

    #[test]
    fn a_step_that_hangs_is_stopped_at_the_timeout() {
        let hang = write_script("exec sleep 30");
        let steps = [step(InstallProgram::Path(hang.to_string_lossy().into_owned()), &[])];
        let started = Instant::now();
        let result = run_install_steps(&steps, path_env, || None, Duration::from_millis(300));
        std::fs::remove_file(&hang).ok();
        assert!(result.unwrap_err().output.contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_python_without_mlx_lm_fails_the_import_check() {
        let python = write_script("exit 1");
        let ok = write_script("exit 0");
        let rejected = python_imports_mlx_lm(python.to_str().unwrap(), &path_env(), &|| false);
        let accepted = python_imports_mlx_lm(ok.to_str().unwrap(), &path_env(), &|| false);
        for p in [&python, &ok] { std::fs::remove_file(p).ok(); }
        assert!(!rejected);
        assert!(accepted);
    }

    #[test]
    fn only_one_install_can_run_at_a_time() {
        let first = InstallGuard::acquire();
        assert!(first.is_some());
        assert!(InstallGuard::acquire().is_none());
        drop(first);
        assert!(InstallGuard::acquire().is_some());
    }

    #[test]
    fn a_running_import_check_is_killed_as_soon_as_detection_stops() {
        let hang = write_script("exec sleep 30");
        let started = Instant::now();
        let accepted = python_imports_mlx_lm(hang.to_str().unwrap(), &path_env(), &|| started.elapsed() > Duration::from_millis(200));
        std::fs::remove_file(&hang).ok();
        assert!(!accepted);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    fn uv_found() -> MlxDetection {
        MlxDetection::Found { executable: "uv".into(), path: "/a/uv".into() }
    }

    #[test]
    fn starting_a_detection_cancels_the_one_already_running() {
        let coordinator = DetectionCoordinator::new();
        let first = coordinator.begin();
        let second = coordinator.begin();
        assert!(first.load(Ordering::SeqCst));
        assert!(!second.load(Ordering::SeqCst));
        coordinator.cancel();
        assert!(second.load(Ordering::SeqCst));
    }

    #[test]
    fn a_finished_detection_is_reused_until_forced_or_invalidated() {
        let coordinator = DetectionCoordinator::new();
        let runs = std::cell::Cell::new(0);
        let run = |_: &AtomicBool| { runs.set(runs.get() + 1); uv_found() };
        assert_eq!(coordinator.detect(false, run), uv_found());
        assert_eq!(coordinator.detect(false, run), uv_found());
        assert_eq!(runs.get(), 1);
        coordinator.detect(true, run);
        assert_eq!(runs.get(), 2);
        coordinator.invalidate();
        coordinator.detect(false, run);
        assert_eq!(runs.get(), 3);
    }

    #[test]
    fn a_cancelled_detection_is_not_cached() {
        let coordinator = DetectionCoordinator::new();
        coordinator.detect(false, |_| MlxDetection::Cancelled);
        assert_eq!(coordinator.cached(), None);
    }

    #[test]
    fn a_superseded_detection_never_overwrites_the_newer_result() {
        let coordinator = DetectionCoordinator::new();
        let old = coordinator.begin();
        let newer = coordinator.begin();
        coordinator.finish(&old, &MlxDetection::NotFound);
        assert_eq!(coordinator.cached(), None);
        coordinator.finish(&newer, &uv_found());
        assert_eq!(coordinator.cached(), Some(uv_found()));
    }
}
