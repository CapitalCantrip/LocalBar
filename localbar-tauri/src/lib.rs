#![deny(clippy::cognitive_complexity)]
#![deny(clippy::too_many_lines)]

use std::collections::HashMap;
use std::process::Child;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
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
}

impl AppState {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        let persistence = FilePersistence::new(data_dir.join("state.json"));
        let mut registry = InstanceRegistry::new(Box::new(persistence));
        registry.load().ok();
        Self {
            registry: Mutex::new(registry),
            processes: Mutex::new(HashMap::new()),
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
        if check_health_once(&app, id) {
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
}

fn detect_port_conflict(app: &AppHandle, config: &ServerInstanceConfig) -> bool {
    // Port occupied but health check fails → something else is there.
    if port_is_open(&config.host, config.port) {
        let driver = driver_for_type(config.server_type);
        if driver.health_check(config) != HealthStatus::Healthy {
            set_error_emit(app, config.id, InstanceErrorKind::PortConflict { port: config.port },
                &format!("port {} is in use by another process", config.port));
            return true;
        }
    }
    false
}

fn launch_instance(app: AppHandle, id: Uuid) {
    let Some(config) = clone_config(&app, id) else { return };
    let driver = driver_for_type(config.server_type);

    // Adopt-on-start: process already healthy on the configured port.
    if driver.health_check(&config) == HealthStatus::Healthy {
        adopt_running(&app, &config);
        return;
    }

    if detect_port_conflict(&app, &config) {
        return;
    }

    let plan = match driver.launch(&config, None, &config.instance_params) {
        Ok(p) => p,
        Err(e) => { set_error_emit(&app, id, InstanceErrorKind::LaunchFailed, &e); return; }
    };
    let child = match spawn_from_plan(&plan) {
        Ok(c) => c,
        Err(e) => { set_error_emit(&app, id, InstanceErrorKind::LaunchFailed, &e); return; }
    };
    app.state::<AppState>().processes.lock().unwrap().insert(id, child);
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
    if let Some(mut child) = state.processes.lock().unwrap().remove(&uuid) {
        let _ = child.kill();
        let _ = child.wait();
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
fn start_instance(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
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
    launch_instance(app, uuid);
    Ok(())
}

#[tauri::command]
fn stop_instance(state: State<'_, AppState>, app: AppHandle, id: String) -> Result<(), String> {
    let uuid = parse_uuid(&id)?;
    state.registry.lock().unwrap().set_phase(uuid, InstancePhase::Stopping)?;
    app.emit("phase-changed", &id).ok();

    if let Some(mut child) = state.processes.lock().unwrap().remove(&uuid) {
        let _ = child.kill();
        let _ = child.wait();
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
    let configs: Vec<_> = state.registry.lock().unwrap().all_configs().cloned().collect();
    for config in configs {
        if config.was_running_when_quit || config.start_on_launch {
            let handle = app.handle().clone();
            let id = config.id;
            tauri::async_runtime::spawn(async move { launch_instance(handle, id) });
        }
    }
}

// ─── Tray / window wiring ────────────────────────────────────────────────────

fn build_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let icon = app.default_window_icon().expect("no default icon configured").clone();
    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { .. } = event {
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
    Ok(())
}

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
    app.manage(AppState::new(data_dir));
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    build_tray(app)?;
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
