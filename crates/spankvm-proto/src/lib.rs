//! SpanKVM wire protocol, zero-copy fast datagram codec, and shared IPC types.

use serde::{Deserialize, Serialize};

pub const MAX_DEVICES: usize = 9;
pub const GRID_COLS: i32 = 5;
pub const GRID_ROWS: i32 = 5;
pub const DEFAULT_QUIC_PORT: u16 = 24800;
pub const MDNS_SERVICE_TYPE: &str = "_spankvm._udp.local.";

/// Compact 14-byte pointer frame transmitted over QUIC Unreliable Datagrams (RFC 9221)
/// to eliminate TCP head-of-line blocking on 120Hz–360Hz displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastPointerDatagram {
    /// 0x01 = RelativeDelta, 0x02 = HiResWheel, 0x03 = ButtonMask
    pub frame_type: u8,
    /// Target device index (0..9)
    pub target_node: u8,
    /// Monotonic sequence number (drops out-of-order stale frames)
    pub seq: u32,
    /// Sender monotonic clock microseconds (lower 32 bits)
    pub timestamp_us: u32,
    /// Sub-pixel scaled delta X (1/16th px precision)
    pub dx_subpx: i16,
    /// Sub-pixel scaled delta Y (1/16th px precision)
    pub dy_subpx: i16,
}

impl FastPointerDatagram {
    pub const WIRE_LEN: usize = 14;

    #[inline]
    pub fn encode(&self) -> [u8; Self::WIRE_LEN] {
        let mut buf = [0u8; Self::WIRE_LEN];
        buf[0] = self.frame_type;
        buf[1] = self.target_node;
        buf[2..6].copy_from_slice(&self.seq.to_le_bytes());
        buf[6..10].copy_from_slice(&self.timestamp_us.to_le_bytes());
        buf[10..12].copy_from_slice(&self.dx_subpx.to_le_bytes());
        buf[12..14].copy_from_slice(&self.dy_subpx.to_le_bytes());
        buf
    }

    #[inline]
    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::WIRE_LEN {
            return None;
        }
        Some(Self {
            frame_type: buf[0],
            target_node: buf[1],
            seq: u32::from_le_bytes([buf[2], buf[3], buf[4], buf[5]]),
            timestamp_us: u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]),
            dx_subpx: i16::from_le_bytes([buf[10], buf[11]]),
            dy_subpx: i16::from_le_bytes([buf[12], buf[13]]),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OsKind {
    #[serde(rename = "macos")]
    MacOs,
    #[serde(rename = "windows")]
    Windows,
    #[serde(rename = "linux-wayland")]
    LinuxWayland,
    #[serde(rename = "linux-x11")]
    LinuxX11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsFamily {
    MacOs,
    Windows,
    Linux,
}

impl OsKind {
    pub fn family(self) -> OsFamily {
        match self {
            OsKind::MacOs => OsFamily::MacOs,
            OsKind::Windows => OsFamily::Windows,
            OsKind::LinuxWayland | OsKind::LinuxX11 => OsFamily::Linux,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinuxWaylandStrategy {
    /// Confirmed Option B (Recommended): Wayland InputCapture/RemoteDesktop Portal (libei)
    /// with automatic fallback to /dev/uinput + X11 XI2/XTest.
    PortalAndUinputFallback,
    /// Strict XDG Desktop Portal (libei) + X11 without kernel /dev/uinput fallback.
    PortalOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridPos {
    pub col: i32,
    pub row: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceNode {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub os: OsKind,
    pub ip: String,
    pub resolution: Resolution,
    pub refresh_rate_hz: u16,
    pub is_primary: bool,
    pub grid_pos: GridPos,
    pub auto_pair: bool,
    pub online: bool,
    pub latency_ms: f32,
    pub fingerprint: String,
    pub paired_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyMappingRule {
    pub id: String,
    pub label: String,
    /// "all" | "macos" | "windows" | "linux"
    pub target_os: String,
    pub source_key: String,
    pub target_key: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTuning {
    pub daemon_active: bool,
    pub hid_polling_hz: u16,
    pub edge_delay_ms: u16,
    pub release_hotkey: String,
    pub linux_wayland_strategy: LinuxWaylandStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardPolicy {
    pub sync_plain_text: bool,
    pub sync_images: bool,
    pub enable_drag_drop_file: bool,
    pub max_file_size_mb: u32,
    pub save_dir: String,
}

/// Reliable prioritized QUIC stream messages (keyboard state, focus handoff, clipboard, file chunks).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReliableControlFrame {
    KeyTransition {
        scancode: u16,
        symbolic_key: String,
        pressed: bool,
    },
    FocusEnter {
        node_id: String,
        normalized_x: f32,
        normalized_y: f32,
    },
    ReleaseAllHeldKeys,
    ClipboardSync {
        mime: String,
        blake3_hex: String,
        payload: Vec<u8>,
    },
    FileChunk {
        transfer_id: String,
        file_name: String,
        offset: u64,
        total_bytes: u64,
        chunk_blake3_hex: String,
        bytes: Vec<u8>,
    },
}
