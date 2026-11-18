pub mod commands;

use std::sync::RwLock;
use commands::{
    cmd_check_capacity, cmd_emergency_return_primary, cmd_move_node_cell, cmd_set_primary_node,
    cmd_update_engine_tuning, get_daemon_snapshot, SharedDaemonState,
};
use spankvm_core::FocusRouter;
use spankvm_proto::{
    ClipboardPolicy, DeviceNode, EngineTuning, GridPos, LinuxWaylandStrategy, OsKind, Resolution,
};

fn default_daemon_state() -> SharedDaemonState {
    let primary_id = "node-mac-studio".to_string();
    let nodes = vec![DeviceNode {
        id: primary_id.clone(),
        name: "Local Primary Host".into(),
        hostname: "localhost.local".into(),
        os: if cfg!(target_os = "macos") {
            OsKind::MacOs
        } else if cfg!(target_os = "windows") {
            OsKind::Windows
        } else {
            OsKind::LinuxWayland
        },
        ip: "192.168.50.10".into(),
        resolution: Resolution {
            width: 2560,
            height: 1440,
        },
        refresh_rate_hz: 144,
        is_primary: true,
        grid_pos: GridPos { col: 2, row: 2 },
        auto_pair: true,
        online: true,
        latency_ms: 0.1,
        fingerprint: "ED25519:LOCAL:HOST".into(),
        paired_at: "本机宿主".into(),
    }];

    SharedDaemonState {
        nodes,
        router: FocusRouter::new(primary_id),
        key_rules: Vec::new(),
        tuning: EngineTuning {
            daemon_active: true,
            hid_polling_hz: 1000,
            edge_delay_ms: 0,
            release_hotkey: "Ctrl+Alt+Shift+Escape".into(),
            linux_wayland_strategy: LinuxWaylandStrategy::PortalAndUinputFallback,
        },
        clipboard_policy: ClipboardPolicy {
            sync_plain_text: true,
            sync_images: true,
            enable_drag_drop_file: true,
            max_file_size_mb: 500,
            save_dir: "~/Downloads/SpanKVM".into(),
        },
        local_ephemeral_pin: "482-915".into(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RwLock::new(default_daemon_state()))
        .invoke_handler(tauri::generate_handler![
            get_daemon_snapshot,
            cmd_set_primary_node,
            cmd_move_node_cell,
            cmd_emergency_return_primary,
            cmd_update_engine_tuning,
            cmd_check_capacity,
        ])
        .on_window_event(|window, event| {
            // Keep background Rust KVM routing daemon alive in tray when configuration window closes
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running SpanKVM Tauri v2 application");
}
