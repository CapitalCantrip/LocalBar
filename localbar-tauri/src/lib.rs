use std::collections::HashMap;
use tauri::{
    tray::{TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

use localbar_core::types::ServerInstanceConfig;

/// Wire-format for InstancePhase — mirrors localbar_core::types::InstancePhase.
/// Kept separate because InstancePhase intentionally has no Serialize impl in core.
#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InstancePhaseDto {
    Stopped,
    Starting,
    Running,
    Stopping,
    SwitchingModel,
    Error { message: String },
}

// ─── IPC stubs ────────────────────────────────────────────────────────────────
// All commands return empty/placeholder data. Subsequent tickets replace the
// bodies with real implementations without changing the signatures.

#[tauri::command]
fn list_instances() -> Vec<ServerInstanceConfig> {
    Vec::new()
}

#[tauri::command]
fn list_instance_phases() -> HashMap<String, InstancePhaseDto> {
    HashMap::new()
}

#[tauri::command]
fn start_instance(_id: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
fn stop_instance(_id: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
fn open_settings(app: tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

// ─── App entry point ─────────────────────────────────────────────────────────

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let icon = app
                .default_window_icon()
                .expect("no default icon configured")
                .clone();

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

            // Hide the settings window on close instead of destroying it so
            // open_settings() can re-show it without recreating the webview.
            if let Some(settings_win) = app.get_webview_window("settings") {
                let win = settings_win.clone();
                settings_win.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_instances,
            list_instance_phases,
            start_instance,
            stop_instance,
            open_settings,
            quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
