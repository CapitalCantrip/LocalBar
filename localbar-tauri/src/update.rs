use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use localbar_core::update_check::{check_for_update, is_check_due, UpdateCheckOutcome, LATEST_RELEASE_URL};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::AppState;

const STARTUP_DELAY: Duration = Duration::from_secs(15);
const DUE_POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const UPDATE_STATUS_EVENT: &str = "update-status-changed";

#[derive(Default)]
pub struct UpdateStatus(Mutex<Option<UpdateCheckOutcome>>);

pub fn app_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn fetch_latest_release(user_agent: &str) -> Result<String, String> {
    ureq::AgentBuilder::new()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .get(LATEST_RELEASE_URL)
        .set("User-Agent", user_agent)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
}

fn run_check_blocking(app: &AppHandle) -> UpdateCheckOutcome {
    let version = app_version(app);
    let user_agent = format!("LocalBar/{version}");
    let outcome = check_for_update(&version, || fetch_latest_release(&user_agent));
    if outcome == UpdateCheckOutcome::Failed {
        eprintln!("[localbar] update check failed");
    } else {
        let state = app.state::<AppState>();
        let recorded = state.registry.lock().unwrap().record_update_check(now_secs());
        if let Err(e) = recorded {
            eprintln!("[localbar] could not record update check time: {e}");
        }
    }
    outcome
}

async fn run_check(app: AppHandle) -> UpdateCheckOutcome {
    let worker = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || run_check_blocking(&worker))
        .await
        .unwrap_or(UpdateCheckOutcome::Failed);
    if outcome != UpdateCheckOutcome::Failed {
        *app.state::<UpdateStatus>().0.lock().unwrap() = Some(outcome.clone());
        app.emit(UPDATE_STATUS_EVENT, outcome.clone()).ok();
    }
    outcome
}

fn scheduled_check_due(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let reg = state.registry.lock().unwrap();
    reg.get_app_settings().check_for_updates && is_check_due(reg.last_update_check(), now_secs())
}

async fn run_scheduled_checks(app: AppHandle) {
    tokio::time::sleep(STARTUP_DELAY).await;
    loop {
        if scheduled_check_due(&app) {
            run_check(app.clone()).await;
        }
        tokio::time::sleep(DUE_POLL_INTERVAL).await;
    }
}

pub fn start(app: &tauri::App) {
    app.manage(UpdateStatus::default());
    tauri::async_runtime::spawn(run_scheduled_checks(app.handle().clone()));
}

#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app_version(&app)
}

#[tauri::command]
pub fn get_update_status(state: State<'_, AppState>, status: State<'_, UpdateStatus>) -> Option<UpdateCheckOutcome> {
    if !state.registry.lock().unwrap().get_app_settings().check_for_updates {
        return None;
    }
    status.0.lock().unwrap().clone()
}

#[tauri::command]
pub async fn check_for_updates_now(app: AppHandle) -> UpdateCheckOutcome {
    run_check(app).await
}
