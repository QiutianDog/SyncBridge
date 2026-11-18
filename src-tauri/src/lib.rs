pub mod commands;

use std::fs;
use std::sync::RwLock;

use commands::{
    create_default_persisted_data, dispatch_daemon_ipc, get_daemon_snapshot, get_workspace_file,
    get_workspace_files, PersistedDaemonData, SharedDaemonState,
};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, RunEvent, WindowEvent};

fn load_or_init_daemon_state(app: &tauri::App) -> SharedDaemonState {
    let state_file_path = app
        .path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("spankvm-daemon-state.json"));

    let data = state_file_path
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str::<PersistedDaemonData>(&raw).ok())
        .unwrap_or_else(create_default_persisted_data);

    SharedDaemonState {
        data,
        state_file_path,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let initial_state = load_or_init_daemon_state(app);
            app.manage(RwLock::new(initial_state));

            let handle = app.handle();
            let show_item = MenuItem::with_id(
                handle,
                "show",
                "打开 SpanKVM 控制台",
                true,
                None::<&str>,
            )?;
            let quit_item = MenuItem::with_id(
                handle,
                "quit",
                "退出 SpanKVM",
                true,
                None::<&str>,
            )?;
            let tray_menu = Menu::with_items(handle, &[&show_item, &quit_item])?;

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&tray_menu)
                .on_menu_event(|app_handle, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app_handle.exit(0);
                    }
                    _ => {}
                });

            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            let _ = tray_builder.build(handle);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dispatch_daemon_ipc,
            get_daemon_snapshot,
            get_workspace_files,
            get_workspace_file,
        ])
        .on_window_event(|window, event| {
            // Keep background Rust KVM routing daemon alive in tray when configuration window closes
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building SpanKVM Tauri v2 application");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } = event
        {
            if let Some(window) = app_handle.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app_handle, event);
    });
}
