use std::collections::HashMap;
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use quinn::Endpoint;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use spankvm_core::{
    compute_neighbors, find_next_adjacent_slot, move_node_to_cell, set_primary_node, FocusRouter,
};
use spankvm_net::{
    broadcast_lan_beacon, client_pair_spake2_over_quic, compute_chunk_blake3_hex,
    detect_local_hostname, detect_local_lan_ipv4, LocalEd25519Identity, QuicPeerSession,
};
use spankvm_pal::{
    clipboard::write_os_clipboard_text, detect_active_driver_stack, detect_local_os_kind,
    probe_local_display, prompt_input_permission, CaptureControlState, CrossPlatformInjector,
};
use spankvm_proto::{
    decode_base64, encode_base64, ClipboardPolicy, DeviceNode, EngineTuning, GridPos,
    KeyMappingRule, LanDiscoveryBeacon, LinuxWaylandStrategy, OsKind, ReliableControlFrame,
    Resolution, DEFAULT_QUIC_PORT, DISCOVERY_UDP_PORT, FILE_CHUNK_SIZE, MAX_DEVICES,
};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPeerDto {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub os: OsKind,
    pub ip: String,
    pub quic_port: u16,
    pub resolution: Resolution,
    pub refresh_rate_hz: u16,
    pub latency_ms: f32,
    pub fingerprint: String,
    #[serde(default)]
    pub last_seen_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualCursorDto {
    pub active_node_id: String,
    pub x: f32,
    pub y: f32,
    pub transition_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_transition_edge: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferRecordDto {
    pub id: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub title: String,
    pub size_label: String,
    pub from_node_name: String,
    pub to_node_name: String,
    pub timestamp: String,
    pub encrypted: bool,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSystemStatusDto {
    pub local_node_id: String,
    pub local_os: OsKind,
    pub local_ip: String,
    pub quic_port: u16,
    pub discovery_port: u16,
    pub fingerprint: String,
    pub input_permission_granted: bool,
    pub connected_peer_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonSnapshotDto {
    pub nodes: Vec<DeviceNode>,
    pub discovered_peers: Vec<DiscoveredPeerDto>,
    pub cursor: VirtualCursorDto,
    pub tuning: EngineTuning,
    pub clipboard_policy: ClipboardPolicy,
    pub key_rules: Vec<KeyMappingRule>,
    pub transfers: Vec<TransferRecordDto>,
    pub local_ephemeral_pin: String,
    pub active_driver_name: String,
    pub system_status: LocalSystemStatusDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcEnvelope {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<DaemonSnapshotDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swapped_with: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_node: Option<DeviceNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedDaemonData {
    pub local_node_id: String,
    pub identity_secret_hex: String,
    pub nodes: Vec<DeviceNode>,
    pub tuning: EngineTuning,
    pub clipboard_policy: ClipboardPolicy,
    pub key_rules: Vec<KeyMappingRule>,
    pub transfers: Vec<TransferRecordDto>,
    pub pin_seed: u64,
}

pub struct SharedDaemonState {
    pub data: PersistedDaemonData,
    pub discovered_peers: Vec<DiscoveredPeerDto>,
    pub cursor: VirtualCursorDto,
    pub router: FocusRouter,
    pub state_file_path: Option<PathBuf>,
    pub last_synced_clipboard_hash: String,
}

pub struct DaemonHub {
    pub state: RwLock<SharedDaemonState>,
    pub sessions: RwLock<HashMap<String, QuicPeerSession>>,
    pub identity: LocalEd25519Identity,
    pub capture_ctrl: Arc<CaptureControlState>,
    pub injector: Mutex<Option<CrossPlatformInjector>>,
    pub quic_endpoint: RwLock<Option<Endpoint>>,
    pub quic_port: AtomicU32,
    pub datagram_seq: AtomicU32,
}

impl DaemonHub {
    pub fn build_snapshot(&self) -> DaemonSnapshotDto {
        let guard = self.state.read().expect("state read lock");
        let sessions = self.sessions.read().expect("sessions read lock");
        let driver = detect_active_driver_stack(guard.data.tuning.linux_wayland_strategy);
        let local_ip = detect_local_lan_ipv4().to_string();
        let qport = self.quic_port.load(Ordering::Relaxed) as u16;

        let mut nodes = guard.data.nodes.clone();
        for n in &mut nodes {
            if n.id == guard.data.local_node_id {
                n.online = true;
                n.ip = local_ip.clone();
                n.latency_ms = 0.1;
                if !guard.data.tuning.local_device_name.trim().is_empty() {
                    n.name = guard.data.tuning.local_device_name.trim().to_string();
                }
            } else if let Some(sess) = sessions.get(&n.id) {
                n.online = true;
                n.latency_ms = sess.rtt_ms();
                n.ip = sess.peer_ip.to_string();
            } else {
                n.online = false;
            }
        }

        let visible_peers: Vec<DiscoveredPeerDto> = guard
            .discovered_peers
            .iter()
            .filter(|p| p.id != guard.data.local_node_id && !nodes.iter().any(|n| n.id == p.id))
            .cloned()
            .collect();

        DaemonSnapshotDto {
            nodes,
            discovered_peers: visible_peers,
            cursor: guard.cursor.clone(),
            tuning: guard.data.tuning.clone(),
            clipboard_policy: guard.data.clipboard_policy.clone(),
            key_rules: guard.data.key_rules.clone(),
            transfers: guard.data.transfers.clone(),
            local_ephemeral_pin: format_pin_from_seed(guard.data.pin_seed),
            active_driver_name: driver.os_backend_name.to_string(),
            system_status: LocalSystemStatusDto {
                local_node_id: guard.data.local_node_id.clone(),
                local_os: detect_local_os_kind(),
                local_ip,
                quic_port: qport,
                discovery_port: DISCOVERY_UDP_PORT,
                fingerprint: self.identity.fingerprint.clone(),
                input_permission_granted: driver.permission_granted,
                connected_peer_count: sessions.len(),
            },
        }
    }

    pub fn persist(&self) {
        let guard = self.state.read().expect("state read lock");
        if let Some(ref path) = guard.state_file_path {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(serialized) = serde_json::to_string_pretty(&guard.data) {
                let _ = fs::write(path, serialized);
            }
        }
    }

    pub fn sync_capture_edges(&self) {
        let guard = self.state.read().expect("state read lock");
        let sessions = self.sessions.read().expect("sessions read lock");
        self.capture_ctrl
            .set_daemon_enabled(guard.data.tuning.daemon_active);

        let local_id = &guard.data.local_node_id;
        let neighbors = compute_neighbors(&guard.data.nodes, local_id);
        let is_online_peer = |opt: Option<&DeviceNode>| -> bool {
            opt.map(|n| n.id == *local_id || sessions.contains_key(&n.id))
                .unwrap_or(false)
        };
        let armed = [
            is_online_peer(neighbors.up),
            is_online_peer(neighbors.down),
            is_online_peer(neighbors.left),
            is_online_peer(neighbors.right),
        ];
        self.capture_ctrl.set_armed_edges(armed);
    }

    pub fn local_node_descriptor(&self) -> DeviceNode {
        let guard = self.state.read().expect("state read lock");
        let local_ip = detect_local_lan_ipv4().to_string();
        if let Some(existing) = guard
            .data
            .nodes
            .iter()
            .find(|n| n.id == guard.data.local_node_id)
        {
            let mut copy = existing.clone();
            copy.ip = local_ip;
            copy.online = true;
            if !guard.data.tuning.local_device_name.trim().is_empty() {
                copy.name = guard.data.tuning.local_device_name.trim().to_string();
            }
            return copy;
        }
        let (resolution, refresh_rate_hz) = probe_local_display();
        let hostname = detect_local_hostname();
        DeviceNode {
            id: guard.data.local_node_id.clone(),
            name: hostname.clone(),
            hostname,
            os: detect_local_os_kind(),
            ip: local_ip,
            resolution,
            refresh_rate_hz,
            is_primary: true,
            grid_pos: GridPos { col: 2, row: 2 },
            auto_pair: true,
            online: true,
            latency_ms: 0.1,
            fingerprint: self.identity.fingerprint.clone(),
            paired_at: "本机设备".into(),
        }
    }

    pub fn emit_beacon_now(&self) {
        let local_ip = detect_local_lan_ipv4();
        let node = self.local_node_descriptor();
        let qport = self.quic_port.load(Ordering::Relaxed) as u16;
        let beacon = LanDiscoveryBeacon {
            magic: LanDiscoveryBeacon::MAGIC.to_string(),
            node_id: node.id,
            name: node.name,
            hostname: node.hostname,
            os: node.os,
            quic_port: qport,
            resolution: node.resolution,
            refresh_rate_hz: node.refresh_rate_hz,
            fingerprint: self.identity.fingerprint.clone(),
            sent_epoch_ms: now_epoch_millis() as u64,
        };
        broadcast_lan_beacon(&beacon, local_ip);
    }

    pub fn notify_frontend(&self, app_handle: &AppHandle) {
        self.sync_capture_edges();
        let snap = self.build_snapshot();
        let _ = app_handle.emit("spankvm://snapshot", snap);
    }
}

pub fn format_pin_from_seed(seed: u64) -> String {
    let n = (seed % 900_000) + 100_000;
    format!("{:03}-{:03}", n / 1000, n % 1000)
}

pub fn now_epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(1_763_400_000_000)
}

pub fn now_hms_string() -> String {
    let total_secs = (now_epoch_millis() / 1000) % 86_400;
    let hours = (total_secs / 3600) % 24;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    format!("{hours:02}:{mins:02}:{secs:02}")
}

pub fn expand_save_dir(raw_dir: &str) -> PathBuf {
    let trimmed = raw_dir.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(if trimmed.is_empty() {
        "SpanKVM-Received"
    } else {
        trimmed
    })
}

pub fn default_key_rules() -> Vec<KeyMappingRule> {
    vec![
        KeyMappingRule {
            id: "rule-mac-cmd-to-win-ctrl".into(),
            label: "Mac ⌘ Command → Windows Ctrl（复制 / 粘贴 / 全选）".into(),
            target_os: "windows".into(),
            source_key: "Command (⌘)".into(),
            target_key: "Ctrl".into(),
            enabled: true,
        },
        KeyMappingRule {
            id: "rule-mac-cmd-to-linux-ctrl".into(),
            label: "Mac ⌘ Command → Linux Ctrl（桌面快捷键对齐）".into(),
            target_os: "linux".into(),
            source_key: "Command (⌘)".into(),
            target_key: "Ctrl".into(),
            enabled: true,
        },
        KeyMappingRule {
            id: "rule-mac-opt-to-alt".into(),
            label: "Mac ⌥ Option → Windows Alt".into(),
            target_os: "windows".into(),
            source_key: "Option (⌥)".into(),
            target_key: "Alt".into(),
            enabled: true,
        },
        KeyMappingRule {
            id: "rule-caps-to-esc".into(),
            label: "CapsLock → Escape（全局可选）".into(),
            target_os: "all".into(),
            source_key: "CapsLock".into(),
            target_key: "Escape".into(),
            enabled: false,
        },
    ]
}

pub fn create_default_persisted_data() -> PersistedDaemonData {
    let identity = LocalEd25519Identity::generate();
    let hostname = detect_local_hostname();
    let os = detect_local_os_kind();
    let (resolution, refresh_rate_hz) = probe_local_display();
    let ip = detect_local_lan_ipv4().to_string();

    let id_hash = blake3::hash(format!("{}:{}", hostname, identity.pubkey_hex).as_bytes())
        .to_hex()
        .to_string();
    let local_node_id = format!("node-{}", &id_hash[0..10]);

    let local_node = DeviceNode {
        id: local_node_id.clone(),
        name: hostname.clone(),
        hostname: hostname.clone(),
        os,
        ip,
        resolution,
        refresh_rate_hz,
        is_primary: true,
        grid_pos: GridPos { col: 2, row: 2 },
        auto_pair: true,
        online: true,
        latency_ms: 0.1,
        fingerprint: identity.fingerprint.clone(),
        paired_at: "本机设备".into(),
    };

    let seed = (now_epoch_millis() as u64) ^ 0x5A17_C3E9;

    PersistedDaemonData {
        local_node_id,
        identity_secret_hex: identity.secret_hex,
        nodes: vec![local_node],
        tuning: EngineTuning {
            daemon_active: true,
            hid_polling_hz: 1000,
            edge_delay_ms: 0,
            release_hotkey: "Ctrl + Alt + Shift + Esc".into(),
            linux_wayland_strategy: LinuxWaylandStrategy::PortalAndUinputFallback,
            hide_dock_icon: false,
            local_device_name: hostname,
        },
        clipboard_policy: ClipboardPolicy {
            sync_plain_text: true,
            sync_images: true,
            enable_drag_drop_file: true,
            max_file_size_mb: 500,
            save_dir: "~/Downloads/SpanKVM".into(),
        },
        key_rules: default_key_rules(),
        transfers: Vec::new(),
        pin_seed: seed,
    }
}

fn validate_pin(raw: &str) -> Result<String, String> {
    let digits: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err("请输入对方设备上显示的 6 位数字临时配对码。".into());
    }
    Ok(format!("{}-{}", &digits[0..3], &digits[3..6]))
}

pub fn format_bytes_human(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[tauri::command]
pub async fn get_daemon_snapshot(
    hub: State<'_, Arc<DaemonHub>>,
) -> Result<DaemonSnapshotDto, String> {
    Ok(hub.build_snapshot())
}

#[tauri::command]
pub async fn dispatch_daemon_ipc(
    app_handle: AppHandle,
    hub: State<'_, Arc<DaemonHub>>,
    command: String,
    payload: Option<Value>,
) -> Result<IpcEnvelope, String> {
    let payload = payload.unwrap_or(Value::Object(serde_json::Map::new()));

    match command.as_str() {
        "get_daemon_snapshot" => Ok(IpcEnvelope {
            ok: true,
            snapshot: Some(hub.build_snapshot()),
            error: None,
            swapped_with: None,
            added_node: None,
        }),

        "scan_lan_peers" => {
            hub.emit_beacon_now();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "request_input_permission" => {
            prompt_input_permission();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "open_receive_dir" => {
            let dir = {
                let guard = hub.state.read().map_err(|e| e.to_string())?;
                expand_save_dir(&guard.data.clipboard_policy.save_dir)
            };
            let _ = fs::create_dir_all(&dir);
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("open").arg(&dir).spawn();
            }
            #[cfg(target_os = "windows")]
            {
                let _ = std::process::Command::new("explorer").arg(&dir).spawn();
            }
            #[cfg(target_os = "linux")]
            {
                let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
            }
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "set_primary_node" => {
            let primary_id = payload
                .get("primaryId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                if let Err(err) = set_primary_node(&mut guard.data.nodes, &primary_id) {
                    return Ok(IpcEnvelope {
                        ok: false,
                        snapshot: None,
                        error: Some(err.to_string()),
                        swapped_with: None,
                        added_node: None,
                    });
                }
            }
            hub.persist();
            hub.sync_capture_edges();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "move_node_cell" => {
            let node_id = payload
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let col = payload.get("col").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let row = payload.get("row").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

            let swapped_with = {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                let occupant_name = guard
                    .data
                    .nodes
                    .iter()
                    .find(|n| n.grid_pos.col == col && n.grid_pos.row == row && n.id != node_id)
                    .map(|n| n.name.clone());

                match move_node_to_cell(&guard.data.nodes, &node_id, col, row) {
                    Ok(updated) => {
                        guard.data.nodes = updated;
                        occupant_name
                    }
                    Err(err) => {
                        return Ok(IpcEnvelope {
                            ok: false,
                            snapshot: None,
                            error: Some(err.to_string()),
                            swapped_with: None,
                            added_node: None,
                        });
                    }
                }
            };

            hub.persist();
            hub.sync_capture_edges();

            // Broadcast updated topology layout to connected peers
            let nodes_copy = hub
                .state
                .read()
                .map(|g| g.data.nodes.clone())
                .unwrap_or_default();
            let active_sessions: Vec<QuicPeerSession> = hub
                .sessions
                .read()
                .map(|s| s.values().cloned().collect())
                .unwrap_or_default();
            for sess in active_sessions {
                let frame = ReliableControlFrame::TopologySync {
                    nodes: nodes_copy.clone(),
                };
                tauri::async_runtime::spawn(async move {
                    let _ = sess.send_reliable_frame(&frame, false).await;
                });
            }

            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with,
                added_node: None,
            })
        }

        "update_node_refresh_rate" => {
            let node_id = payload
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let hz = payload.get("hz").and_then(|v| v.as_u64()).unwrap_or(120) as u16;
            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                for n in &mut guard.data.nodes {
                    if n.id == node_id {
                        n.refresh_rate_hz = hz;
                    }
                }
            }
            hub.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "update_engine_tuning" => {
            let mut dock_changed: Option<bool> = None;
            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                if let Some(patch) = payload.get("tuning").and_then(|v| v.as_object()) {
                    if let Some(v) = patch.get("daemonActive").and_then(|v| v.as_bool()) {
                        guard.data.tuning.daemon_active = v;
                    }
                    if let Some(v) = patch.get("hidPollingHz").and_then(|v| v.as_u64()) {
                        guard.data.tuning.hid_polling_hz = v as u16;
                    }
                    if let Some(v) = patch.get("edgeDelayMs").and_then(|v| v.as_u64()) {
                        guard.data.tuning.edge_delay_ms = v as u16;
                    }
                    if let Some(v) = patch.get("releaseHotkey").and_then(|v| v.as_str()) {
                        guard.data.tuning.release_hotkey = v.to_string();
                    }
                    if let Some(v) = patch.get("linuxWaylandStrategy").and_then(|v| v.as_str()) {
                        guard.data.tuning.linux_wayland_strategy = match v {
                            "portal-only" => LinuxWaylandStrategy::PortalOnly,
                            _ => LinuxWaylandStrategy::PortalAndUinputFallback,
                        };
                    }
                    if let Some(hide_dock) = patch.get("hideDockIcon").and_then(|v| v.as_bool()) {
                        if guard.data.tuning.hide_dock_icon != hide_dock {
                            guard.data.tuning.hide_dock_icon = hide_dock;
                            dock_changed = Some(hide_dock);
                        }
                    }
                    if let Some(name) = patch.get("localDeviceName").and_then(|v| v.as_str()) {
                        let cleaned = name.trim();
                        if !cleaned.is_empty() {
                            guard.data.tuning.local_device_name = cleaned.to_string();
                            let local_id = guard.data.local_node_id.clone();
                            for n in &mut guard.data.nodes {
                                if n.id == local_id {
                                    n.name = cleaned.to_string();
                                }
                            }
                        }
                    }
                }
            }

            if let Some(hide_dock) = dock_changed {
                #[cfg(target_os = "macos")]
                {
                    let _ = app_handle.set_dock_visibility(!hide_dock);
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = (&app_handle, hide_dock);
                }
            }

            hub.persist();
            hub.sync_capture_edges();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "jump_cursor_to_node" => {
            let target_id = payload
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let (local_id, target_sess) = {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                if !guard.data.nodes.iter().any(|n| n.id == target_id) {
                    return Ok(IpcEnvelope {
                        ok: false,
                        snapshot: None,
                        error: Some("目标屏幕不存在。".into()),
                        swapped_with: None,
                        added_node: None,
                    });
                }
                if guard.cursor.active_node_id != target_id {
                    guard.cursor.transition_count += 1;
                }
                guard.cursor.active_node_id = target_id.clone();
                guard.cursor.x = 0.5;
                guard.cursor.y = 0.5;
                guard.router.active_node_id = target_id.clone();
                guard.router.norm_x = 0.5;
                guard.router.norm_y = 0.5;

                let sess = hub
                    .sessions
                    .read()
                    .ok()
                    .and_then(|s| s.get(&target_id).cloned());
                (guard.data.local_node_id.clone(), sess)
            };

            if target_id == local_id {
                hub.capture_ctrl.set_suppressing(false);
                if let Ok(mut inj_guard) = hub.injector.lock() {
                    if let Some(ref mut inj) = *inj_guard {
                        let _ = inj.inject_mouse_warp(0.5, 0.5);
                    }
                }
            } else if let Some(sess) = target_sess {
                hub.capture_ctrl.set_suppressing(true);
                let frame = ReliableControlFrame::FocusEnter {
                    node_id: target_id,
                    normalized_x: 0.5,
                    normalized_y: 0.5,
                };
                tauri::async_runtime::spawn(async move {
                    let _ = sess.send_reliable_frame(&frame, true).await;
                });
            }

            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "emergency_return_primary" => {
            hub.capture_ctrl.set_suppressing(false);
            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                let local_id = guard.data.local_node_id.clone();
                let outcome = guard.router.force_return_to_primary(&local_id);
                guard.cursor.active_node_id = outcome.active_node_id;
                guard.cursor.x = 0.5;
                guard.cursor.y = 0.5;
                guard.cursor.last_transition_edge = None;
            }
            let sessions: Vec<QuicPeerSession> = hub
                .sessions
                .read()
                .map(|s| s.values().cloned().collect())
                .unwrap_or_default();
            for sess in sessions {
                tauri::async_runtime::spawn(async move {
                    let _ = sess
                        .send_reliable_frame(&ReliableControlFrame::ReleaseAllHeldKeys, true)
                        .await;
                });
            }
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "rotate_ephemeral_pin" => {
            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                guard.data.pin_seed = guard
                    .data
                    .pin_seed
                    .wrapping_add(137_911 + ((now_epoch_millis() as u64) % 899));
            }
            hub.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "pair_peer_spake2" | "pair_by_ip_spake2" => {
            let pin_input = payload
                .get("pinInput")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let normalized_pin = match validate_pin(pin_input) {
                Ok(p) => p,
                Err(err) => {
                    return Ok(IpcEnvelope {
                        ok: false,
                        snapshot: None,
                        error: Some(err),
                        swapped_with: None,
                        added_node: None,
                    });
                }
            };

            let auto_pair = payload
                .get("autoPair")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            // Resolve target SocketAddr either from discovered peerId or manual IP:port
            let target_addr: SocketAddr = if command == "pair_by_ip_spake2" {
                let raw_ip = payload
                    .get("ip")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .trim();
                let port = payload
                    .get("port")
                    .and_then(|v| v.as_u64())
                    .map(|p| p as u16)
                    .unwrap_or(DEFAULT_QUIC_PORT);

                if let Ok(sock_addr) = raw_ip.parse::<SocketAddr>() {
                    sock_addr
                } else if let Ok(ip_addr) = raw_ip.parse::<IpAddr>() {
                    SocketAddr::new(ip_addr, port)
                } else {
                    return Ok(IpcEnvelope {
                        ok: false,
                        snapshot: None,
                        error: Some("请输入有效的局域网 IPv4 地址（例如 192.168.1.25）。".into()),
                        swapped_with: None,
                        added_node: None,
                    });
                }
            } else {
                let peer_id = payload
                    .get("peerId")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let found = {
                    let guard = hub.state.read().map_err(|e| e.to_string())?;
                    guard
                        .discovered_peers
                        .iter()
                        .find(|p| p.id == peer_id)
                        .cloned()
                };
                let Some(peer) = found else {
                    return Ok(IpcEnvelope {
                        ok: false,
                        snapshot: None,
                        error: Some("该局域网设备已离线或不存在，请刷新扫描或使用 IP 直接配对。".into()),
                        swapped_with: None,
                        added_node: None,
                    });
                };
                let ip: IpAddr = peer.ip.parse().map_err(|_| "无效的设备 IP 地址")?;
                SocketAddr::new(ip, peer.quic_port)
            };

            let node_count = hub
                .state
                .read()
                .map(|g| g.data.nodes.len())
                .unwrap_or(0);
            if node_count >= MAX_DEVICES {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some(format!("最多支持 {MAX_DEVICES} 台电脑互联，当前已达上限。")),
                    swapped_with: None,
                    added_node: None,
                });
            }

            let endpoint_opt = hub.quic_endpoint.read().ok().and_then(|g| g.clone());
            let Some(endpoint) = endpoint_opt else {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some("本地 QUIC 传输引擎尚未初始化完成。".into()),
                    swapped_with: None,
                    added_node: None,
                });
            };

            let local_node = hub.local_node_descriptor();
            match client_pair_spake2_over_quic(
                &endpoint,
                target_addr,
                local_node,
                &normalized_pin,
                auto_pair,
            )
            .await
            {
                Ok((session, mut remote_node)) => {
                    let added = {
                        let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                        let slot = find_next_adjacent_slot(&guard.data.nodes)
                            .unwrap_or(GridPos { col: 3, row: 2 });
                        remote_node.is_primary = false;
                        remote_node.auto_pair = auto_pair;
                        remote_node.online = true;
                        remote_node.ip = target_addr.ip().to_string();
                        remote_node.latency_ms = session.rtt_ms();
                        remote_node.paired_at = format!("今日 {}", &now_hms_string()[0..5]);
                        if let Some(existing) = guard
                            .data
                            .nodes
                            .iter_mut()
                            .find(|n| n.id == remote_node.id)
                        {
                            existing.ip = remote_node.ip.clone();
                            existing.online = true;
                            existing.auto_pair = auto_pair;
                            existing.fingerprint = remote_node.fingerprint.clone();
                            existing.clone()
                        } else {
                            remote_node.grid_pos = slot;
                            guard.data.nodes.push(remote_node.clone());
                            remote_node.clone()
                        }
                    };

                    crate::runtime::register_and_spawn_peer_session(
                        Arc::clone(&hub),
                        app_handle.clone(),
                        session,
                    );
                    hub.persist();
                    hub.notify_frontend(&app_handle);

                    Ok(IpcEnvelope {
                        ok: true,
                        snapshot: Some(hub.build_snapshot()),
                        error: None,
                        swapped_with: None,
                        added_node: Some(added),
                    })
                }
                Err(net_err) => Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some(net_err.to_string()),
                    swapped_with: None,
                    added_node: None,
                }),
            }
        }

        "toggle_auto_pair" => {
            let node_id = payload
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                for n in &mut guard.data.nodes {
                    if n.id == node_id {
                        n.auto_pair = !n.auto_pair;
                    }
                }
            }
            hub.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "remove_paired_node" => {
            let node_id = payload
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            if let Ok(mut sessions) = hub.sessions.write() {
                if let Some(sess) = sessions.remove(&node_id) {
                    sess.connection.close(quinn::VarInt::from_u32(200), b"unpaired");
                }
            }

            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                let local_id = guard.data.local_node_id.clone();
                if node_id != local_id {
                    guard.data.nodes.retain(|n| n.id != node_id);
                    if guard.cursor.active_node_id == node_id {
                        guard.cursor.active_node_id = local_id;
                        guard.cursor.x = 0.5;
                        guard.cursor.y = 0.5;
                        hub.capture_ctrl.set_suppressing(false);
                    }
                }
            }
            hub.persist();
            hub.sync_capture_edges();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "add_key_rule" => {
            let Some(rule_obj) = payload.get("rule") else {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some("缺少按键映射参数。".into()),
                    swapped_with: None,
                    added_node: None,
                });
            };
            let label = rule_obj
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let target_os = rule_obj
                .get("targetOs")
                .and_then(|v| v.as_str())
                .unwrap_or("all")
                .to_string();
            let source_key = rule_obj
                .get("sourceKey")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let target_key = rule_obj
                .get("targetKey")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            let enabled = rule_obj
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            if source_key.is_empty() || target_key.is_empty() {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some("请选择源按键与目标映射按键。".into()),
                    swapped_with: None,
                    added_node: None,
                });
            }
            if source_key == target_key {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some("源按键与目标按键不能相同。".into()),
                    swapped_with: None,
                    added_node: None,
                });
            }

            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                let duplicate = guard.data.key_rules.iter().any(|r| {
                    r.source_key == source_key
                        && (r.target_os == target_os || r.target_os == "all" || target_os == "all")
                });
                if duplicate {
                    return Ok(IpcEnvelope {
                        ok: false,
                        snapshot: None,
                        error: Some(
                            "该系统范围下已存在相同源按键的映射规则，请先删除旧规则。".into(),
                        ),
                        swapped_with: None,
                        added_node: None,
                    });
                }

                guard.data.key_rules.insert(
                    0,
                    KeyMappingRule {
                        id: format!("rule-{}", now_epoch_millis()),
                        label: if label.is_empty() {
                            format!("{source_key} → {target_key}")
                        } else {
                            label
                        },
                        target_os,
                        source_key,
                        target_key,
                        enabled,
                    },
                );
            }
            hub.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "toggle_key_rule" => {
            let rule_id = payload
                .get("ruleId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                for r in &mut guard.data.key_rules {
                    if r.id == rule_id {
                        r.enabled = !r.enabled;
                    }
                }
            }
            hub.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "delete_key_rule" => {
            let rule_id = payload
                .get("ruleId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                guard.data.key_rules.retain(|r| r.id != rule_id);
            }
            hub.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "update_clipboard_policy" => {
            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                if let Some(patch) = payload.get("policy").and_then(|v| v.as_object()) {
                    if let Some(v) = patch.get("syncPlainText").and_then(|v| v.as_bool()) {
                        guard.data.clipboard_policy.sync_plain_text = v;
                    }
                    if let Some(v) = patch.get("syncImages").and_then(|v| v.as_bool()) {
                        guard.data.clipboard_policy.sync_images = v;
                    }
                    if let Some(v) = patch.get("enableDragDropFile").and_then(|v| v.as_bool()) {
                        guard.data.clipboard_policy.enable_drag_drop_file = v;
                    }
                    if let Some(v) = patch.get("maxFileSizeMb").and_then(|v| v.as_u64()) {
                        guard.data.clipboard_policy.max_file_size_mb = v as u32;
                    }
                    if let Some(v) = patch.get("saveDir").and_then(|v| v.as_str()) {
                        guard.data.clipboard_policy.save_dir = v.to_string();
                    }
                }
            }
            hub.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "broadcast_clipboard_text" => {
            let text = payload
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if text.trim().is_empty() {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some("剪贴板文本内容不能为空。".into()),
                    swapped_with: None,
                    added_node: None,
                });
            }

            // Write directly to local OS clipboard AND broadcast to all connected QUIC peers
            let _ = write_os_clipboard_text(&text);
            let digest = compute_chunk_blake3_hex(text.as_bytes());

            let active_sessions: Vec<QuicPeerSession> = hub
                .sessions
                .read()
                .map(|s| s.values().cloned().collect())
                .unwrap_or_default();
            let peer_count = active_sessions.len();

            for sess in active_sessions {
                let frame = ReliableControlFrame::ClipboardSync {
                    mime: "text/plain;charset=utf-8".into(),
                    blake3_hex: digest.clone(),
                    text: Some(text.clone()),
                    image: None,
                };
                tauri::async_runtime::spawn(async move {
                    let _ = sess.send_reliable_frame(&frame, false).await;
                });
            }

            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                guard.last_synced_clipboard_hash = digest.clone();
                let from_name = guard.data.tuning.local_device_name.clone();
                let to_label = if peer_count > 0 {
                    format!("已同步至 {peer_count} 台在线设备")
                } else {
                    "本机系统剪贴板（暂无在线远端设备）".into()
                };
                let title = if text.chars().count() > 48 {
                    let prefix: String = text.chars().take(48).collect();
                    format!("{prefix}...")
                } else {
                    text.clone()
                };
                guard.data.transfers.insert(
                    0,
                    TransferRecordDto {
                        id: format!("tr-{}", now_epoch_millis()),
                        record_type: "clipboard-text".into(),
                        title,
                        size_label: format_bytes_human(text.len()),
                        from_node_name: from_name,
                        to_node_name: to_label,
                        timestamp: now_hms_string(),
                        encrypted: true,
                        status: "completed".into(),
                        digest_hex: Some(digest[0..16].to_string()),
                    },
                );
            }
            hub.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "send_file_payload" => {
            let file_name = payload
                .get("fileName")
                .and_then(|v| v.as_str())
                .unwrap_or("transfer.bin")
                .to_string();
            let target_node_id = payload
                .get("targetNodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let b64 = payload
                .get("base64Data")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            let bytes = match decode_base64(b64) {
                Ok(b) => b,
                Err(e) => {
                    return Ok(IpcEnvelope {
                        ok: false,
                        snapshot: None,
                        error: Some(format!("文件编码解析失败: {e}")),
                        swapped_with: None,
                        added_node: None,
                    });
                }
            };

            let (local_id, local_name, max_mb, save_dir, target_name) = {
                let guard = hub.state.read().map_err(|e| e.to_string())?;
                if !guard.data.clipboard_policy.enable_drag_drop_file {
                    return Ok(IpcEnvelope {
                        ok: false,
                        snapshot: None,
                        error: Some("文件拖拽传输开关当前已关闭，请先开启后再发送。".into()),
                        swapped_with: None,
                        added_node: None,
                    });
                }
                let tname = guard
                    .data
                    .nodes
                    .iter()
                    .find(|n| n.id == target_node_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "目标设备".into());
                (
                    guard.data.local_node_id.clone(),
                    guard.data.tuning.local_device_name.clone(),
                    guard.data.clipboard_policy.max_file_size_mb,
                    guard.data.clipboard_policy.save_dir.clone(),
                    tname,
                )
            };

            if bytes.len() > (max_mb as usize) * 1024 * 1024 {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some(format!("文件大小超过当前设置的 {max_mb} MB 上限。")),
                    swapped_with: None,
                    added_node: None,
                });
            }

            let full_digest = compute_chunk_blake3_hex(&bytes);
            let safe_name = PathBuf::from(&file_name)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "file.bin".into());

            if target_node_id.is_empty() || target_node_id == local_id {
                // Save into local receive folder for local verification
                let out_dir = expand_save_dir(&save_dir);
                let _ = fs::create_dir_all(&out_dir);
                let out_path = out_dir.join(&safe_name);
                fs::write(&out_path, &bytes)
                    .map_err(|e| format!("写入本地接收目录失败: {e}"))?;
            } else {
                let sess_opt = hub
                    .sessions
                    .read()
                    .ok()
                    .and_then(|s| s.get(&target_node_id).cloned());
                let Some(sess) = sess_opt else {
                    return Ok(IpcEnvelope {
                        ok: false,
                        snapshot: None,
                        error: Some(format!(
                            "目标设备「{target_name}」当前未建立活跃加密连接，请确认两台电脑在线。"
                        )),
                        swapped_with: None,
                        added_node: None,
                    });
                };

                let transfer_id = format!("xf-{}", now_epoch_millis());
                let chunks: Vec<&[u8]> = if bytes.is_empty() {
                    vec![&[]]
                } else {
                    bytes.chunks(FILE_CHUNK_SIZE).collect()
                };
                let total_chunks = chunks.len() as u32;
                let total_bytes = bytes.len() as u64;

                for (idx, chunk) in chunks.into_iter().enumerate() {
                    let frame = ReliableControlFrame::FileChunk {
                        transfer_id: transfer_id.clone(),
                        file_name: safe_name.clone(),
                        chunk_index: idx as u32,
                        total_chunks,
                        total_bytes,
                        chunk_blake3_hex: compute_chunk_blake3_hex(chunk),
                        bytes_b64: encode_base64(chunk),
                    };
                    if let Err(err) = sess.send_reliable_frame(&frame, false).await {
                        return Ok(IpcEnvelope {
                            ok: false,
                            snapshot: None,
                            error: Some(format!("文件分块传输中断: {err}")),
                            swapped_with: None,
                            added_node: None,
                        });
                    }
                }
            }

            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                guard.data.transfers.insert(
                    0,
                    TransferRecordDto {
                        id: format!("tr-{}", now_epoch_millis()),
                        record_type: "file-drop".into(),
                        title: safe_name,
                        size_label: format_bytes_human(bytes.len()),
                        from_node_name: local_name,
                        to_node_name: target_name,
                        timestamp: now_hms_string(),
                        encrypted: true,
                        status: "completed".into(),
                        digest_hex: Some(full_digest[0..16].to_string()),
                    },
                );
            }
            hub.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "clear_transfer_records" => {
            {
                let mut guard = hub.state.write().map_err(|e| e.to_string())?;
                guard.data.transfers.clear();
            }
            hub.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(hub.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        unknown => Ok(IpcEnvelope {
            ok: false,
            snapshot: None,
            error: Some(format!("未知桌面守护命令: {unknown}")),
            swapped_with: None,
            added_node: None,
        }),
    }
}
