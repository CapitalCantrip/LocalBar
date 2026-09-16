use tauri::{
    tray::{TrayIconBuilder, TrayIconEvent},
    Manager,
};

#[tauri::command]
fn open_settings(app: tauri::AppHandle) {
    // Stub: T3 replaces this with real settings window logic.
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Menu bar app: hide from Dock on macOS.
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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![open_settings, quit_app])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
