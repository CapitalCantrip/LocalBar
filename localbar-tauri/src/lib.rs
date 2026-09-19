#![deny(clippy::cognitive_complexity)]
#![deny(clippy::too_many_lines)]

use std::collections::HashMap;
use std::process::Child;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Listener, Manager, State, WindowEvent};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use uuid::Uuid;

use localbar_core::driver::{HealthStatus, ServerDriver};
use localbar_core::drivers::external::ExternalDriver;
use localbar_core::drivers::mlx_lm::MLXLMDriver;
use localbar_core::drivers::ollama::OllamaDriver;
use localbar_core::persistence::FilePersistence;
use localbar_core::registry::InstanceRegistry;
use localbar_core::types::{
    InstanceError, InstanceErrorKind, InstancePhase, ServerInstanceConfig, ServerType,
};

// ─── DTO ─────────────────────────────────────────────────────────────────────

/// Wire-format for InstancePhase. Separate from core type — core has no Serialize impl.
#[derive(serde::Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InstancePhaseDto {
    Stopped,
    Starting,
    Running,
    Stopping,
    SwitchingModel,
    Error { message: String },
}

// ─── AppState ────────────────────────────────────────────────────────────────

pub struct AppState {
    pub registry: Mutex<InstanceRegistry>,
    pub processes: Mutex<HashMap<Uuid, Child>>,
    /// Stored here so on_startup can emit it once windows are ready.
    pub load_error: Option<String>,
}

impl AppState {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        let persistence = FilePersistence::new(data_dir.join("state.json"));
        let registry = InstanceRegistry::new(Box::new(persistence));
        Self {
            registry: Mutex::new(registry),
            processes: Mutex::new(HashMap::new()),
            load_error: None,
        }
    }
}

// ─── Driver factory ──────────────────────────────────────────────────────────

fn driver_for_type(server_type: ServerType) -> Box<dyn ServerDriver> {
    match server_type {
        ServerType::Ollama => Box::new(OllamaDriver),
        ServerType::MlxLm => Box::new(MLXLMDriver::default()),
        ServerType::External => Box::new(ExternalDriver),
    }
}

// ─── Small helpers ────────────────────────────────────────────────────────────

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

fn phase_to_dto(phase: &InstancePhase) -> InstancePhaseDto {
    match phase {
        InstancePhase::Stopped => InstancePhaseDto::Stopped,
        InstancePhase::Starting => InstancePhaseDto::Starting,
        InstancePhase::Running => InstancePhaseDto::Running,
        InstancePhase::Stopping => InstancePhaseDto::Stopping,
        InstancePhase::SwitchingModel => InstancePhaseDto::SwitchingModel,
        InstancePhase::Error(e) => InstancePhaseDto::Error { message: e.message.clone() },
    }
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
    cmd.spawn().map_err(|e| format!("spawn failed: {e}"))
}

fn port_is_open(host: &str, port: u16) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let Ok(mut addrs) = (host, port).to_socket_addrs() else { return false };
    let Some(addr) = addrs.next() else { return false };
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

/// Send SIGTERM on Unix, then wait up to `grace_secs`, then SIGKILL.
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

// ─── Phase mutation helpers ───────────────────────────────────────────────────

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

fn clone_config(app: &AppHandle, id: Uuid) -> Option<ServerInstanceConfig> {
    app.state::<AppState>().registry.lock().unwrap().get_config(id).cloned()
}

// ─── Health-poll loop ─────────────────────────────────────────────────────────

fn check_health_once(app: &AppHandle, id: Uuid) -> bool {
    let Some(config) = clone_config(app, id) else { return false };
    driver_for_type(config.server_type).health_check(&config) == HealthStatus::Healthy
}

fn process_is_alive(app: &AppHandle, id: Uuid) -> bool {
    let state = app.state::<AppState>();
    let mut processes = state.processes.lock().unwrap();
    match processes.get_mut(&id) {
        None => false,
        Some(child) => child.try_wait().map(|s| s.is_none()).unwrap_or(false),
    }
}

fn current_phase_is_starting(app: &AppHandle, id: Uuid) -> bool {
    let state = app.state::<AppState>();
    let reg = state.registry.lock().unwrap();
    matches!(reg.get_phase(id), Some(InstancePhase::Starting))
}

async fn run_health_poll(app: AppHandle, id: Uuid) {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(30);
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if !current_phase_is_starting(&app, id) {
            return;
        }
        if !process_is_alive(&app, id) {
            set_error_emit(&app, id, InstanceErrorKind::LaunchFailed, "process exited unexpectedly");
            return;
        }
        let app_clone = app.clone();
        let healthy = tauri::async_runtime::spawn_blocking(move || check_health_once(&app_clone, id))
            .await
            .unwrap_or(false);
        if healthy {
            set_phase_emit(&app, id, InstancePhase::Running);
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            set_error_emit(&app, id, InstanceErrorKind::HealthCheckFailed, "startup timed out after 30 s");
            return;
        }
    }
}

// ─── Core launch logic (shared by IPC command + startup) ─────────────────────

fn adopt_running(app: &AppHandle, config: &ServerInstanceConfig) {
    set_phase_emit(app, config.id, InstancePhase::Running);
    // Adopted instances have no process to watch, so spin up a health poller so the
    // phase self-corrects if the external server goes down.
    tauri::async_runtime::spawn(run_adopted_health_poll(app.clone(), config.id));
}

/// Polls health every 10 s for an externally-adopted (unmanaged) instance.
/// Transitions to Error if the server becomes unreachable, and re-adopts (Running) if it
/// comes back. Exits when the phase leaves the Running/Error cycle (e.g. user clicks Stop).
async fn run_adopted_health_poll(app: AppHandle, id: Uuid) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        // Stop polling once the instance is no longer in a state we manage here.
        {
            let state = app.state::<AppState>();
            let reg = state.registry.lock().unwrap();
            match reg.get_phase(id) {
                Some(InstancePhase::Running) | Some(InstancePhase::Error(_)) => {}
                _ => return,
            }
        }

        let app_clone = app.clone();
        let healthy = tauri::async_runtime::spawn_blocking(move || check_health_once(&app_clone, id))
            .await
            .unwrap_or(false);

        {
            let state = app.state::<AppState>();
            let reg = state.registry.lock().unwrap();
            let current_phase = reg.get_phase(id).cloned();
            drop(reg);
            match (healthy, current_phase) {
                (false, Some(InstancePhase::Running)) => {
                    set_error_emit(&app, id, InstanceErrorKind::HealthCheckFailed,
                        "server is no longer reachable");
                }
                (true, Some(InstancePhase::Error(_))) => {
                    set_phase_emit(&app, id, InstancePhase::Running);
                }
                _ => {}
            }
        }
    }
}

/// Returns true and sets PortConflict error if the port is occupied by a foreign process.
/// Accepts the already-computed health status to avoid a redundant network round-trip.
fn detect_port_conflict(app: &AppHandle, config: &ServerInstanceConfig, health: &HealthStatus) -> bool {
    if port_is_open(&config.host, config.port) && *health != HealthStatus::Healthy {
        set_error_emit(app, config.id, InstanceErrorKind::PortConflict { port: config.port },
            &format!("port {} is in use by another process", config.port));
        return true;
    }
    false
}

/// Returns the spawned Child, or None if the instance was adopted/conflicted/errored.
fn try_spawn_instance(app: &AppHandle, id: Uuid, config: &ServerInstanceConfig) -> Option<Child> {
    let driver = driver_for_type(config.server_type);
    let first_health = driver.health_check(config);

    if first_health == HealthStatus::Healthy {
        adopt_running(app, config);
        return None;
    }
    if detect_port_conflict(app, config, &first_health) {
        return None;
    }
    // External drivers never own a process — if the server isn't reachable, that's an error.
    if !driver.manages_lifecycle() {
        set_error_emit(app, id, InstanceErrorKind::HealthCheckFailed,
            &format!("no server detected at {}:{}", config.host, config.port));
        return None;
    }
    let plan = match driver.launch(config, None, &config.instance_params) {
        Ok(p) => p,
        Err(e) => { set_error_emit(app, id, InstanceErrorKind::LaunchFailed, &e); return None; }
    };
    match spawn_from_plan(&plan) {
        Ok(c) => Some(c),
        Err(e) => { set_error_emit(app, id, InstanceErrorKind::LaunchFailed, &e); None }
    }
}

fn launch_instance(app: AppHandle, id: Uuid) {
    let Some(config) = clone_config(&app, id) else { return };
    let Some(child) = try_spawn_instance(&app, id, &config) else { return };

    // Guard against a Stop that arrived during the blocking I/O above.
    {
        let state = app.state::<AppState>();
        let reg = state.registry.lock().unwrap();
        if !matches!(reg.get_phase(id), Some(InstancePhase::Starting)) {
            drop(reg);
            graceful_kill(child, 0.0);
            return;
        }
    }
    app.state::<AppState>().processes.lock().unwrap().insert(id, child);
    // ParamValues::resolve is not called here — resolution is scaffolded for T8+.
    tauri::async_runtime::spawn(run_health_poll(app, id));
}

// ─── IPC: read queries ────────────────────────────────────────────────────────

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

#[tauri::command]
fn get_start_warning(state: State<'_, AppState>, id: String) -> Option<String> {
    let uuid = parse_uuid(&id).ok()?;
    state.registry.lock().unwrap().start_warning(uuid)
}

#[tauri::command]
async fn check_memory_warning(state: State<'_, AppState>, id: String) -> Result<Option<String>, String> {
    let uuid = parse_uuid(&id)?;
    let config = state.registry.lock().unwrap().get_config(uuid).cloned();
    let Some(config) = config else { return Ok(None) };
    let Some(model_key) = config.selected_model_key.clone() else { return Ok(None) };
    let models = tauri::async_runtime::spawn_blocking(move || {
        driver_for_type(config.server_type).list_models(&config)
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

// ─── IPC: mutations ──────────────────────────────────────────────────────────

#[tauri::command]
fn add_instance(
    state: State<'_, AppState>,
    name: String,
    server_type: String,
    port: u16,
    executable_path: String,
) -> Result<String, String> {
    let stype = parse_server_type(&server_type)?;
    let config = ServerInstanceConfig::new(name, stype, port, executable_path);
    let id = config.id;
    let mut reg = state.registry.lock().unwrap();
    reg.add_instance(config);
    reg.save()?;
    Ok(id.to_string())
}

#[tauri::command]
fn remove_instance(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    // Extract child before kill/wait so the mutex is not held during blocking ops.
    let child = state.processes.lock().unwrap().remove(&uuid);
    let grace = state.registry.lock().unwrap()
        .get_config(uuid)
        .map(|c| driver_for_type(c.server_type).stop(c).grace_period_secs)
        .unwrap_or(0.0);
    if let Some(child) = child {
        graceful_kill(child, grace);
    }
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

// ─── IPC: lifecycle ───────────────────────────────────────────────────────────

#[tauri::command]
async fn start_instance(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    {
        let mut reg = state.registry.lock().unwrap();
        let phase = reg.get_phase(uuid);
        if matches!(phase, Some(InstancePhase::Starting | InstancePhase::Running | InstancePhase::Stopping)) {
            return Ok(());
        }
        reg.get_config(uuid).ok_or("instance not found")?;
        reg.set_phase(uuid, InstancePhase::Starting).ok();
    }
    app.emit("phase-changed", uuid.to_string()).ok();
    // launch_instance does blocking network I/O (health checks); run it off the main thread.
    tauri::async_runtime::spawn_blocking(move || launch_instance(app, uuid));
    Ok(())
}

/// Returns Err if the instance was adopted and the server is still up after the stop attempt.
async fn do_stop(app: &AppHandle, uuid: Uuid, child: Option<Child>, grace: f64) -> Result<(), String> {
    if let Some(child) = child {
        tauri::async_runtime::spawn_blocking(move || graceful_kill(child, grace))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    // Adopted instance — verify it actually stopped.
    let config = clone_config(app, uuid);
    let still_up = tauri::async_runtime::spawn_blocking(move || {
        config.map(|c| driver_for_type(c.server_type).health_check(&c) == HealthStatus::Healthy)
              .unwrap_or(false)
    }).await.unwrap_or(false);
    if still_up {
        Err("cannot stop: server was not launched by LocalBar and is still running".to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
async fn stop_instance(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    state.registry.lock().unwrap().set_phase(uuid, InstancePhase::Stopping)?;
    app.emit("phase-changed", &id).ok();

    // Extract child before blocking ops so the mutex is not held during kill/wait.
    let child = state.processes.lock().unwrap().remove(&uuid);
    let grace = clone_config(&app, uuid)
        .map(|c| driver_for_type(c.server_type).stop(&c).grace_period_secs)
        .unwrap_or(0.0);

    if let Err(e) = do_stop(&app, uuid, child, grace).await {
        // Show an error badge so the user knows why Stop didn't work (e.g. adopted server
        // still running), rather than silently reverting to Running with no indication.
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

#[tauri::command]
fn open_settings(app: AppHandle) {
    if let Some(popover) = app.get_webview_window("popover") {
        let _ = popover.hide();
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

// ─── App lifecycle helpers ────────────────────────────────────────────────────

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

fn on_startup(app: &tauri::App) {
    let state = app.state::<AppState>();

    if let Some(e) = &state.load_error {
        eprintln!("[localbar] state.json failed to load: {e}");
        app.emit("startup-error", e.clone()).ok();
    }

    // The C3 start_warning gate is bypassed here on purpose for now: every
    // auto-start instance is launched concurrently, so two large models can
    // load at once and exhaust memory. That risk is known; a startup chooser
    // that lets the user pick which instances to restore is tracked in #15.
    let configs: Vec<_> = state.registry.lock().unwrap().all_configs().cloned().collect();
    for config in configs {
        if config.was_running_when_quit || config.start_on_launch {
            let handle = app.handle().clone();
            let id = config.id;
            tauri::async_runtime::spawn(async move {
                tauri::async_runtime::spawn_blocking(move || launch_instance(handle, id)).await.ok();
            });
        }
    }
}

// ─── Tray icon state ─────────────────────────────────────────────────────────

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
        app.state::<tauri::tray::TrayIcon>().set_icon(Some(icon)).ok();
    }
}

// ─── Tray / window wiring ────────────────────────────────────────────────────

fn build_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let icon = tauri::image::Image::from_bytes(icon_bytes(TrayIconState::Idle))?;
    let tray = TrayIconBuilder::new()
        .icon(icon)
        .icon_as_template(true)
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

/// Install a local NSEvent monitor so that ESC and Cmd+W dismiss the popover
/// regardless of WKWebView's key-event filtering.
/// Cmd+H is intentionally NOT intercepted: LSUIElement apps have no Dock presence,
/// so "hide app" is meaningless and the event should pass through unconsumed.
#[cfg(target_os = "macos")]
fn install_popover_key_monitor(app: &tauri::App) {
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
    use block2::RcBlock;

    let Some(popover) = app.get_webview_window("popover") else { return };

    // key codes (hardware-layout independent)
    const KEY_W:   u16 = 13;
    const KEY_ESC: u16 = 53;
    const CMD: NSEventModifierFlags = NSEventModifierFlags::Command;

    let block = RcBlock::new(move |event: std::ptr::NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: the pointer comes directly from AppKit and is valid for this call.
        let ev = unsafe { event.as_ref() };
        let code = ev.keyCode();
        let mods = ev.modifierFlags().intersection(CMD);
        let dismiss = code == KEY_ESC || (mods == CMD && code == KEY_W);
        if dismiss && popover.is_visible().unwrap_or(false) {
            let _ = popover.hide();
            return std::ptr::null_mut(); // consume the event
        }
        event.as_ptr()
    });

    unsafe {
        // Leak the monitor intentionally: it must live as long as the app.
        let _monitor = NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::KeyDown,
            &*block,
        );
        std::mem::forget(_monitor);
    }
}

#[cfg(not(target_os = "macos"))]
fn install_popover_key_monitor(_app: &tauri::App) {}

fn wire_settings_close(app: &tauri::App) {
    if let Some(settings_win) = app.get_webview_window("settings") {
        let win = settings_win.clone();
        settings_win.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = win.hide();
            }
        });
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn setup_handler(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let mut state = AppState::new(data_dir);
    if let Err(e) = state.registry.lock().unwrap().load() {
        state.load_error = Some(e);
    }
    app.manage(state);
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    // Propagating Err here would cause Tauri to panic!() inside applicationDidFinishLaunching,
    // which cannot unwind through ObjC and aborts. Use process::exit for fatal tray failures.
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
        .setup(setup_handler)
        .invoke_handler(tauri::generate_handler![
            list_instances,
            list_instance_phases,
            add_instance,
            remove_instance,
            get_start_warning,
            check_memory_warning,
            start_instance,
            stop_instance,
            set_start_on_launch,
            set_selected_model,
            open_settings,
            quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                mark_running_instances_for_reconnect(app);
            }
        });
}
