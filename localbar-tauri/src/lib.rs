#![deny(clippy::cognitive_complexity)]
#![deny(clippy::too_many_lines)]

use std::collections::HashMap;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Listener, Manager, State, WindowEvent};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use uuid::Uuid;

use localbar_core::driver::{HealthStatus, ModelMetadata, ServerDriver};
use localbar_core::drivers::external::ExternalDriver;
use localbar_core::drivers::mlx_lm::MLXLMDriver;
use localbar_core::drivers::ollama::{self, OllamaDriver};
use localbar_core::lifecycle::{self, displayed_pid, LifecycleEvent, StopKill, PollContext, PollOutcome, SwitchPlan};
use localbar_core::net::port_is_open;
use localbar_core::persistence::FilePersistence;
use localbar_core::quit::{ids_to_forget_reconnect_on_launch, ids_to_start_on_launch, ids_to_stop_on_quit, quit_sequence_for, quit_shutdown_budget_secs, ExitSignal, QuitCandidate, QuitSequence};
use localbar_core::registry::InstanceRegistry;
use localbar_core::types::{
    AppSettings, DiscoveryConfig, InstanceError, InstanceErrorKind, InstancePhase, ModelMemoryKey, ModelRef,
    ParamValues, ServerInstanceConfig, ServerType,
};
use localbar_core::{adopt_external_as_new_instance, set_active_profile, set_instance_params};

#[derive(serde::Serialize, Clone)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ErrorKindDto {
    LaunchFailed,
    PortConflict { port: u16 },
    HealthCheckFailed,
    StopFailed,
    ModelSwitchFailed,
    Unexpected,
}

#[derive(serde::Serialize, Clone)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InstancePhaseDto {
    Stopped,
    Starting,
    Running,
    Stopping,
    SwitchingModel,
    Error { kind: ErrorKindDto, message: String },
}

#[derive(serde::Serialize, Clone, Copy)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct InstancePidDto {
    pub pid: u32,
    pub adopted: bool,
}

#[derive(serde::Serialize, Clone)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AdoptResultDto {
    pub id: String,
    pub warning: Option<String>,
}

#[derive(serde::Serialize, Clone)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ReconnectNoticeDto {
    pub id: String,
    pub seq: u32,
    pub message: String,
}

#[derive(Default)]
pub struct ReconnectNotices {
    next_seq: u32,
    latest: HashMap<Uuid, ReconnectNoticeDto>,
}

impl ReconnectNotices {
    fn record(&mut self, id: Uuid, message: String) -> ReconnectNoticeDto {
        self.next_seq += 1;
        let notice = ReconnectNoticeDto { id: id.to_string(), seq: self.next_seq, message };
        self.latest.insert(id, notice.clone());
        notice
    }
}

pub struct AppState {
    pub registry: Mutex<InstanceRegistry>,
    pub processes: Mutex<HashMap<Uuid, Child>>,
    pub load_error: Option<String>,
    pub quitting: AtomicBool,
    pub reconnect_notices: Mutex<ReconnectNotices>,
}

impl AppState {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        let persistence = FilePersistence::new(data_dir.join("state.json"));
        let registry = InstanceRegistry::new(Box::new(persistence));
        Self {
            registry: Mutex::new(registry),
            processes: Mutex::new(HashMap::new()),
            load_error: None,
            quitting: AtomicBool::new(false),
            reconnect_notices: Mutex::new(ReconnectNotices::default()),
        }
    }
}

fn driver_for(config: &ServerInstanceConfig, discovery: &DiscoveryConfig) -> Box<dyn ServerDriver> {
    match config.server_type {
        ServerType::Ollama => Box::new(OllamaDriver),
        ServerType::MlxLm => {
            let paths = discovery.resolved_mlx_paths(config.model_search_path_override.as_deref());
            Box::new(MLXLMDriver::new(paths))
        }
        ServerType::External => Box::new(ExternalDriver),
    }
}

fn parse_uuid(id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(id).map_err(|e| format!("invalid id: {e}"))
}

fn parse_server_type(s: &str) -> Result<ServerType, String> {
    match s {
        "ollama" => Ok(ServerType::Ollama),
        "mlx-lm" => Ok(ServerType::MlxLm),
        "external" => Ok(ServerType::External),
        _ => Err(format!("unknown server type: {s}")),
    }
}

fn error_kind_to_dto(kind: &InstanceErrorKind) -> ErrorKindDto {
    match kind {
        InstanceErrorKind::LaunchFailed => ErrorKindDto::LaunchFailed,
        InstanceErrorKind::PortConflict { port } => ErrorKindDto::PortConflict { port: *port },
        InstanceErrorKind::HealthCheckFailed => ErrorKindDto::HealthCheckFailed,
        InstanceErrorKind::StopFailed => ErrorKindDto::StopFailed,
        InstanceErrorKind::ModelSwitchFailed => ErrorKindDto::ModelSwitchFailed,
        InstanceErrorKind::Unexpected => ErrorKindDto::Unexpected,
    }
}

fn phase_to_dto(phase: &InstancePhase) -> InstancePhaseDto {
    match phase {
        InstancePhase::Stopped => InstancePhaseDto::Stopped,
        InstancePhase::Starting => InstancePhaseDto::Starting,
        InstancePhase::Running => InstancePhaseDto::Running,
        InstancePhase::Stopping => InstancePhaseDto::Stopping,
        InstancePhase::SwitchingModel => InstancePhaseDto::SwitchingModel,
        InstancePhase::Error(e) => InstancePhaseDto::Error {
            kind: error_kind_to_dto(&e.kind),
            message: e.message.clone(),
        },
    }
}

fn instance_pid_to_dto(pid: lifecycle::InstancePid) -> InstancePidDto {
    InstancePidDto { pid: pid.pid, adopted: pid.adopted }
}

fn spawn_from_plan(plan: &localbar_core::driver::LaunchPlan) -> Result<Child, String> {
    let mut cmd = std::process::Command::new(&plan.executable);
    cmd.args(&plan.arguments);
    for (k, v) in &plan.environment {
        cmd.env(k, v);
    }
    if let Some(dir) = &plan.working_directory {
        cmd.current_dir(dir);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    cmd.spawn().map_err(|e| format!("spawn failed: {e}"))
}

#[cfg(unix)]
fn find_listening_pid(port: u16) -> Option<u32> {
    let output = std::process::Command::new("lsof")
        .args(["-nP", "-t", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .and_then(|line| line.trim().parse::<u32>().ok())
}

#[cfg(not(unix))]
fn find_listening_pid(_port: u16) -> Option<u32> {
    None
}

#[cfg(unix)]
fn ps_args_for_pid(pid: u32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(not(unix))]
fn ps_args_for_pid(_pid: u32) -> Option<String> {
    None
}

#[cfg(unix)]
fn pid_is_alive(pid: u32) -> bool {
    // SAFETY: kill(2) with signal 0 only probes for the pid's existence and permission; it sends no signal.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(unix)]
fn kill_by_pid(pid: u32, grace_secs: f64) -> Result<(), String> {
    // SAFETY: kill(2) is always safe to call with a valid pid and SIGTERM.
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); }
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs_f64(grace_secs.clamp(0.0, 60.0));
    while pid_is_alive(pid) {
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if !pid_is_alive(pid) {
        return Ok(());
    }
    // SAFETY: kill(2) is always safe to call with a valid pid and SIGKILL.
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL); }
    std::thread::sleep(std::time::Duration::from_millis(200));
    if pid_is_alive(pid) {
        Err(format!("process {pid} survived SIGKILL"))
    } else {
        Ok(())
    }
}

#[cfg(not(unix))]
fn kill_by_pid(_pid: u32, _grace_secs: f64) -> Result<(), String> {
    Err("stopping adopted processes is not supported on this platform".to_string())
}

fn graceful_kill(mut child: Child, grace_secs: f64) {
    #[cfg(unix)]
    {
        // SAFETY: kill(2) is always safe to call with a valid pid and SIGTERM.
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM); }
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_secs_f64(grace_secs.clamp(0.0, 60.0));
        loop {
            if child.try_wait().map(|s| s.is_some()).unwrap_or(true) {
                return;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn set_phase_emit(app: &AppHandle, id: Uuid, phase: InstancePhase) {
    let state = app.state::<AppState>();
    state.registry.lock().unwrap().set_phase(id, phase).ok();
    app.emit("phase-changed", id.to_string()).ok();
}

fn set_error_emit(app: &AppHandle, id: Uuid, kind: InstanceErrorKind, msg: &str) {
    set_phase_emit(app, id, InstancePhase::Error(InstanceError {
        kind,
        message: msg.to_string(),
    }));
}

fn emit_lifecycle_events(app: &AppHandle, events: Vec<LifecycleEvent>) {
    for e in events {
        match e {
            LifecycleEvent::PhaseChanged(id, _phase) => {
                app.emit("phase-changed", id.to_string()).ok();
            }
            LifecycleEvent::Reconnected { id, pid } => {
                let notice = app.state::<AppState>().reconnect_notices.lock().unwrap()
                    .record(id, lifecycle::reconnect_notice(pid));
                app.emit("instance-reconnected", notice).ok();
            }
        }
    }
}

fn clone_config(app: &AppHandle, id: Uuid) -> Option<ServerInstanceConfig> {
    app.state::<AppState>().registry.lock().unwrap().get_config(id).cloned()
}

fn discovery_config(app: &AppHandle) -> DiscoveryConfig {
    app.state::<AppState>().registry.lock().unwrap().get_discovery_config().clone()
}

fn delete_managed_config_if_present(config: Option<&ServerInstanceConfig>) {
    let Some(cfg) = config else { return };
    let Some(tag) = &cfg.managed_model_tag else { return };
    driver_for(cfg, &DiscoveryConfig::default()).delete_managed_config(cfg, tag).ok();
}

fn check_health_once(app: &AppHandle, id: Uuid) -> bool {
    let Some(config) = clone_config(app, id) else { return false };
    driver_for(&config, &DiscoveryConfig::default()).health_check(&config) == HealthStatus::Healthy
}

fn process_is_alive(app: &AppHandle, id: Uuid) -> bool {
    let state = app.state::<AppState>();
    let mut processes = state.processes.lock().unwrap();
    match processes.get_mut(&id) {
        None => false,
        Some(child) => child.try_wait().map(|s| s.is_none()).unwrap_or(false),
    }
}

fn poll_tick(
    app: &AppHandle,
    id: Uuid,
    health: bool,
    process_alive: bool,
    elapsed: std::time::Duration,
    ctx: PollContext,
) -> (PollOutcome, Vec<LifecycleEvent>) {
    let state = app.state::<AppState>();
    let mut reg = state.registry.lock().unwrap();
    let Some(config) = reg.get_config(id).cloned() else { return (PollOutcome::Done, vec![]) };
    let discovery = reg.get_discovery_config().clone();
    let driver = driver_for(&config, &discovery);
    lifecycle::poll_once(&mut reg, id, &*driver, health, process_alive, elapsed, ctx)
}

async fn run_poll_loop(app: AppHandle, id: Uuid, ctx: PollContext) {
    let started = tokio::time::Instant::now();
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        let app_clone = app.clone();
        let (health, alive) = tauri::async_runtime::spawn_blocking(move || {
            (check_health_once(&app_clone, id), process_is_alive(&app_clone, id))
        }).await.unwrap_or((false, false));
        let elapsed = started.elapsed();
        let (outcome, events) = poll_tick(&app, id, health, alive, elapsed, ctx.clone());
        emit_lifecycle_events(&app, events);
        if outcome == PollOutcome::Done { return; }
    }
}

async fn run_health_poll(app: AppHandle, id: Uuid) {
    run_poll_loop(app, id, PollContext::Startup).await;
}

async fn run_restart_switch_poll(app: AppHandle, id: Uuid, old_key: Option<String>) {
    run_poll_loop(app, id, PollContext::ModelSwitch { old_key }).await;
}

async fn switch_model_restart(
    app: &AppHandle,
    id: Uuid,
    plan: &localbar_core::driver::LaunchPlan,
    old_key: Option<String>,
) -> Result<(), String> {
    let child = app.state::<AppState>().processes.lock().unwrap().remove(&id);
    let grace = clone_config(app, id)
        .map(|c| driver_for(&c, &DiscoveryConfig::default()).stop(&c).grace_period_secs)
        .unwrap_or(0.0);
    if let Some(child) = child {
        tauri::async_runtime::spawn_blocking(move || graceful_kill(child, grace))
            .await
            .map_err(|e| e.to_string())?;
    }
    match spawn_from_plan(plan) {
        Ok(child) => {
            app.state::<AppState>().processes.lock().unwrap().insert(id, child);
            tauri::async_runtime::spawn(run_restart_switch_poll(app.clone(), id, old_key));
            Ok(())
        }
        Err(e) => fail_switch(app, id, old_key, e),
    }
}

fn fail_switch(app: &AppHandle, id: Uuid, old_key: Option<String>, message: String) -> Result<(), String> {
    let events = {
        let state = app.state::<AppState>();
        let mut reg = state.registry.lock().unwrap();
        let Some(config) = reg.get_config(id).cloned() else { return Err(message) };
        let discovery = reg.get_discovery_config().clone();
        let driver = driver_for(&config, &discovery);
        lifecycle::finish_warm_load(&mut reg, id, &*driver, Err(message.clone()), old_key)
    };
    emit_lifecycle_events(app, events);
    Err(message)
}

fn adopt_running(app: &AppHandle, config: &ServerInstanceConfig) {
    let was_running = {
        let state = app.state::<AppState>();
        let reg = state.registry.lock().unwrap();
        let result = matches!(reg.get_phase(config.id), Some(InstancePhase::Running));
        result
    };
    set_phase_emit(app, config.id, InstancePhase::Running);
    let is_first_adoption = !was_running;
    if is_first_adoption {
        tauri::async_runtime::spawn(run_adopted_health_poll(app.clone(), config.id));
    }
}

fn adopted_phase_is_active(app: &AppHandle, id: Uuid) -> bool {
    let state = app.state::<AppState>();
    let reg = state.registry.lock().unwrap();
    lifecycle::adopted_poll_continues(reg.get_phase(id))
}

async fn run_adopted_health_poll(app: AppHandle, id: Uuid) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        if !adopted_phase_is_active(&app, id) { return; }
        let app_clone = app.clone();
        let healthy = tauri::async_runtime::spawn_blocking(move || check_health_once(&app_clone, id))
            .await
            .unwrap_or(false);
        let events = {
            let state = app.state::<AppState>();
            let mut reg = state.registry.lock().unwrap();
            lifecycle::poll_adopted(&mut reg, id, healthy)
        };
        emit_lifecycle_events(&app, events);
    }
}

fn launch_from_plan(app: &AppHandle, id: Uuid, plan: &localbar_core::driver::LaunchPlan) {
    let child = match spawn_from_plan(plan) {
        Ok(c) => c,
        Err(e) => { set_error_emit(app, id, InstanceErrorKind::LaunchFailed, &e); return; }
    };
    let state = app.state::<AppState>();
    let still_starting = matches!(state.registry.lock().unwrap().get_phase(id), Some(InstancePhase::Starting));
    if !still_starting {
        graceful_kill(child, 0.0);
        return;
    }
    state.processes.lock().unwrap().insert(id, child);
    tauri::async_runtime::spawn(run_health_poll(app.clone(), id));
}

fn finish_adoption_if_running(app: &AppHandle, id: Uuid, config: &ServerInstanceConfig, driver: &dyn ServerDriver) {
    let running = matches!(
        app.state::<AppState>().registry.lock().unwrap().get_phase(id),
        Some(InstancePhase::Running)
    );
    if running {
        if localbar_core::model_probe::reports_loaded_model(config.server_type) {
            detect_and_correct_adopted_model(app, id, config, driver);
        }
        tauri::async_runtime::spawn(run_adopted_health_poll(app.clone(), id));
    }
}

fn detect_and_correct_adopted_model(
    app: &AppHandle,
    id: Uuid,
    config: &ServerInstanceConfig,
    driver: &dyn ServerDriver,
) {
    let pid = app.state::<AppState>().registry.lock().unwrap().get_adopted_pid(id);
    let Some(reported) = pid
        .and_then(ps_args_for_pid)
        .and_then(|args| localbar_core::model_probe::served_model_from_args(&args))
    else {
        return;
    };
    let already_matches = config
        .selected_model_key
        .as_deref()
        .is_some_and(|k| localbar_core::model_probe::model_matches(k, &reported));
    if already_matches {
        return;
    }
    let candidates: Vec<String> = driver
        .list_models(config)
        .map(|models| models.into_iter().map(|m| m.key).collect())
        .unwrap_or_default();
    let Some(corrected) = localbar_core::model_probe::best_matching_key(&reported, &candidates) else { return };
    let state = app.state::<AppState>();
    let mut reg = state.registry.lock().unwrap();
    reg.update_config(id, |c| c.selected_model_key = Some(corrected)).ok();
    reg.save().ok();
}

fn find_listener(port: u16) -> Option<lifecycle::Listener> {
    let pid = find_listening_pid(port)?;
    let command_line = ps_args_for_pid(pid).unwrap_or_default();
    Some(lifecycle::Listener { pid, command_line })
}

fn launch_instance(app: AppHandle, id: Uuid) {
    let Some(config) = clone_config(&app, id) else { return };
    let discovery = discovery_config(&app);
    let driver = driver_for(&config, &discovery);

    let listener = find_listener(config.port);
    let (plan, events) = {
        let state = app.state::<AppState>();
        let mut reg = state.registry.lock().unwrap();
        lifecycle::start(&mut reg, id, &*driver, listener)
    };
    emit_lifecycle_events(&app, events);

    match plan {
        Some(plan) => launch_from_plan(&app, id, &plan),
        None => finish_adoption_if_running(&app, id, &config, &*driver),
    }
}

#[tauri::command]
fn list_instances(state: State<'_, AppState>) -> Vec<ServerInstanceConfig> {
    state.registry.lock().unwrap().all_configs().cloned().collect()
}

#[tauri::command]
fn list_instance_phases(state: State<'_, AppState>) -> HashMap<String, InstancePhaseDto> {
    let reg = state.registry.lock().unwrap();
    reg.all_configs()
        .filter_map(|c| reg.get_phase(c.id).map(|p| (c.id.to_string(), phase_to_dto(p))))
        .collect()
}

struct PidLookup {
    port: u16,
    external: bool,
    spawned: Option<u32>,
    adopted: Option<u32>,
    probe_listener: bool,
}

fn pid_lookup(state: &AppState, id: Uuid) -> Option<PidLookup> {
    let reg = state.registry.lock().unwrap();
    let config = reg.get_config(id)?;
    let external = config.server_type == ServerType::External;
    let port = config.port;
    let probe_listener = matches!(
        reg.get_phase(id),
        Some(InstancePhase::Running) | Some(InstancePhase::SwitchingModel)
    );
    let adopted = reg.get_adopted_pid(id);
    drop(reg);
    let spawned = state.processes.lock().unwrap().get(&id).map(Child::id);
    Some(PidLookup { port, external, spawned, adopted, probe_listener })
}

#[tauri::command]
async fn get_instance_pid(state: State<'_, AppState>, id: String) -> Result<Option<InstancePidDto>, String> {
    let uuid = parse_uuid(&id)?;
    let Some(ctx) = pid_lookup(&state, uuid) else { return Ok(None) };
    if ctx.external {
        return Ok(None);
    }
    let listener = if ctx.probe_listener {
        tauri::async_runtime::spawn_blocking(move || find_listening_pid(ctx.port))
            .await
            .unwrap_or(None)
    } else {
        None
    };
    Ok(displayed_pid(ctx.spawned, ctx.adopted, listener).map(instance_pid_to_dto))
}

#[tauri::command]
fn list_reconnect_notices(state: State<'_, AppState>) -> Vec<ReconnectNoticeDto> {
    state.reconnect_notices.lock().unwrap().latest.values().cloned().collect()
}

#[tauri::command]
fn get_start_warning(state: State<'_, AppState>, id: String) -> Option<String> {
    let uuid = parse_uuid(&id).ok()?;
    state.registry.lock().unwrap().start_warning(uuid)
}

#[tauri::command]
async fn check_memory_warning(state: State<'_, AppState>, id: String) -> Result<Option<String>, String> {
    let uuid = parse_uuid(&id)?;
    let (config, discovery) = {
        let reg = state.registry.lock().unwrap();
        (reg.get_config(uuid).cloned(), reg.get_discovery_config().clone())
    };
    let Some(config) = config else { return Ok(None) };
    let Some(model_key) = config.selected_model_key.clone() else { return Ok(None) };
    let models = tauri::async_runtime::spawn_blocking(move || {
        driver_for(&config, &discovery).list_models(&config)
    }).await.map_err(|e| e.to_string())??;
    let size_bytes = match models.iter().find(|m| m.key == model_key).and_then(|m| m.size_bytes) {
        Some(s) => s as u64,
        None => return Ok(None),
    };
    let total_ram = system_total_ram_bytes();
    let threshold = (total_ram as f64 * 0.7) as u64;
    Ok((size_bytes > threshold).then(|| ram_warning_text(size_bytes, total_ram)))
}

fn system_total_ram_bytes() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.total_memory()
}

fn ram_warning_text(model_bytes: u64, total_bytes: u64) -> String {
    let model_gb = model_bytes as f64 / 1_073_741_824.0;
    let total_gb = total_bytes as f64 / 1_073_741_824.0;
    format!(
        "Model is ~{model_gb:.1} GB. System RAM is {total_gb:.1} GB total (70% threshold = {:.1} GB). Starting may cause memory pressure.",
        total_gb * 0.7,
    )
}

#[tauri::command]
fn add_instance(
    state: State<'_, AppState>,
    name: String,
    server_type: String,
    host: Option<String>,
    port: u16,
    executable_path: String,
) -> Result<String, String> {
    let stype = parse_server_type(&server_type)?;
    let mut config = ServerInstanceConfig::new(name, stype, port, executable_path);
    if let Some(h) = host {
        config.host = h;
    }
    let id = config.id;
    let mut reg = state.registry.lock().unwrap();
    reg.add_instance(config);
    reg.save()?;
    Ok(id.to_string())
}

async fn probe_external_model_key(host: String, port: u16) -> Option<String> {
    let mut probe = ServerInstanceConfig::new("probe", ServerType::External, port, "");
    probe.host = host;
    tauri::async_runtime::spawn_blocking(move || {
        ExternalDriver.list_models(&probe).ok()
            .and_then(|ms| ms.into_iter().next().map(|m| m.key))
    }).await.unwrap_or(None)
}

async fn probe_external_health(host: String, port: u16) -> HealthStatus {
    let mut probe = ServerInstanceConfig::new("probe", ServerType::External, port, "");
    probe.host = host;
    tauri::async_runtime::spawn_blocking(move || ExternalDriver.health_check(&probe))
        .await
        .unwrap_or(HealthStatus::Unreachable)
}

fn activate_adopted_instance(app: &AppHandle, state: &AppState, original_id: Uuid, new_id: Uuid) {
    state.registry.lock().unwrap().set_phase(original_id, InstancePhase::Stopped).ok();
    app.emit("phase-changed", original_id.to_string()).ok();
    let config = state.registry.lock().unwrap().get_config(new_id).cloned();
    if let Some(cfg) = config { adopt_running(app, &cfg); }
    app.emit("instance-added", new_id.to_string()).ok();
}

#[tauri::command]
async fn adopt_as_external_instance(
    state: State<'_, AppState>,
    app: AppHandle,
    conflicting_id: String,
) -> Result<AdoptResultDto, String> {
    let uuid = parse_uuid(&conflicting_id)?;
    let (host, port) = {
        let reg = state.registry.lock().unwrap();
        reg.get_config(uuid)
            .ok_or_else(|| format!("instance {conflicting_id} not found"))
            .map(|c| (c.host.clone(), c.port))?
    };
    let detected_model = probe_external_model_key(host.clone(), port).await;
    let health = probe_external_health(host, port).await;
    let new_id = adopt_external_as_new_instance(
        &mut state.registry.lock().unwrap(),
        uuid,
        detected_model,
    )?;
    activate_adopted_instance(&app, &state, uuid, new_id);
    Ok(AdoptResultDto { id: new_id.to_string(), warning: lifecycle::adopt_health_warning(health, port) })
}

#[tauri::command]
fn remove_instance(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    let child = state.processes.lock().unwrap().remove(&uuid);
    let (grace, config_snap) = {
        let reg = state.registry.lock().unwrap();
        let grace = reg.get_config(uuid)
            .map(|c| driver_for(c, &DiscoveryConfig::default()).stop(c).grace_period_secs)
            .unwrap_or(0.0);
        (grace, reg.get_config(uuid).cloned())
    };
    if let Some(child) = child {
        graceful_kill(child, grace);
    }
    delete_managed_config_if_present(config_snap.as_ref());
    let mut reg = state.registry.lock().unwrap();
    reg.remove_instance(uuid)?;
    reg.save()?;
    app.emit("instance-removed", id).ok();
    Ok(())
}

#[tauri::command]
fn set_start_on_launch(state: State<'_, AppState>, id: String, value: bool) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    let mut reg = state.registry.lock().unwrap();
    reg.update_config(uuid, |c| c.start_on_launch = value)?;
    reg.save()
}

#[tauri::command]
fn rename_instance(state: State<'_, AppState>, id: String, name: String) -> Result<(), String> {
    let name = name.trim().to_owned();
    if name.is_empty() { return Err("name cannot be empty".into()); }
    let uuid = parse_uuid(&id)?;
    let mut reg = state.registry.lock().unwrap();
    reg.update_config(uuid, |c| c.name = name)?;
    reg.save()
}

#[tauri::command]
fn set_instance_port(state: State<'_, AppState>, id: String, port: u16) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    let mut reg = state.registry.lock().unwrap();
    reg.update_config(uuid, |c| c.port = port)?;
    reg.save()
}

#[tauri::command]
fn set_selected_model(
    state: State<'_, AppState>,
    id: String,
    model_key: Option<String>,
) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    let mut reg = state.registry.lock().unwrap();
    reg.update_config(uuid, |c| c.selected_model_key = model_key)?;
    reg.save()
}

#[tauri::command]
fn set_model_search_path_override(
    state: State<'_, AppState>,
    id: String,
    path: Option<String>,
) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    let mut reg = state.registry.lock().unwrap();
    reg.update_config(uuid, |c| c.model_search_path_override = path)?;
    reg.save()
}

#[tauri::command]
async fn start_instance(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    {
        let reg = state.registry.lock().unwrap();
        reg.get_config(uuid).ok_or("instance not found")?;
    }
    tauri::async_runtime::spawn_blocking(move || launch_instance(app, uuid));
    Ok(())
}

async fn do_stop(app: &AppHandle, uuid: Uuid, child: Option<Child>, grace: f64) -> Result<(), String> {
    if let Some(child) = child {
        tauri::async_runtime::spawn_blocking(move || graceful_kill(child, grace))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let config = clone_config(app, uuid);
    let Some(config) = config else { return Ok(()) };
    let adopted_pid = adopted_pid_if_killable(app, uuid, &config);
    match adopted_pid {
        Some(pid) => stop_adopted_by_pid(app, uuid, &config, pid, grace).await,
        None => stop_without_kill(config).await,
    }
}

fn adopted_pid_if_killable(app: &AppHandle, uuid: Uuid, config: &ServerInstanceConfig) -> Option<u32> {
    if config.server_type == ServerType::External {
        return None;
    }
    app.state::<AppState>().registry.lock().unwrap().get_adopted_pid(uuid)
}

async fn stop_without_kill(config: ServerInstanceConfig) -> Result<(), String> {
    let port = config.port;
    let still_open = tauri::async_runtime::spawn_blocking(move || port_is_open(&config.host, config.port))
        .await
        .unwrap_or(false);
    lifecycle::stop_outcome(StopKill::Nothing, port, still_open)
}

async fn stop_adopted_by_pid(
    app: &AppHandle,
    uuid: Uuid,
    config: &ServerInstanceConfig,
    pid: u32,
    grace: f64,
) -> Result<(), String> {
    let port = config.port;
    let currently_listening = tauri::async_runtime::spawn_blocking(move || find_listening_pid(port))
        .await
        .unwrap_or(None);
    if currently_listening != Some(pid) {
        app.state::<AppState>().registry.lock().unwrap().clear_adopted_pid(uuid);
        return Err(format!("adopted process {pid} is no longer the listener on port {port}"));
    }
    let result = tauri::async_runtime::spawn_blocking(move || kill_by_pid(pid, grace))
        .await
        .map_err(|e| e.to_string())?;
    if result.is_ok() {
        app.state::<AppState>().registry.lock().unwrap().clear_adopted_pid(uuid);
    }
    result
}

fn take_child(state: &AppState, id: Uuid) -> Option<Child> {
    state.processes.lock().unwrap().remove(&id)
}

#[tauri::command]
async fn stop_instance(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    state.registry.lock().unwrap().set_phase(uuid, InstancePhase::Stopping)?;
    app.emit("phase-changed", &id).ok();

    let child = take_child(&state, uuid);
    let grace = clone_config(&app, uuid)
        .map(|c| driver_for(&c, &DiscoveryConfig::default()).stop(&c).grace_period_secs)
        .unwrap_or(0.0);

    if let Err(e) = do_stop(&app, uuid, child, grace).await {
        set_error_emit(&app, uuid, InstanceErrorKind::StopFailed, &e);
        return Err(e);
    }

    {
        let mut reg = state.registry.lock().unwrap();
        reg.set_phase(uuid, InstancePhase::Stopped)?;
        reg.update_config(uuid, |c| c.was_running_when_quit = false).ok();
        reg.save().ok();
    }
    app.emit("phase-changed", id).ok();
    Ok(())
}

async fn switch_model_warm_load(
    app: &AppHandle,
    id: Uuid,
    model: ModelRef,
    old_key: Option<String>,
) -> Result<(), String> {
    let Some(config) = clone_config(app, id) else { return Ok(()) };
    let discovery = discovery_config(app);
    let driver = driver_for(&config, &discovery);
    let params = config.instance_params.clone();
    let switch_result = tauri::async_runtime::spawn_blocking(move || {
        driver.switch_model(&model, &params, &config)
    }).await.map_err(|e| e.to_string())?;

    let events = {
        let state = app.state::<AppState>();
        let mut reg = state.registry.lock().unwrap();
        let Some(cfg) = reg.get_config(id).cloned() else { return switch_result };
        let discovery = reg.get_discovery_config().clone();
        let driver = driver_for(&cfg, &discovery);
        lifecycle::finish_warm_load(&mut reg, id, &*driver, switch_result.clone(), old_key)
    };
    emit_lifecycle_events(app, events);
    switch_result
}

#[tauri::command]
async fn switch_model_cmd(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
    model_key: String,
) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    let (plan, events) = {
        let mut reg = state.registry.lock().unwrap();
        let config = reg.get_config(uuid).cloned().ok_or("instance not found")?;
        let discovery = reg.get_discovery_config().clone();
        let driver = driver_for(&config, &discovery);
        lifecycle::switch_model(&mut reg, uuid, &model_key, &*driver)
    };
    emit_lifecycle_events(&app, events);
    app.emit("config-changed", uuid.to_string()).ok();

    match plan {
        SwitchPlan::KeyOnly => Ok(()),
        SwitchPlan::Failed { message } => Err(message),
        SwitchPlan::Restart { plan, old_key } => switch_model_restart(&app, uuid, &plan, old_key).await,
        SwitchPlan::WarmLoad { model, old_key } => switch_model_warm_load(&app, uuid, model, old_key).await,
    }
}

#[tauri::command]
fn update_instance_params(
    state: State<'_, AppState>,
    id: String,
    params: ParamValues,
) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    let mut reg = state.registry.lock().unwrap();
    let config = reg.get_config(uuid).cloned().ok_or("instance not found")?;
    let driver = driver_for(&config, &DiscoveryConfig::default());
    set_instance_params(&mut reg, uuid, params, &*driver)
}

#[tauri::command]
fn set_active_profile_cmd(
    state: State<'_, AppState>,
    id: String,
    profile_id: Option<String>,
) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    let pid = profile_id.as_deref().map(parse_uuid).transpose()?;
    let mut reg = state.registry.lock().unwrap();
    let config = reg.get_config(uuid).cloned().ok_or("instance not found")?;
    let driver = driver_for(&config, &DiscoveryConfig::default());
    set_active_profile(&mut reg, uuid, pid, &*driver)
}

fn list_non_ollama_models(stype: ServerType, discovery: &DiscoveryConfig) -> Result<Vec<ModelRef>, String> {
    let probe = ServerInstanceConfig::new("probe", stype, 0, "");
    driver_for(&probe, discovery).list_models(&probe)
}

const OLLAMA_UNREACHABLE: &str = "OLLAMA_UNREACHABLE";
const HOME_ENV_VAR: &str = "HOME";
const OLLAMA_APP_SUPPORT_RELATIVE_PATH: &str = "Library/Application Support/Ollama/db.sqlite";
const SQLITE3_EXECUTABLE: &str = "/usr/bin/sqlite3";

fn ollama_app_models_setting(home: &str) -> Option<String> {
    let db_path = std::path::Path::new(home).join(OLLAMA_APP_SUPPORT_RELATIVE_PATH);
    let uri = format!("file:{}?mode=ro", db_path.display());
    let output = std::process::Command::new(SQLITE3_EXECUTABLE)
        .arg("-readonly")
        .arg(uri)
        .arg("select models from settings where id = 1")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    ollama::parse_app_models_setting(&output.stdout)
}

fn list_ollama_models_with_fallback(discovery: &DiscoveryConfig) -> Result<Vec<ModelRef>, String> {
    let probe = ServerInstanceConfig::new("probe", ServerType::Ollama, 11434, "");
    if let Ok(models) = driver_for(&probe, discovery).list_models(&probe) {
        return Ok(models);
    }
    let env_value = std::env::var(ollama::OLLAMA_MODELS_ENV_VAR).ok();
    let home = std::env::var(HOME_ENV_VAR).ok();
    let app_setting = home.as_deref().and_then(ollama_app_models_setting);
    let models_dir = ollama::resolve_models_dir(env_value.as_deref(), app_setting.as_deref(), home.as_deref())
        .ok_or_else(|| OLLAMA_UNREACHABLE.to_string())?;
    ollama::list_models_from_manifests(&models_dir).map_err(|_| OLLAMA_UNREACHABLE.to_string())
}

#[tauri::command]
async fn list_models_for_type(state: State<'_, AppState>, server_type: String) -> Result<Vec<ModelRef>, String> {
    let stype = parse_server_type(&server_type)?;
    let discovery = state.registry.lock().unwrap().get_discovery_config().clone();
    tauri::async_runtime::spawn_blocking(move || match stype {
        ServerType::Ollama => list_ollama_models_with_fallback(&discovery),
        _ => list_non_ollama_models(stype, &discovery),
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
struct DiscoveredModel {
    server_type: String,
    key: String,
    display_name: String,
    publisher: Option<String>,
    architecture: Option<String>,
    parameter_count: Option<String>,
    quantization: Option<String>,
    size_bytes: Option<i64>,
    modified_secs: Option<i64>,
}

fn model_to_discovered(stype: &str, m: ModelRef, meta: Option<ModelMetadata>) -> DiscoveredModel {
    DiscoveredModel {
        server_type: stype.to_string(),
        key: m.key,
        display_name: m.display_name,
        publisher: m.publisher,
        architecture: m.architecture,
        parameter_count: meta.as_ref().and_then(|md| md.parameter_count.clone()),
        quantization: meta.as_ref().and_then(|md| md.quantization.clone()),
        size_bytes: m.size_bytes,
        modified_secs: m.modified_secs,
    }
}

fn discover_mlx_models(discovery: &DiscoveryConfig) -> Vec<DiscoveredModel> {
    let probe = ServerInstanceConfig::new("probe", ServerType::MlxLm, 0, "");
    let driver = driver_for(&probe, discovery);
    driver.list_models(&probe).unwrap_or_default().into_iter().map(|m| {
        let meta = driver.fetch_model_metadata(&m.key, &probe);
        model_to_discovered("mlx-lm", m, meta)
    }).collect()
}

fn discover_ollama_models(_discovery: &DiscoveryConfig) -> Vec<DiscoveredModel> {
    let probe = ServerInstanceConfig::new("probe", ServerType::Ollama, 11434, "");
    driver_for(&probe, &DiscoveryConfig::default())
        .list_models(&probe)
        .unwrap_or_default()
        .into_iter()
        .map(|m| model_to_discovered("ollama", m, None))
        .collect()
}

fn discover_all_models(discovery: &DiscoveryConfig) -> Vec<DiscoveredModel> {
    let mut result = discover_mlx_models(discovery);
    result.extend(discover_ollama_models(discovery));
    result
}

#[tauri::command]
async fn list_all_discovered_models(state: State<'_, AppState>) -> Result<Vec<DiscoveredModel>, String> {
    let discovery = state.registry.lock().unwrap().get_discovery_config().clone();
    tauri::async_runtime::spawn_blocking(move || discover_all_models(&discovery))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_discovery_config(state: State<'_, AppState>) -> DiscoveryConfig {
    state.registry.lock().unwrap().get_discovery_config().clone()
}

#[tauri::command]
fn set_discovery_config(state: State<'_, AppState>, config: DiscoveryConfig) -> Result<(), String> {
    state.registry.lock().unwrap().set_discovery_config(config)
}

#[tauri::command]
async fn list_models_cmd(state: State<'_, AppState>, id: String) -> Result<Vec<ModelRef>, String> {
    let uuid = parse_uuid(&id)?;
    let (config, discovery) = {
        let reg = state.registry.lock().unwrap();
        (reg.get_config(uuid).cloned(), reg.get_discovery_config().clone())
    };
    let Some(config) = config else { return Err(format!("instance {id} not found")) };
    tauri::async_runtime::spawn_blocking(move || {
        driver_for(&config, &discovery).list_models(&config)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn fetch_model_metadata_cmd(
    state: State<'_, AppState>,
    id: String,
    model_key: String,
) -> Result<Option<ModelMetadata>, String> {
    let uuid = parse_uuid(&id)?;
    let (config, discovery) = {
        let reg = state.registry.lock().unwrap();
        let config = reg.get_config(uuid)
            .ok_or_else(|| format!("instance {id} not found"))?.clone();
        (config, reg.get_discovery_config().clone())
    };
    tauri::async_runtime::spawn_blocking(move || {
        Ok(driver_for(&config, &discovery).fetch_model_metadata(&model_key, &config))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_resolved_params(state: State<'_, AppState>, id: String) -> Result<ParamValues, String> {
    let uuid = parse_uuid(&id)?;
    let reg = state.registry.lock().unwrap();
    let config = reg.get_config(uuid).ok_or_else(|| format!("instance {id} not found"))?;
    let profile = reg.get_active_profile_params(uuid);
    let mem_key = ModelMemoryKey {
        server_type: config.server_type,
        model_key: config.selected_model_key.clone().unwrap_or_default(),
    };
    let memory = reg.get_model_memory(&mem_key);
    let schema = driver_for(config, &DiscoveryConfig::default()).param_schema();
    Ok(ParamValues::resolve(profile, memory, &schema).0)
}

#[derive(serde::Serialize)]
struct ParamSchemaEntry {
    key: &'static str,
    label: &'static str,
    kind: &'static str,
    server_flag: &'static str,
    default_value: Option<localbar_core::types::ParamValue>,
}

fn param_schema_entry(d: &localbar_core::types::ParamDescriptor) -> Option<ParamSchemaEntry> {
    use localbar_core::types::CanonicalParam::*;
    let (key, label, kind) = match d.param {
        Temperature     => ("temperature",     "Temperature",       "double"),
        TopP            => ("topP",            "Top-P",             "double"),
        TopK            => ("topK",            "Top-K",             "int"),
        MinP            => ("minP",            "Min-P",             "double"),
        MaxTokens       => ("maxTokens",       "Max tokens",        "int"),
        RepeatPenalty   => ("repeatPenalty",   "Repeat penalty",    "double"),
        PresencePenalty => ("presencePenalty", "Presence penalty",  "double"),
        Seed            => ("seed",            "Seed",              "int"),
        ContextLength   => ("contextLength",   if d.server_flag_name == "--max-kv-size" { "KV cache size" } else { "Context length" },   "int"),
        SystemPrompt    => return None,
    };
    Some(ParamSchemaEntry { key, label, kind, server_flag: d.server_flag_name, default_value: d.default_value.clone() })
}

#[tauri::command]
fn get_param_schema(server_type: String) -> Result<Vec<ParamSchemaEntry>, String> {
    let stype = parse_server_type(&server_type)?;
    let probe = ServerInstanceConfig::new("probe", stype, 0, "");
    Ok(driver_for(&probe, &DiscoveryConfig::default()).param_schema().iter().filter_map(param_schema_entry).collect())
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    if let Some(popover) = app.get_webview_window("popover") {
        let _ = popover.hide();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        set_dock_icon();
    }
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn get_app_settings(state: State<'_, AppState>) -> AppSettings {
    state.registry.lock().unwrap().get_app_settings().clone()
}

#[tauri::command]
fn set_app_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    state.registry.lock().unwrap().set_app_settings(settings)
}

#[tauri::command]
fn open_url(url: String) {
    let _ = std::process::Command::new("open").arg(&url).spawn();
}

fn mark_running_instances_for_reconnect(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut reg = state.registry.lock().unwrap();
    let active_ids: Vec<_> = reg.all_configs()
        .filter(|c| reg.get_phase(c.id).map(|p| p.is_active()).unwrap_or(false))
        .map(|c| c.id)
        .collect();
    for id in active_ids {
        reg.update_config(id, |c| c.was_running_when_quit = true).ok();
    }
    reg.save().ok();
}

struct QuitStop {
    id: Uuid,
    child: Child,
    grace: f64,
}

fn quit_candidates(state: &AppState) -> Vec<QuitCandidate> {
    let spawned: Vec<Uuid> = state.processes.lock().unwrap().keys().copied().collect();
    let reg = state.registry.lock().unwrap();
    reg.all_configs()
        .map(|c| QuitCandidate { id: c.id, has_child: spawned.contains(&c.id), server_type: c.server_type })
        .collect()
}

fn grace_for(app: &AppHandle, id: Uuid) -> f64 {
    clone_config(app, id)
        .map(|c| driver_for(&c, &DiscoveryConfig::default()).stop(&c).grace_period_secs)
        .unwrap_or(0.0)
}

fn take_quit_stops(app: &AppHandle) -> Vec<QuitStop> {
    let state = app.state::<AppState>();
    let settings = state.registry.lock().unwrap().get_app_settings().clone();
    let ids = ids_to_stop_on_quit(&quit_candidates(&state), &settings);
    ids.into_iter()
        .filter_map(|id| take_child(&state, id).map(|child| QuitStop { id, child, grace: grace_for(app, id) }))
        .collect()
}

fn stop_all_for_quit(app: &AppHandle, stops: Vec<QuitStop>) {
    let graces: Vec<f64> = stops.iter().map(|s| s.grace).collect();
    let budget = std::time::Duration::from_secs_f64(quit_shutdown_budget_secs(&graces));
    let deadline = std::time::Instant::now() + budget;
    let (tx, rx) = std::sync::mpsc::channel();
    for stop in stops {
        set_phase_emit(app, stop.id, InstancePhase::Stopping);
        let tx = tx.clone();
        std::thread::spawn(move || {
            graceful_kill(stop.child, stop.grace);
            let _ = tx.send(());
        });
    }
    drop(tx);
    while rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())).is_ok() {}
}

fn begin_quit(app: &AppHandle, signal: ExitSignal) -> QuitSequence {
    let already_quitting = app.state::<AppState>().quitting.swap(true, Ordering::SeqCst);
    let sequence = quit_sequence_for(signal, already_quitting);
    if sequence != QuitSequence::AlreadyRan {
        mark_running_instances_for_reconnect(app);
    }
    sequence
}

fn handle_exit(app: &AppHandle) {
    if begin_quit(app, ExitSignal::Terminating) == QuitSequence::Inline {
        stop_all_for_quit(app, take_quit_stops(app));
    }
}

fn handle_exit_requested(app: &AppHandle, api: &tauri::ExitRequestApi) {
    if begin_quit(app, ExitSignal::Requested) != QuitSequence::InBackground {
        return;
    }
    let stops = take_quit_stops(app);
    if stops.is_empty() {
        return;
    }
    api.prevent_exit();
    let handle = app.clone();
    std::thread::spawn(move || {
        stop_all_for_quit(&handle, stops);
        handle.exit(0);
    });
}

fn on_startup(app: &tauri::App) {
    let state = app.state::<AppState>();

    if let Some(e) = &state.load_error {
        eprintln!("[localbar] state.json failed to load: {e}");
        app.emit("startup-error", e.clone()).ok();
    }

    let (configs, settings) = {
        let mut reg = state.registry.lock().unwrap();
        let configs = reg.all_configs().cloned().collect::<Vec<_>>();
        let settings = reg.get_app_settings().clone();
        let forget = ids_to_forget_reconnect_on_launch(&configs, &settings);
        if !forget.is_empty() {
            for id in forget {
                reg.update_config(id, |c| c.was_running_when_quit = false).ok();
            }
            reg.save().ok();
        }
        (configs, settings)
    };
    for id in ids_to_start_on_launch(&configs, &settings) {
        let handle = app.handle().clone();
        tauri::async_runtime::spawn(async move {
            tauri::async_runtime::spawn_blocking(move || launch_instance(handle, id)).await.ok();
        });
    }
}

#[derive(Clone, Copy, PartialEq)]
enum TrayIconState { Idle, Running, Transitioning, Error }

fn icon_bytes(state: TrayIconState) -> &'static [u8] {
    match state {
        TrayIconState::Idle          => include_bytes!("../icons/tray-idle.png"),
        TrayIconState::Running       => include_bytes!("../icons/tray-running.png"),
        TrayIconState::Transitioning => include_bytes!("../icons/tray-transitioning.png"),
        TrayIconState::Error         => include_bytes!("../icons/tray-error.png"),
    }
}

fn compute_tray_state(app: &AppHandle) -> TrayIconState {
    let state = app.state::<AppState>();
    let reg = state.registry.lock().unwrap();
    let (mut running, mut transitioning, mut error) = (false, false, false);
    for config in reg.all_configs() {
        match reg.get_phase(config.id) {
            Some(InstancePhase::Running)       => running       = true,
            Some(InstancePhase::Error(_))      => error         = true,
            Some(InstancePhase::Starting
               | InstancePhase::Stopping
               | InstancePhase::SwitchingModel) => transitioning = true,
            _ => {}
        }
    }
    if error             { TrayIconState::Error }
    else if transitioning { TrayIconState::Transitioning }
    else if running       { TrayIconState::Running }
    else                  { TrayIconState::Idle }
}

fn sync_tray_icon(app: &AppHandle) {
    let state = compute_tray_state(app);
    if let Ok(icon) = tauri::image::Image::from_bytes(icon_bytes(state)) {
        let tray = app.state::<tauri::tray::TrayIcon>();
        tray.set_icon(Some(icon)).ok();
        tray.set_icon_as_template(true).ok();
    }
}

fn build_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let icon = tauri::image::Image::from_bytes(icon_bytes(TrayIconState::Idle))?;
    let tray = TrayIconBuilder::new()
        .icon(icon)
        .icon_as_template(true)
        .tooltip(concat!("LocalBar v", env!("CARGO_PKG_VERSION")))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button_state, .. } = event {
                if button_state != tauri::tray::MouseButtonState::Up {
                    return;
                }
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("popover") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        })
        .build(app)?;
    app.manage(tray);
    Ok(())
}

fn wire_popover_autohide(app: &tauri::App) {
    if let Some(popover) = app.get_webview_window("popover") {
        let win = popover.clone();
        popover.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                let _ = win.hide();
            }
        });
    }
}

#[cfg(target_os = "macos")]
fn install_popover_key_monitor(app: &tauri::App) {
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
    use block2::RcBlock;

    let Some(popover) = app.get_webview_window("popover") else { return };

    const KEYCODE_W:   u16 = 13;
    const KEYCODE_ESC: u16 = 53;
    const CMD: NSEventModifierFlags = NSEventModifierFlags::Command;

    let block = RcBlock::new(move |event: std::ptr::NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: the pointer comes directly from AppKit and is valid for this call.
        let ev = unsafe { event.as_ref() };
        let code = ev.keyCode();
        let mods = ev.modifierFlags().intersection(CMD);
        let dismiss = code == KEYCODE_ESC || (mods == CMD && code == KEYCODE_W);
        if dismiss && popover.is_visible().unwrap_or(false) {
            let _ = popover.hide();
            return std::ptr::null_mut();
        }
        event.as_ptr()
    });

    // SAFETY: addLocalMonitorForEventsMatchingMask_handler is called on the main
    // thread with a valid mask and block, as required by AppKit. The returned
    // monitor is forgotten rather than dropped so it keeps intercepting events
    // for the lifetime of the app, matching the installed handler's lifetime.
    unsafe {
        let monitor_lives_for_app = NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::KeyDown,
            &block,
        );
        std::mem::forget(monitor_lives_for_app);
    }
}

#[cfg(not(target_os = "macos"))]
fn install_popover_key_monitor(_app: &tauri::App) {}

#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;
    // SAFETY: called from setup_handler which runs on the main thread.
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    // SAFETY: NSData::with_bytes copies the embedded icon bytes into a new
    // NSData, and mtm proves this runs on the main thread as required by
    // NSApplication::sharedApplication.
    unsafe {
        let data = NSData::with_bytes(include_bytes!("../icons/icon.png"));
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&*image));
        }
    }
}

fn wire_settings_close(app: &tauri::App) {
    if let Some(settings_win) = app.get_webview_window("settings") {
        let win = settings_win.clone();
        let _handle = app.handle().clone();
        settings_win.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = win.hide();
                #[cfg(target_os = "macos")]
                let _ = _handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
        });
    }
}

fn load_persisted_state(state: &mut AppState) {
    let mut reg = state.registry.lock().unwrap();
    if let Err(e) = reg.load() {
        state.load_error = Some(e);
    } else {
        reg.load_model_memory().ok();
        reg.load_profiles().ok();
        reg.load_discovery_config().ok();
        reg.load_app_settings().ok();
    }
}

fn setup_handler(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let mut state = AppState::new(data_dir);
    load_persisted_state(&mut state);
    app.manage(state);
    #[cfg(target_os = "macos")]
    {
        app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        set_dock_icon();
    }
    if let Err(e) = build_tray(app) {
        eprintln!("[localbar] fatal: could not create tray icon: {e}");
        std::process::exit(1);
    }
    {
        let handle = app.handle().clone();
        app.listen("phase-changed", move |_| sync_tray_icon(&handle));
    }
    wire_popover_autohide(app);
    install_popover_key_monitor(app);
    wire_settings_close(app);
    on_startup(app);
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(setup_handler)
        .invoke_handler(tauri::generate_handler![
            list_instances, list_instance_phases, add_instance, remove_instance,
            get_start_warning, list_reconnect_notices, get_instance_pid, check_memory_warning, start_instance, stop_instance,
            set_start_on_launch, rename_instance, set_instance_port, set_selected_model,
            switch_model_cmd, update_instance_params, set_active_profile_cmd,
            list_models_cmd, list_models_for_type, fetch_model_metadata_cmd, get_resolved_params, get_param_schema,
            list_all_discovered_models,
            get_discovery_config, set_discovery_config, get_app_settings, set_app_settings, set_model_search_path_override,
            adopt_as_external_instance,
            open_settings, quit_app, open_url,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match event {
                tauri::RunEvent::ExitRequested { api, .. } => handle_exit_requested(app, &api),
                tauri::RunEvent::Exit => handle_exit(app),
                _ => {}
            }
        });
}
