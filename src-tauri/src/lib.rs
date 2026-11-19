pub mod commands;
pub mod runtime;

use std::collections::HashMap;
use std::fs;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex, RwLock};

use commands::{
    create_default_persisted_data, dispatch_daemon_ipc, get_daemon_snapshot, DaemonHub,
    PersistedDaemonData, SharedDaemonState, VirtualCursorDto,
};
use spankvm_core::FocusRouter;
use spankvm_net::LocalEd25519Identity;
use spankvm_pal::{CaptureControlState, CrossPlatformInjector};
use spankvm_proto::{ReliableControlFrame, DEFAULT_QUIC_PORT};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
#[cfg(target_os = "macos")]
use tauri::RunEvent;

fn show_and_focus_main_window(app_handle: &AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn load_or_init_hub(app: &tauri::App) -> Arc<DaemonHub> {
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

    let identity = LocalEd25519Identity::from_secret_hex(&data.identity_secret_hex)
        .unwrap_or_else(LocalEd25519Identity::generate);

    let local_id = data.local_node_id.clone();
    let daemon_active = data.tuning.daemon_active;
    let wayland_strategy = data.tuning.linux_wayland_strategy;

    let injector = CrossPlatformInjector::initialize(wayland_strategy).ok();

    let shared_state = SharedDaemonState {
        data,
        discovered_peers: Vec::new(),
        cursor: VirtualCursorDto {
            active_node_id: local_id.clone(),
            x: 0.5,
            y: 0.5,
            transition_count: 0,
            last_transition_edge: None,
        },
        router: FocusRouter::new(local_id),
        state_file_path,
        last_synced_clipboard_hash: String::new(),
        last_os_clipboard_seq: spankvm_pal::clipboard::os_clipboard_sequence_token(),
    };

    Arc::new(DaemonHub {
        state: RwLock::new(shared_state),
        sessions: RwLock::new(HashMap::new()),
        identity,
        capture_ctrl: Arc::new(CaptureControlState::new(daemon_active)),
        injector: Mutex::new(injector),
        quic_endpoint: RwLock::new(None),
        quic_port: AtomicU32::new(DEFAULT_QUIC_PORT as u32),
        datagram_seq: AtomicU32::new(1),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            spankvm_pal::macos::prevent_app_nap();
            let hub = load_or_init_hub(app);
            hub.persist();

            // Apply persisted macOS Dock visibility preference (default: hide_dock_icon = false)
            let initial_hide_dock = hub
                .state
                .read()
                .map(|g| g.data.tuning.hide_dock_icon)
                .unwrap_or(false);

            #[cfg(target_os = "macos")]
            if initial_hide_dock {
                let _ = app.set_dock_visibility(false);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = initial_hide_dock;

            app.manage(Arc::clone(&hub));

            let handle = app.handle();
            runtime::start_background_services(Arc::clone(&hub), handle.clone());

            let show_item =
                MenuItem::with_id(handle, "show", "打开 SpanKVM 主界面", true, None::<&str>)?;
            let toggle_daemon_item = MenuItem::with_id(
                handle,
                "toggle_daemon",
                "暂停 / 恢复跨屏键鼠共享",
                true,
                None::<&str>,
            )?;
            let reclaim_item = MenuItem::with_id(
                handle,
                "reclaim",
                "强制切回本机焦点 (Ctrl+Alt+Shift+Esc)",
                true,
                None::<&str>,
            )?;
            let quit_item = MenuItem::with_id(handle, "quit", "退出 SpanKVM", true, None::<&str>)?;
            let tray_menu = Menu::with_items(
                handle,
                &[&show_item, &toggle_daemon_item, &reclaim_item, &quit_item],
            )?;

            // Left-click tray icon shows/focuses SpanKVM; ONLY right-click opens context menu
            let mut tray_builder = TrayIconBuilder::new()
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_and_focus_main_window(tray.app_handle());
                    }
                })
                .on_menu_event(|app_handle, event| match event.id().as_ref() {
                    "show" => {
                        show_and_focus_main_window(app_handle);
                    }
                    "toggle_daemon" => {
                        if let Some(hub) = app_handle.try_state::<Arc<DaemonHub>>() {
                            if let Ok(mut guard) = hub.state.write() {
                                guard.data.tuning.daemon_active = !guard.data.tuning.daemon_active;
                                if !guard.data.tuning.daemon_active {
                                    let local_id = guard.data.local_node_id.clone();
                                    guard.cursor.active_node_id = local_id.clone();
                                    guard.cursor.x = 0.5;
                                    guard.cursor.y = 0.5;
                                    guard.router.active_node_id = local_id;
                                }
                            }
                            hub.persist();
                            hub.notify_frontend(app_handle);
                        }
                    }
                    "reclaim" => {
                        if let Some(hub) = app_handle.try_state::<Arc<DaemonHub>>() {
                            hub.capture_ctrl.set_suppressing(false);
                            if let Ok(mut guard) = hub.state.write() {
                                let local_id = guard.data.local_node_id.clone();
                                let outcome = guard.router.force_return_to_primary(&local_id);
                                guard.cursor.active_node_id = outcome.active_node_id;
                                guard.cursor.x = 0.5;
                                guard.cursor.y = 0.5;
                            }
                            let sessions: Vec<_> = hub
                                .sessions
                                .read()
                                .map(|s| s.values().cloned().collect())
                                .unwrap_or_default();
                            for sess in sessions {
                                tauri::async_runtime::spawn(async move {
                                    let _ = sess
                                        .send_reliable_frame(
                                            &ReliableControlFrame::ReleaseAllHeldKeys,
                                            true,
                                        )
                                        .await;
                                });
                            }
                            hub.notify_frontend(app_handle);
                        }
                    }
                    "quit" => {
                        if let Some(hub) = app_handle.try_state::<Arc<DaemonHub>>() {
                            hub.capture_ctrl.set_suppressing(false);
                        }
                        app_handle.exit(0);
                    }
                    _ => {}
                });

            let tray_icon = tauri::include_image!("icons/tray-icon@2x.png");
            let tray_builder = tray_builder
                .icon(tray_icon)
                .icon_as_template(cfg!(target_os = "macos"));
            let _ = tray_builder.build(handle);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dispatch_daemon_ipc,
            get_daemon_snapshot,
        ])
        .on_window_event(|window, event| {
            // Keep background KVM daemon alive in tray when main window closes
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
            show_and_focus_main_window(app_handle);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app_handle, event);
    });
}
