//! SpanKVM wire protocol, zero-copy 14-byte fast datagram codec, Base64 codec,
//! LAN discovery beacons, SPAKE2 handshake frames, and shared IPC types.

use serde::{Deserialize, Serialize};

pub const MAX_DEVICES: usize = 9;
pub const GRID_COLS: i32 = 5;
pub const GRID_ROWS: i32 = 5;
pub const DEFAULT_QUIC_PORT: u16 = 24800;
pub const DISCOVERY_UDP_PORT: u16 = 24801;
pub const MDNS_SERVICE_TYPE: &str = "_spankvm._udp.local.";
pub const FILE_CHUNK_SIZE: usize = 128 * 1024;

/// Compact 14-byte pointer frame transmitted over QUIC Unreliable Datagrams (RFC 9221)
/// to eliminate TCP head-of-line blocking on 120Hz–360Hz displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastPointerDatagram {
    /// 0x01 = RelativeDelta, 0x02 = HiResWheel, 0x03 = MouseButton
    pub frame_type: u8,
    /// Target device index (0..9)
    pub target_node: u8,
    /// Monotonic sequence number (drops out-of-order stale frames)
    pub seq: u32,
    /// Sender monotonic clock microseconds (lower 32 bits)
    pub timestamp_us: u32,
    /// Sub-pixel scaled delta X (1/16th px precision) or button id
    pub dx_subpx: i16,
    /// Sub-pixel scaled delta Y (1/16th px precision) or button state
    pub dy_subpx: i16,
}

impl FastPointerDatagram {
    pub const WIRE_LEN: usize = 14;
    pub const TYPE_RELATIVE_DELTA: u8 = 0x01;
    pub const TYPE_HIRES_WHEEL: u8 = 0x02;
    pub const TYPE_MOUSE_BUTTON: u8 = 0x03;

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
    PortalAndUinputFallback,
    PortalOnly,
}

impl Default for LinuxWaylandStrategy {
    fn default() -> Self {
        Self::PortalAndUinputFallback
    }
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
    /// macOS option: hide Dock icon and run purely in menu-bar tray (default: false).
    #[serde(default)]
    pub hide_dock_icon: bool,
    /// Custom friendly name advertised to other LAN computers.
    #[serde(default)]
    pub local_device_name: String,
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

/// UDP broadcast beacon emitted on port 24801 for zero-config LAN discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanDiscoveryBeacon {
    pub magic: String,
    pub node_id: String,
    pub name: String,
    pub hostname: String,
    pub os: OsKind,
    pub quic_port: u16,
    pub resolution: Resolution,
    pub refresh_rate_hz: u16,
    pub fingerprint: String,
    pub sent_epoch_ms: u64,
}

impl LanDiscoveryBeacon {
    pub const MAGIC: &'static str = "SPANKVM_BEACON_V1";
}

/// Initial QUIC bidirectional authentication stream handshake.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WireHandshakeRequest {
    PairSpake2Init {
        initiator_node: DeviceNode,
        spake2_msg_b64: String,
        auto_pair: bool,
    },
    PairSpake2Confirm {
        confirm_hash_hex: String,
    },
    TrustedReconnect {
        initiator_node: DeviceNode,
        ed25519_pubkey_hex: String,
        timestamp_ms: u64,
        signature_hex: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WireHandshakeResponse {
    PairSpake2Challenge {
        responder_node: DeviceNode,
        spake2_msg_b64: String,
        responder_confirm_hex: String,
    },
    Accepted {
        responder_node: DeviceNode,
    },
    Rejected {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardImageWire {
    pub width: u32,
    pub height: u32,
    pub rgba_b64: String,
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
    MouseButton {
        button: u8,
        pressed: bool,
    },
    WheelScroll {
        vertical_120: i32,
        horizontal_120: i32,
    },
    FocusEnter {
        node_id: String,
        normalized_x: f32,
        normalized_y: f32,
    },
    FocusReturnPrimary {
        edge: String,
        normalized_x: f32,
        normalized_y: f32,
    },
    ReleaseAllHeldKeys,
    ClipboardSync {
        mime: String,
        blake3_hex: String,
        text: Option<String>,
        image: Option<ClipboardImageWire>,
    },
    FileChunk {
        transfer_id: String,
        file_name: String,
        chunk_index: u32,
        total_chunks: u32,
        total_bytes: u64,
        chunk_blake3_hex: String,
        bytes_b64: String,
    },
    TopologySync {
        nodes: Vec<DeviceNode>,
    },
}

const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode_base64(input: &[u8]) -> String {
    if input.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(((input.len() + 2) / 3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(B64_TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64_TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut lut = [0xFFu8; 256];
    for (idx, &ch) in B64_TABLE.iter().enumerate() {
        lut[ch as usize] = idx as u8;
    }

    let cleaned: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    if cleaned.len() % 4 != 0 {
        return Err("Invalid base64 length".into());
    }

    let mut out = Vec::with_capacity((cleaned.len() / 4) * 3);
    for quad in cleaned.chunks_exact(4) {
        let pad = quad.iter().rev().take_while(|&&c| c == b'=').count();
        let v0 = lut[quad[0] as usize];
        let v1 = lut[quad[1] as usize];
        let v2 = if quad[2] == b'=' { 0 } else { lut[quad[2] as usize] };
        let v3 = if quad[3] == b'=' { 0 } else { lut[quad[3] as usize] };

        if v0 == 0xFF || v1 == 0xFF || v2 == 0xFF || v3 == 0xFF || pad > 2 {
            return Err("Invalid base64 symbol".into());
        }

        let triple = ((v0 as u32) << 18)
            | ((v1 as u32) << 12)
            | ((v2 as u32) << 6)
            | (v3 as u32);
        out.push(((triple >> 16) & 0xFF) as u8);
        if pad < 2 {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if pad < 1 {
            out.push((triple & 0xFF) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_pointer_datagram_roundtrip_14_bytes() {
        let frame = FastPointerDatagram {
            frame_type: FastPointerDatagram::TYPE_RELATIVE_DELTA,
            target_node: 3,
            seq: 102_400,
            timestamp_us: 998_123,
            dx_subpx: -240,
            dy_subpx: 128,
        };
        let wire = frame.encode();
        assert_eq!(wire.len(), FastPointerDatagram::WIRE_LEN);
        let decoded = FastPointerDatagram::decode(&wire).expect("valid 14-byte frame");
        assert_eq!(decoded, frame);
    }

    #[test]
    fn base64_codec_roundtrip() {
        let raw = b"SpanKVM binary chunk \x00\xFF\x7F\x10";
        let b64 = encode_base64(raw);
        let decoded = decode_base64(&b64).expect("decode ok");
        assert_eq!(decoded, raw);
    }
}
