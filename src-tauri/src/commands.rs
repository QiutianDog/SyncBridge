use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use spankvm_core::{compute_neighbors, move_node_to_cell, set_primary_node};
use spankvm_pal::detect_active_driver_stack;
use spankvm_proto::{
    ClipboardPolicy, DeviceNode, EngineTuning, GridPos, KeyMappingRule, LinuxWaylandStrategy,
    OsKind, Resolution, GRID_COLS, GRID_ROWS, MAX_DEVICES,
};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPeerDto {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub os: OsKind,
    pub ip: String,
    pub resolution: Resolution,
    pub refresh_rate_hz: u16,
    pub latency_ms: f32,
    pub fingerprint: String,
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
    pub nodes: Vec<DeviceNode>,
    pub discovered_peers: Vec<DiscoveredPeerDto>,
    pub cursor: VirtualCursorDto,
    pub tuning: EngineTuning,
    pub clipboard_policy: ClipboardPolicy,
    pub key_rules: Vec<KeyMappingRule>,
    pub transfers: Vec<TransferRecordDto>,
    pub pin_seed: u64,
}

pub struct SharedDaemonState {
    pub data: PersistedDaemonData,
    pub state_file_path: Option<PathBuf>,
}

impl SharedDaemonState {
    pub fn persist(&self) {
        if let Some(ref path) = self.state_file_path {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(serialized) = serde_json::to_string_pretty(&self.data) {
                let _ = fs::write(path, serialized);
            }
        }
    }

    pub fn build_snapshot(&self) -> DaemonSnapshotDto {
        let driver = detect_active_driver_stack(self.data.tuning.linux_wayland_strategy);
        DaemonSnapshotDto {
            nodes: self.data.nodes.clone(),
            discovered_peers: self.data.discovered_peers.clone(),
            cursor: self.data.cursor.clone(),
            tuning: self.data.tuning.clone(),
            clipboard_policy: self.data.clipboard_policy.clone(),
            key_rules: self.data.key_rules.clone(),
            transfers: self.data.transfers.clone(),
            local_ephemeral_pin: format_pin_from_seed(self.data.pin_seed),
            active_driver_name: driver.os_backend_name.to_string(),
        }
    }
}

pub fn format_pin_from_seed(seed: u64) -> String {
    let n = (seed % 900_000) + 100_000;
    format!("{:03}-{:03}", n / 1000, n % 1000)
}

fn now_epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(1_763_400_000_000)
}

fn now_hms_string() -> String {
    let total_secs = (now_epoch_millis() / 1000) % 86_400;
    let hours = (total_secs / 3600) % 24;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    format!("{hours:02}:{mins:02}:{secs:02}")
}

pub fn default_nodes_preset() -> Vec<DeviceNode> {
    vec![
        DeviceNode {
            id: "node-mac-studio".into(),
            name: "MacBook Pro M4".into(),
            hostname: "mbp-m4-studio.local".into(),
            os: OsKind::MacOs,
            ip: "192.168.50.10".into(),
            resolution: Resolution { width: 3024, height: 1964 },
            refresh_rate_hz: 120,
            is_primary: cfg!(target_os = "macos") || (!cfg!(target_os = "windows") && !cfg!(target_os = "linux")),
            grid_pos: GridPos { col: 2, row: 2 },
            auto_pair: true,
            online: true,
            latency_ms: if cfg!(target_os = "macos") { 0.1 } else { 0.4 },
            fingerprint: "ED25519:9A:4C:E2:11".into(),
            paired_at: if cfg!(target_os = "macos") { "本机宿主".into() } else { "已互信节点".into() },
        },
        DeviceNode {
            id: "node-win-tower".into(),
            name: "Win11 主力工作站".into(),
            hostname: "win11-rtx4090.local".into(),
            os: OsKind::Windows,
            ip: "192.168.50.18".into(),
            resolution: Resolution { width: 3840, height: 2160 },
            refresh_rate_hz: 240,
            is_primary: cfg!(target_os = "windows"),
            grid_pos: GridPos { col: 3, row: 2 },
            auto_pair: true,
            online: true,
            latency_ms: if cfg!(target_os = "windows") { 0.1 } else { 0.5 },
            fingerprint: "ED25519:3D:88:7B:04".into(),
            paired_at: if cfg!(target_os = "windows") { "本机宿主".into() } else { "已互信节点".into() },
        },
        DeviceNode {
            id: "node-ubuntu-wayland".into(),
            name: "Ubuntu 编译服务器".into(),
            hostname: "ubuntu-2404-dev.local".into(),
            os: OsKind::LinuxWayland,
            ip: "192.168.50.26".into(),
            resolution: Resolution { width: 2560, height: 1440 },
            refresh_rate_hz: 144,
            is_primary: cfg!(target_os = "linux"),
            grid_pos: GridPos { col: 2, row: 1 },
            auto_pair: true,
            online: true,
            latency_ms: if cfg!(target_os = "linux") { 0.1 } else { 0.6 },
            fingerprint: "ED25519:71:F0:2C:69".into(),
            paired_at: if cfg!(target_os = "linux") { "本机宿主".into() } else { "已互信节点".into() },
        },
        DeviceNode {
            id: "node-arch-x11".into(),
            name: "Arch Linux 测试机".into(),
            hostname: "arch-thinkpad.local".into(),
            os: OsKind::LinuxX11,
            ip: "192.168.50.31".into(),
            resolution: Resolution { width: 2560, height: 1600 },
            refresh_rate_hz: 120,
            is_primary: false,
            grid_pos: GridPos { col: 1, row: 2 },
            auto_pair: true,
            online: true,
            latency_ms: 0.7,
            fingerprint: "ED25519:C5:19:4A:8E".into(),
            paired_at: "已互信节点".into(),
        },
    ]
}

pub fn nine_grid_preset() -> Vec<DeviceNode> {
    let mut base = default_nodes_preset();
    base.extend([
        DeviceNode {
            id: "node-mac-mini".into(),
            name: "Mac mini 构建节点".into(),
            hostname: "mac-mini-ci.local".into(),
            os: OsKind::MacOs,
            ip: "192.168.50.42".into(),
            resolution: Resolution { width: 2560, height: 1440 },
            refresh_rate_hz: 120,
            is_primary: false,
            grid_pos: GridPos { col: 1, row: 1 },
            auto_pair: true,
            online: true,
            latency_ms: 0.6,
            fingerprint: "ED25519:44:B2:10:9F".into(),
            paired_at: "已互信节点".into(),
        },
        DeviceNode {
            id: "node-win-surface".into(),
            name: "Surface 调试终端".into(),
            hostname: "win11-surface.local".into(),
            os: OsKind::Windows,
            ip: "192.168.50.48".into(),
            resolution: Resolution { width: 2880, height: 1920 },
            refresh_rate_hz: 120,
            is_primary: false,
            grid_pos: GridPos { col: 3, row: 1 },
            auto_pair: true,
            online: true,
            latency_ms: 0.8,
            fingerprint: "ED25519:6E:08:FA:23".into(),
            paired_at: "已互信节点".into(),
        },
        DeviceNode {
            id: "node-fedora-lab".into(),
            name: "Fedora Wayland 节点".into(),
            hostname: "fedora-ws.local".into(),
            os: OsKind::LinuxWayland,
            ip: "192.168.50.53".into(),
            resolution: Resolution { width: 2560, height: 1440 },
            refresh_rate_hz: 144,
            is_primary: false,
            grid_pos: GridPos { col: 1, row: 3 },
            auto_pair: true,
            online: true,
            latency_ms: 0.6,
            fingerprint: "ED25519:92:C1:34:7D".into(),
            paired_at: "已互信节点".into(),
        },
        DeviceNode {
            id: "node-win-sim".into(),
            name: "Win11 高刷渲染机".into(),
            hostname: "win11-oled360.local".into(),
            os: OsKind::Windows,
            ip: "192.168.50.61".into(),
            resolution: Resolution { width: 2560, height: 1440 },
            refresh_rate_hz: 360,
            is_primary: false,
            grid_pos: GridPos { col: 2, row: 3 },
            auto_pair: true,
            online: true,
            latency_ms: 0.4,
            fingerprint: "ED25519:18:DA:77:B0".into(),
            paired_at: "已互信节点".into(),
        },
        DeviceNode {
            id: "node-debian-nas".into(),
            name: "Debian 桌面监控端".into(),
            hostname: "debian-ops.local".into(),
            os: OsKind::LinuxX11,
            ip: "192.168.50.75".into(),
            resolution: Resolution { width: 1920, height: 1080 },
            refresh_rate_hz: 60,
            is_primary: false,
            grid_pos: GridPos { col: 3, row: 3 },
            auto_pair: true,
            online: true,
            latency_ms: 0.7,
            fingerprint: "ED25519:53:0E:91:CC".into(),
            paired_at: "已互信节点".into(),
        },
    ]);
    base
}

pub fn default_discovered_peers() -> Vec<DiscoveredPeerDto> {
    vec![
        DiscoveredPeerDto {
            id: "node-mac-mini".into(),
            name: "Mac mini 构建节点".into(),
            hostname: "mac-mini-ci.local".into(),
            os: OsKind::MacOs,
            ip: "192.168.50.42".into(),
            resolution: Resolution { width: 2560, height: 1440 },
            refresh_rate_hz: 120,
            latency_ms: 0.6,
            fingerprint: "ED25519:44:B2:10:9F".into(),
        },
        DiscoveredPeerDto {
            id: "node-win-sim".into(),
            name: "Win11 高刷渲染机".into(),
            hostname: "win11-oled360.local".into(),
            os: OsKind::Windows,
            ip: "192.168.50.61".into(),
            resolution: Resolution { width: 2560, height: 1440 },
            refresh_rate_hz: 360,
            latency_ms: 0.4,
            fingerprint: "ED25519:18:DA:77:B0".into(),
        },
        DiscoveredPeerDto {
            id: "node-fedora-lab".into(),
            name: "Fedora Wayland 节点".into(),
            hostname: "fedora-ws.local".into(),
            os: OsKind::LinuxWayland,
            ip: "192.168.50.53".into(),
            resolution: Resolution { width: 2560, height: 1440 },
            refresh_rate_hz: 144,
            latency_ms: 0.6,
            fingerprint: "ED25519:92:C1:34:7D".into(),
        },
        DiscoveredPeerDto {
            id: "node-win-surface".into(),
            name: "Surface 调试终端".into(),
            hostname: "win11-surface.local".into(),
            os: OsKind::Windows,
            ip: "192.168.50.48".into(),
            resolution: Resolution { width: 2880, height: 1920 },
            refresh_rate_hz: 120,
            latency_ms: 0.8,
            fingerprint: "ED25519:6E:08:FA:23".into(),
        },
        DiscoveredPeerDto {
            id: "node-debian-nas".into(),
            name: "Debian 桌面监控端".into(),
            hostname: "debian-ops.local".into(),
            os: OsKind::LinuxX11,
            ip: "192.168.50.75".into(),
            resolution: Resolution { width: 1920, height: 1080 },
            refresh_rate_hz: 60,
            latency_ms: 0.7,
            fingerprint: "ED25519:53:0E:91:CC".into(),
        },
    ]
}

pub fn default_key_rules() -> Vec<KeyMappingRule> {
    vec![
        KeyMappingRule {
            id: "rule-cmd-win-ctrl".into(),
            label: "Mac 主控快捷键对齐 Windows Ctrl".into(),
            target_os: "windows".into(),
            source_key: "Command (⌘)".into(),
            target_key: "Ctrl".into(),
            enabled: true,
        },
        KeyMappingRule {
            id: "rule-cmd-linux-ctrl".into(),
            label: "Mac 主控快捷键对齐 Linux Ctrl".into(),
            target_os: "linux".into(),
            source_key: "Command (⌘)".into(),
            target_key: "Ctrl".into(),
            enabled: true,
        },
        KeyMappingRule {
            id: "rule-opt-win-alt".into(),
            label: "Mac Option 映射为 Windows Alt".into(),
            target_os: "windows".into(),
            source_key: "Option (⌥)".into(),
            target_key: "Alt".into(),
            enabled: true,
        },
        KeyMappingRule {
            id: "rule-caps-esc".into(),
            label: "Vim 效率键：CapsLock 映射为 Escape".into(),
            target_os: "all".into(),
            source_key: "CapsLock".into(),
            target_key: "Escape".into(),
            enabled: true,
        },
    ]
}

pub fn create_default_persisted_data() -> PersistedDaemonData {
    let nodes = default_nodes_preset();
    let primary_id = nodes
        .iter()
        .find(|n| n.is_primary)
        .map(|n| n.id.clone())
        .unwrap_or_else(|| "node-mac-studio".into());

    PersistedDaemonData {
        nodes,
        discovered_peers: default_discovered_peers(),
        cursor: VirtualCursorDto {
            active_node_id: primary_id,
            x: 0.5,
            y: 0.5,
            transition_count: 0,
            last_transition_edge: None,
        },
        tuning: EngineTuning {
            daemon_active: true,
            hid_polling_hz: 1000,
            edge_delay_ms: 0,
            release_hotkey: "Ctrl + Alt + Shift + Esc".into(),
            linux_wayland_strategy: LinuxWaylandStrategy::PortalAndUinputFallback,
        },
        clipboard_policy: ClipboardPolicy {
            sync_plain_text: true,
            sync_images: true,
            enable_drag_drop_file: true,
            max_file_size_mb: 500,
            save_dir: "~/Downloads/SpanKVM".into(),
        },
        key_rules: default_key_rules(),
        transfers: vec![
            TransferRecordDto {
                id: "tr-1".into(),
                record_type: "file-drop".into(),
                title: "hid_quic_benchmark.csv".into(),
                size_label: "4.8 MB".into(),
                from_node_name: "Ubuntu 编译服务器".into(),
                to_node_name: "MacBook Pro M4".into(),
                timestamp: "19:02:14".into(),
                encrypted: true,
                status: "completed".into(),
                digest_hex: Some("a94f3c8011b2e74d".into()),
            },
            TransferRecordDto {
                id: "tr-2".into(),
                record_type: "clipboard-text".into(),
                title: "SpanKVM QUIC TLS 1.3 会话已就绪".into(),
                size_label: "42 B".into(),
                from_node_name: "MacBook Pro M4".into(),
                to_node_name: "Win11 主力工作站".into(),
                timestamp: "18:54:09".into(),
                encrypted: true,
                status: "completed".into(),
                digest_hex: Some("7c21e04b938fd110".into()),
            },
        ],
        pin_seed: 482_915,
    }
}

fn find_next_adjacent_slot(nodes: &[DeviceNode]) -> Option<GridPos> {
    let dirs = [(0, -1), (1, 0), (0, 1), (-1, 0)];
    let mut anchors: Vec<GridPos> = nodes
        .iter()
        .filter(|n| n.is_primary)
        .map(|n| n.grid_pos)
        .collect();
    for n in nodes {
        if !n.is_primary {
            anchors.push(n.grid_pos);
        }
    }

    for anchor in anchors {
        for (dc, dr) in dirs {
            let col = anchor.col + dc;
            let row = anchor.row + dr;
            if !(0..GRID_COLS).contains(&col) || !(0..GRID_ROWS).contains(&row) {
                continue;
            }
            if !nodes.iter().any(|n| n.grid_pos.col == col && n.grid_pos.row == row) {
                return Some(GridPos { col, row });
            }
        }
    }
    None
}

fn step_cursor_normalized(
    nodes: &[DeviceNode],
    cur: &VirtualCursorDto,
    dx: f32,
    dy: f32,
) -> VirtualCursorDto {
    let mut nx = cur.x + dx;
    let mut ny = cur.y + dy;
    let mut target_id = cur.active_node_id.clone();
    let mut edge: Option<String> = None;

    let neighbors = compute_neighbors(nodes, &target_id);
    if nx < 0.0 {
        if let Some(left) = neighbors.left.filter(|n| n.online) {
            target_id = left.id.clone();
            nx = (1.0 + nx).clamp(0.04, 0.96);
            edge = Some("left".into());
        } else {
            nx = 0.0;
        }
    } else if nx > 1.0 {
        if let Some(right) = neighbors.right.filter(|n| n.online) {
            target_id = right.id.clone();
            nx = (nx - 1.0).clamp(0.04, 0.96);
            edge = Some("right".into());
        } else {
            nx = 1.0;
        }
    }

    let v_neighbors = compute_neighbors(nodes, &target_id);
    if ny < 0.0 {
        if let Some(up) = v_neighbors.up.filter(|n| n.online) {
            target_id = up.id.clone();
            ny = (1.0 + ny).clamp(0.04, 0.96);
            edge = Some("up".into());
        } else {
            ny = 0.0;
        }
    } else if ny > 1.0 {
        if let Some(down) = v_neighbors.down.filter(|n| n.online) {
            target_id = down.id.clone();
            ny = (ny - 1.0).clamp(0.04, 0.96);
            edge = Some("down".into());
        } else {
            ny = 1.0;
        }
    }

    let crossed = target_id != cur.active_node_id;
    VirtualCursorDto {
        active_node_id: target_id,
        x: nx.clamp(0.0, 1.0),
        y: ny.clamp(0.0, 1.0),
        transition_count: if crossed {
            cur.transition_count + 1
        } else {
            cur.transition_count
        },
        last_transition_edge: if crossed {
            edge
        } else {
            cur.last_transition_edge.clone()
        },
    }
}

fn validate_pin(raw: &str) -> Result<String, String> {
    let digits: String = raw.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err("请输入对方屏幕上显示的 6 位数字临时配对码。".into());
    }
    Ok(format!("{}-{}", &digits[0..3], &digits[3..6]))
}

#[tauri::command]
pub fn get_daemon_snapshot(
    state: State<'_, RwLock<SharedDaemonState>>,
) -> Result<DaemonSnapshotDto, String> {
    let guard = state.read().map_err(|e| e.to_string())?;
    Ok(guard.build_snapshot())
}

#[tauri::command]
pub fn dispatch_daemon_ipc(
    command: String,
    payload: Value,
    state: State<'_, RwLock<SharedDaemonState>>,
) -> Result<IpcEnvelope, String> {
    let mut guard = state.write().map_err(|e| e.to_string())?;

    match command.as_str() {
        "get_daemon_snapshot" => Ok(IpcEnvelope {
            ok: true,
            snapshot: Some(guard.build_snapshot()),
            error: None,
            swapped_with: None,
            added_node: None,
        }),

        "set_primary_node" => {
            let primary_id = payload
                .get("primaryId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if let Err(err) = set_primary_node(&mut guard.data.nodes, primary_id) {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some(err.to_string()),
                    swapped_with: None,
                    added_node: None,
                });
            }
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "move_node_cell" => {
            let node_id = payload
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let col = payload.get("col").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let row = payload.get("row").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

            let swapped_with = guard
                .data
                .nodes
                .iter()
                .find(|n| n.id != node_id && n.grid_pos.col == col && n.grid_pos.row == row)
                .map(|n| n.name.clone());

            match move_node_to_cell(&guard.data.nodes, node_id, col, row) {
                Ok(updated) => {
                    guard.data.nodes = updated;
                    guard.persist();
                    Ok(IpcEnvelope {
                        ok: true,
                        snapshot: Some(guard.build_snapshot()),
                        error: None,
                        swapped_with,
                        added_node: None,
                    })
                }
                Err(err) => Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some(err.to_string()),
                    swapped_with: None,
                    added_node: None,
                }),
            }
        }

        "update_node_refresh_rate" => {
            let node_id = payload
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let hz = payload.get("hz").and_then(|v| v.as_u64()).unwrap_or(120) as u16;
            for n in &mut guard.data.nodes {
                if n.id == node_id {
                    n.refresh_rate_hz = hz;
                }
            }
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "update_engine_tuning" => {
            if let Some(patch) = payload.get("tuning").and_then(|v| v.as_object()) {
                if let Some(active) = patch.get("daemonActive").and_then(|v| v.as_bool()) {
                    guard.data.tuning.daemon_active = active;
                }
                if let Some(hz) = patch.get("hidPollingHz").and_then(|v| v.as_u64()) {
                    guard.data.tuning.hid_polling_hz = hz as u16;
                }
                if let Some(delay) = patch.get("edgeDelayMs").and_then(|v| v.as_u64()) {
                    guard.data.tuning.edge_delay_ms = delay as u16;
                }
                if let Some(hotkey) = patch.get("releaseHotkey").and_then(|v| v.as_str()) {
                    guard.data.tuning.release_hotkey = hotkey.to_string();
                }
                if let Some(strat_val) = patch.get("linuxWaylandStrategy") {
                    if let Ok(strat) = serde_json::from_value::<LinuxWaylandStrategy>(strat_val.clone()) {
                        guard.data.tuning.linux_wayland_strategy = strat;
                    }
                }
            }
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "step_virtual_cursor" => {
            if guard.data.tuning.daemon_active {
                let dx = payload.get("dx").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let dy = payload.get("dy").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let next_cursor =
                    step_cursor_normalized(&guard.data.nodes, &guard.data.cursor, dx, dy);
                guard.data.cursor = next_cursor;
                guard.persist();
            }
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "jump_cursor_to_node" => {
            let node_id = payload
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if guard.data.nodes.iter().any(|n| n.id == node_id) {
                let changed = guard.data.cursor.active_node_id != node_id;
                let next_count = if changed {
                    guard.data.cursor.transition_count + 1
                } else {
                    guard.data.cursor.transition_count
                };
                let prev_edge = guard.data.cursor.last_transition_edge.clone();
                guard.data.cursor = VirtualCursorDto {
                    active_node_id: node_id,
                    x: 0.5,
                    y: 0.5,
                    transition_count: next_count,
                    last_transition_edge: prev_edge,
                };
                guard.persist();
            }
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "emergency_return_primary" => {
            let primary_id = guard
                .data
                .nodes
                .iter()
                .find(|n| n.is_primary)
                .or_else(|| guard.data.nodes.first())
                .map(|n| n.id.clone())
                .unwrap_or_default();
            let changed = guard.data.cursor.active_node_id != primary_id;
            let next_count = if changed {
                guard.data.cursor.transition_count + 1
            } else {
                guard.data.cursor.transition_count
            };
            guard.data.cursor = VirtualCursorDto {
                active_node_id: primary_id,
                x: 0.5,
                y: 0.5,
                transition_count: next_count,
                last_transition_edge: None,
            };
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "load_nine_preset" => {
            let nine = nine_grid_preset();
            let primary_id = nine
                .iter()
                .find(|n| n.is_primary)
                .or_else(|| nine.first())
                .map(|n| n.id.clone())
                .unwrap_or_default();
            guard.data.discovered_peers = default_discovered_peers()
                .into_iter()
                .filter(|p| !nine.iter().any(|n| n.id == p.id))
                .collect();
            guard.data.nodes = nine;
            guard.data.cursor = VirtualCursorDto {
                active_node_id: primary_id,
                x: 0.5,
                y: 0.5,
                transition_count: 0,
                last_transition_edge: None,
            };
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "reset_default_preset" => {
            guard.data = create_default_persisted_data();
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "rotate_ephemeral_pin" => {
            let next_seed = guard
                .data
                .pin_seed
                .wrapping_add(137_911 + ((now_epoch_millis() as u64) % 899));
            guard.data.pin_seed = next_seed;
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "pair_peer_spake2" => {
            let pin_input = payload
                .get("pinInput")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if let Err(err) = validate_pin(pin_input) {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some(err),
                    swapped_with: None,
                    added_node: None,
                });
            }
            if guard.data.nodes.len() >= MAX_DEVICES {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some(format!("最多支持 {MAX_DEVICES} 台电脑互联，当前已达到上限。")),
                    swapped_with: None,
                    added_node: None,
                });
            }

            let peer_id = payload
                .get("peerId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let auto_pair = payload
                .get("autoPair")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let Some(peer_idx) = guard
                .data
                .discovered_peers
                .iter()
                .position(|p| p.id == peer_id)
            else {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some("未找到待配对的局域网设备。".into()),
                    swapped_with: None,
                    added_node: None,
                });
            };

            let Some(slot) = find_next_adjacent_slot(&guard.data.nodes) else {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some("当前拓扑画布没有可用的相邻空闲槽位。".into()),
                    swapped_with: None,
                    added_node: None,
                });
            };

            let peer = guard.data.discovered_peers.remove(peer_idx);
            let digest = blake3::hash(
                format!("{}:{}:{}", peer.id, pin_input, now_epoch_millis()).as_bytes(),
            )
            .to_hex()
            .to_string()
            .to_uppercase();
            let fingerprint = format!(
                "ED25519:{}:{}:{}:{}",
                &digest[0..2],
                &digest[2..4],
                &digest[4..6],
                &digest[6..8]
            );

            let new_node = DeviceNode {
                id: peer.id,
                name: peer.name,
                hostname: peer.hostname,
                os: peer.os,
                ip: peer.ip,
                resolution: peer.resolution,
                refresh_rate_hz: peer.refresh_rate_hz,
                is_primary: false,
                grid_pos: slot,
                auto_pair,
                online: true,
                latency_ms: peer.latency_ms,
                fingerprint,
                paired_at: format!("今日 {}", &now_hms_string()[0..5]),
            };

            guard.data.nodes.push(new_node.clone());
            guard.persist();

            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: Some(new_node),
            })
        }

        "toggle_auto_pair" => {
            let node_id = payload
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            for n in &mut guard.data.nodes {
                if n.id == node_id {
                    n.auto_pair = !n.auto_pair;
                }
            }
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "remove_paired_node" => {
            let node_id = payload
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if let Some(pos) = guard
                .data
                .nodes
                .iter()
                .position(|n| n.id == node_id && !n.is_primary)
            {
                let removed = guard.data.nodes.remove(pos);
                guard.data.discovered_peers.insert(
                    0,
                    DiscoveredPeerDto {
                        id: removed.id.clone(),
                        name: removed.name,
                        hostname: removed.hostname,
                        os: removed.os,
                        ip: removed.ip,
                        resolution: removed.resolution,
                        refresh_rate_hz: removed.refresh_rate_hz,
                        latency_ms: removed.latency_ms,
                        fingerprint: removed.fingerprint,
                    },
                );
                if guard.data.cursor.active_node_id == removed.id {
                    let fallback_id = guard.data.nodes.first().map(|n| n.id.clone());
                    if let Some(first_id) = fallback_id {
                        guard.data.cursor.active_node_id = first_id;
                        guard.data.cursor.x = 0.5;
                        guard.data.cursor.y = 0.5;
                    }
                }
                guard.persist();
            }
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
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
            let duplicate = guard.data.key_rules.iter().any(|r| {
                r.source_key == source_key
                    && (r.target_os == target_os || r.target_os == "all" || target_os == "all")
            });
            if duplicate {
                return Ok(IpcEnvelope {
                    ok: false,
                    snapshot: None,
                    error: Some("该系统范围下已存在相同源按键的映射规则，请先停用或删除旧规则。".into()),
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
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
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
            for r in &mut guard.data.key_rules {
                if r.id == rule_id {
                    r.enabled = !r.enabled;
                }
            }
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
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
            guard.data.key_rules.retain(|r| r.id != rule_id);
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "update_clipboard_policy" => {
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
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "push_transfer_record" => {
            if let Some(rec) = payload.get("record") {
                let title = rec
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("跨屏同步内容")
                    .to_string();
                let size_label = rec
                    .get("sizeLabel")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0 B")
                    .to_string();
                let digest = blake3::hash(
                    format!("{title}:{size_label}:{}", now_epoch_millis()).as_bytes(),
                )
                .to_hex()
                .to_string();

                let new_rec = TransferRecordDto {
                    id: format!("tr-{}", now_epoch_millis()),
                    record_type: rec
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("clipboard-text")
                        .to_string(),
                    title,
                    size_label,
                    from_node_name: rec
                        .get("fromNodeName")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    to_node_name: rec
                        .get("toNodeName")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    timestamp: now_hms_string(),
                    encrypted: rec
                        .get("encrypted")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true),
                    status: rec
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("completed")
                        .to_string(),
                    digest_hex: Some(digest[0..16].to_string()),
                };
                guard.data.transfers.insert(0, new_rec);
                guard.persist();
            }
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
                error: None,
                swapped_with: None,
                added_node: None,
            })
        }

        "clear_transfer_records" => {
            guard.data.transfers.clear();
            guard.persist();
            Ok(IpcEnvelope {
                ok: true,
                snapshot: Some(guard.build_snapshot()),
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileEntryDto {
    pub path: String,
    pub category: String,
    pub summary: String,
    pub line_count: usize,
    pub byte_size: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFileDetailDto {
    pub path: String,
    pub category: String,
    pub summary: String,
    pub content: String,
}

const EMBEDDED_WORKSPACE_SOURCES: &[(&str, &str, &str, &str)] = &[
    (
        ".github/workflows/build.yml",
        "GitHub Actions CI/CD",
        "Windows / macOS (ARM64 + Intel) / Linux 四端矩阵自动构建与 Artifacts 产物直出",
        include_str!("../../.github/workflows/build.yml"),
    ),
    (
        "Cargo.toml",
        "Rust Workspace",
        "Cargo Workspace 根清单与统一依赖锁",
        include_str!("../../Cargo.toml"),
    ),
    (
        "crates/spankvm-proto/src/lib.rs",
        "spankvm-proto",
        "14B 高速指针数据报编解码与跨端共享协议结构",
        include_str!("../../crates/spankvm-proto/src/lib.rs"),
    ),
    (
        "crates/spankvm-core/src/lib.rs",
        "spankvm-core",
        "最多 9 屏正交连通求解、跨屏焦点跟随、防粘连看门狗与键位映射",
        include_str!("../../crates/spankvm-core/src/lib.rs"),
    ),
    (
        "crates/spankvm-pal/src/lib.rs",
        "spankvm-pal",
        "跨平台输入捕获与低延迟注入抽象层 Trait",
        include_str!("../../crates/spankvm-pal/src/lib.rs"),
    ),
    (
        "crates/spankvm-pal/src/linux_wayland.rs",
        "spankvm-pal (Wayland)",
        "确认项 B：Wayland Portal (ashpd + libei) + /dev/uinput + X11 双栈驱动",
        include_str!("../../crates/spankvm-pal/src/linux_wayland.rs"),
    ),
    (
        "crates/spankvm-pal/src/macos.rs",
        "spankvm-pal (macOS)",
        "macOS CoreGraphics CGEventTap 捕获冻结与 CGEventPost 注入",
        include_str!("../../crates/spankvm-pal/src/macos.rs"),
    ),
    (
        "crates/spankvm-pal/src/windows.rs",
        "spankvm-pal (Windows)",
        "Windows RawInput 1000Hz 无加速增量捕获与 SendInput 扫描码注入",
        include_str!("../../crates/spankvm-pal/src/windows.rs"),
    ),
    (
        "crates/spankvm-net/src/lib.rs",
        "spankvm-net",
        "纯局域网围栏、SPAKE2 6 位临时码协商与 Quinn QUIC 双通道传输",
        include_str!("../../crates/spankvm-net/src/lib.rs"),
    ),
    (
        "src-tauri/tauri.conf.json",
        "Tauri v2",
        "确认项 A：Tauri v2 桌面窗口、打包配置与安全策略",
        include_str!("../tauri.conf.json"),
    ),
    (
        "src-tauri/Cargo.toml",
        "Tauri v2",
        "Tauri v2 宿主依赖配置与系统托盘特性",
        include_str!("../Cargo.toml"),
    ),
    (
        "src-tauri/src/lib.rs",
        "Tauri v2",
        "Tauri v2 应用生命周期与关闭窗口转入后台托盘守护",
        include_str!("lib.rs"),
    ),
    (
        "src-tauri/src/commands.rs",
        "Tauri v2",
        "Tauri v2 #[tauri::command] 前端全量 IPC 桥接处理器",
        include_str!("commands.rs"),
    ),
    (
        "scripts/setup-linux-uinput.sh",
        "Linux 辅助脚本",
        "Linux Wayland 后备 /dev/uinput 免 root udev 权限一键配置脚本",
        include_str!("../../scripts/setup-linux-uinput.sh"),
    ),
    (
        "src/data/seedData.ts",
        "前端拓扑预设数据",
        "默认 4 屏 / 9 屏九宫格拓扑预设与按键映射种子配置",
        include_str!("../../src/data/seedData.ts"),
    ),
    (
        "README.md",
        "构建文档",
        "macOS / Windows / Linux Wayland 本地编译与打包说明",
        include_str!("../../README.md"),
    ),
];

#[tauri::command]
pub fn get_workspace_files() -> Vec<WorkspaceFileEntryDto> {
    EMBEDDED_WORKSPACE_SOURCES
        .iter()
        .map(|(path, category, summary, content)| WorkspaceFileEntryDto {
            path: (*path).to_string(),
            category: (*category).to_string(),
            summary: (*summary).to_string(),
            line_count: content.lines().count(),
            byte_size: content.len(),
        })
        .collect()
}

#[tauri::command]
pub fn get_workspace_file(file_path: String) -> Option<WorkspaceFileDetailDto> {
    EMBEDDED_WORKSPACE_SOURCES
        .iter()
        .find(|(path, _, _, _)| *path == file_path)
        .map(|(path, category, summary, content)| WorkspaceFileDetailDto {
            path: (*path).to_string(),
            category: (*category).to_string(),
            summary: (*summary).to_string(),
            content: (*content).to_string(),
        })
}
