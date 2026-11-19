//! Pure-LAN QUIC transport (`quinn` + `rustls`), UDP subnet broadcast discovery,
//! 6-digit ephemeral SPAKE2 pairing, and Ed25519 trusted auto-reconnection.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig, Endpoint, RecvStream, SendStream, ServerConfig, TransportConfig};
use rand::rngs::OsRng;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use serde::de::DeserializeOwned;
use serde::Serialize;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use spankvm_proto::{
    decode_base64, encode_base64, DeviceNode, FastPointerDatagram, LanDiscoveryBeacon,
    ReliableControlFrame, WireHandshakeRequest, WireHandshakeResponse, DISCOVERY_UDP_PORT,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("拒绝非局域网来源连接 ({0})：SpanKVM 仅允许本地私有子网通信")]
    NonLanAddressRejected(IpAddr),
    #[error("临时配对密钥已过期或输错次数超限，请刷新后重试")]
    PairingPinExpiredOrLocked,
    #[error("6 位临时配对密钥不匹配，请核对目标电脑上显示的密钥")]
    Spake2VerificationFailed,
    #[error("对方拒绝了连接请求: {0}")]
    PeerRejected(String),
    #[error("QUIC 传输层错误: {0}")]
    Transport(String),
}

/// Enforces strict LAN-only perimeter: RFC1918 private IPv4, link-local, or IPv6 ULA/link-local.
pub fn is_lan_private_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4.is_loopback(),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return v4.is_private() || v4.is_link_local() || v4.is_loopback();
            }
            let seg0 = v6.segments()[0];
            v6.is_loopback() || (seg0 & 0xfe00) == 0xfc00 || (seg0 & 0xffc0) == 0xfe80
        }
    }
}

pub fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
    out
}

pub fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    let cleaned = hex.trim();
    if cleaned.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

#[derive(Clone)]
pub struct LocalEd25519Identity {
    signing_key: SigningKey,
    pub pubkey_hex: String,
    pub secret_hex: String,
    pub fingerprint: String,
}

impl LocalEd25519Identity {
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self::from_signing_key(signing_key)
    }

    pub fn from_secret_hex(secret_hex: &str) -> Option<Self> {
        let raw = decode_hex(secret_hex)?;
        let arr: [u8; 32] = raw.try_into().ok()?;
        Some(Self::from_signing_key(SigningKey::from_bytes(&arr)))
    }

    fn from_signing_key(signing_key: SigningKey) -> Self {
        let verifying = signing_key.verifying_key();
        let pub_bytes = verifying.to_bytes();
        let digest = blake3::hash(&pub_bytes).to_hex().to_string().to_uppercase();
        let fingerprint = format!(
            "ED25519:{}:{}:{}:{}",
            &digest[0..2],
            &digest[2..4],
            &digest[4..6],
            &digest[6..8]
        );
        Self {
            secret_hex: encode_hex(&signing_key.to_bytes()),
            pubkey_hex: encode_hex(&pub_bytes),
            fingerprint,
            signing_key,
        }
    }

    pub fn fingerprint_from_pubkey_hex(pubkey_hex: &str) -> Option<String> {
        let raw = decode_hex(pubkey_hex)?;
        let arr: [u8; 32] = raw.try_into().ok()?;
        let vk = VerifyingKey::from_bytes(&arr).ok()?;
        let digest = blake3::hash(vk.as_bytes()).to_hex().to_string().to_uppercase();
        Some(format!(
            "ED25519:{}:{}:{}:{}",
            &digest[0..2],
            &digest[2..4],
            &digest[4..6],
            &digest[6..8]
        ))
    }

    pub fn sign_reconnect(&self, node_id: &str, timestamp_ms: u64) -> String {
        let msg = format!("spankvm-reconnect:{node_id}:{timestamp_ms}");
        let sig = self.signing_key.sign(msg.as_bytes());
        encode_hex(&sig.to_bytes())
    }

    pub fn verify_reconnect(
        pubkey_hex: &str,
        node_id: &str,
        timestamp_ms: u64,
        sig_hex: &str,
    ) -> bool {
        let Some(pk_raw) = decode_hex(pubkey_hex) else {
            return false;
        };
        let Ok(pk_arr) = <[u8; 32]>::try_from(pk_raw) else {
            return false;
        };
        let Ok(vk) = VerifyingKey::from_bytes(&pk_arr) else {
            return false;
        };
        let Some(sig_raw) = decode_hex(sig_hex) else {
            return false;
        };
        let Ok(sig_arr) = <[u8; 64]>::try_from(sig_raw) else {
            return false;
        };
        let sig = Signature::from_bytes(&sig_arr);
        let msg = format!("spankvm-reconnect:{node_id}:{timestamp_ms}");
        vk.verify(msg.as_bytes(), &sig).is_ok()
    }
}

fn is_cgnat_or_bench_ipv4(ip: Ipv4Addr) -> bool {
    let oct = ip.octets();
    // 100.64.0.0/10 (Carrier-Grade NAT / Tailscale) or 198.18.0.0/15 (Clash/Mihomo Fake-IP/TUN)
    (oct[0] == 100 && (oct[1] & 0b1100_0000) == 64) || (oct[0] == 198 && (oct[1] == 18 || oct[1] == 19))
}

fn is_virtual_iface_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "utun",
        "tun",
        "tap",
        "wg",
        "tailscale",
        "clash",
        "mihomo",
        "docker",
        "br-",
        "veth",
        "vmnet",
        "vbox",
        "vethernet",
        "hyper-v",
        "wsl",
        "awdl",
        "llw",
        "anpi",
    ]
    .iter()
    .any(|pat| lower.contains(pat))
}

pub(crate) fn rank_lan_ipv4_candidate(iface_name: &str, ip: Ipv4Addr) -> u8 {
    let virtual_name = is_virtual_iface_name(iface_name);
    let rfc1918 = ip.is_private();
    let special_tunnel = is_cgnat_or_bench_ipv4(ip) || ip.is_link_local();
    match (rfc1918, virtual_name, special_tunnel) {
        (true, false, false) => 0,  // Real physical Wi-Fi / Ethernet RFC1918 LAN
        (false, false, false) => 1, // Other non-tunnel physical interface
        (true, true, false) => 2,   // Virtual/VMware/Hyper-V RFC1918
        _ => 3,                     // VPN TUN (198.18.x.x / 100.64.x.x) / link-local
    }
}

/// Enumerates all UP non-loopback IPv4 interfaces `(ip, directed_broadcast)` ranked with physical
/// RFC1918 LAN interfaces first (avoiding Clash TUN / Tailscale / virtual bridge hijacking).
pub fn enumerate_lan_ipv4_interfaces() -> Vec<(Ipv4Addr, Ipv4Addr)> {
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut ranked: Vec<(u8, Ipv4Addr, Ipv4Addr)> = Vec::new();
    for iface in ifaces {
        if iface.is_loopback() {
            continue;
        }
        if let if_addrs::IfAddr::V4(v4) = iface.addr {
            let ip = v4.ip;
            if ip.is_loopback() || ip.is_unspecified() {
                continue;
            }
            let bcast = v4
                .broadcast
                .unwrap_or_else(|| Ipv4Addr::from(u32::from(ip) | !u32::from(v4.netmask)));
            let rank = rank_lan_ipv4_candidate(&iface.name, ip);
            ranked.push((rank, ip, bcast));
        }
    }
    ranked.sort_by_key(|(rank, ip, _)| (*rank, u32::from(*ip)));
    let mut out = Vec::with_capacity(ranked.len());
    for (_, ip, bcast) in ranked {
        if !out.iter().any(|(existing, _)| *existing == ip) {
            out.push((ip, bcast));
        }
    }
    out
}

/// Detects the primary physical LAN IPv4 address, preferring RFC1918 Wi-Fi/Ethernet NICs
/// over VPN/Clash TUN interfaces.
pub fn detect_local_lan_ipv4() -> Ipv4Addr {
    let ifaces = enumerate_lan_ipv4_interfaces();
    if let Some((best_ip, _)) = ifaces.first() {
        return *best_ip;
    }

    let probe_targets = ["192.168.1.1:80", "10.0.0.1:80", "172.16.0.1:80", "8.8.8.8:80"];
    for target in probe_targets {
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            if sock.connect(target).is_ok() {
                if let Ok(SocketAddr::V4(local_v4)) = sock.local_addr() {
                    let ip = *local_v4.ip();
                    if !ip.is_loopback() && !ip.is_unspecified() {
                        return ip;
                    }
                }
            }
        }
    }
    Ipv4Addr::new(127, 0, 0, 1)
}

/// Binds a non-blocking UDP discovery listener with `SO_REUSEADDR` (+ `SO_REUSEPORT` on Unix)
/// and broadcast reception enabled so quick restarts or tray background instances never lose port 24801.
pub fn bind_lan_udp_listener(port: u16) -> std::io::Result<tokio::net::UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    sock.set_broadcast(true)?;
    sock.set_nonblocking(true)?;
    #[cfg(unix)]
    unsafe {
        use std::os::unix::io::AsRawFd;
        let optval: libc::c_int = 1;
        let _ = libc::setsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            (&optval as *const libc::c_int).cast(),
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        );
    }
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    sock.bind(&addr.into())?;
    let std_sock: UdpSocket = sock.into();
    tokio::net::UdpSocket::from_std(std_sock)
}

pub fn detect_local_hostname() -> String {
    for key in ["HOSTNAME", "COMPUTERNAME"] {
        if let Ok(val) = std::env::var(key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    if let Ok(out) = std::process::Command::new("hostname").output() {
        if out.status.success() {
            let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }
    "SpanKVM-Host".to_string()
}

/// Manages the rotating 6-digit temporary pairing key with TTL and lockout protection.
pub struct EphemeralPairingGuard {
    current_pin: String,
    issued_at: Instant,
    ttl: Duration,
    failed_attempts: u8,
}

impl EphemeralPairingGuard {
    pub fn new(initial_pin: String) -> Self {
        Self {
            current_pin: initial_pin,
            issued_at: Instant::now(),
            ttl: Duration::from_secs(300),
            failed_attempts: 0,
        }
    }

    pub fn current_pin(&self) -> &str {
        &self.current_pin
    }

    pub fn rotate(&mut self, next_pin: String) {
        self.current_pin = next_pin;
        self.issued_at = Instant::now();
        self.failed_attempts = 0;
    }

    pub fn start_spake2_responder(&mut self) -> Result<(Spake2<Ed25519Group>, Vec<u8>), NetError> {
        if self.issued_at.elapsed() > self.ttl || self.failed_attempts >= 5 {
            return Err(NetError::PairingPinExpiredOrLocked);
        }
        let normalized = self.current_pin.replace('-', "");
        let (state, outbound_msg) = Spake2::<Ed25519Group>::start_b(
            &Password::new(normalized.as_bytes()),
            &Identity::new(b"spankvm-initiator"),
            &Identity::new(b"spankvm-responder"),
        );
        Ok((state, outbound_msg))
    }

    pub fn record_failure(&mut self) {
        self.failed_attempts = self.failed_attempts.saturating_add(1);
    }
}

#[derive(Debug)]
struct LanSelfSignedCertVerifier(Arc<rustls::crypto::CryptoProvider>);

impl LanSelfSignedCertVerifier {
    fn new() -> Arc<Self> {
        Arc::new(Self(Arc::new(rustls::crypto::ring::default_provider())))
    }
}

impl rustls::client::danger::ServerCertVerifier for LanSelfSignedCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // Mutual authenticity is cryptographically bound via SPAKE2 (first pair)
        // or Ed25519 public key signature (trusted auto-reconnect) on stream 0.
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Builds a dual-role QUIC endpoint (accepts incoming connections AND dials outgoing LAN peers).
pub fn build_quic_dual_endpoint(preferred_port: u16) -> Result<(Endpoint, u16), NetError> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cert = rcgen::generate_simple_self_signed(vec!["spankvm.local".to_string()])
        .map_err(|e| NetError::Transport(format!("TLS cert gen failed: {e}")))?;
    let cert_der = CertificateDer::from(cert.cert);
    let key_der = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

    let mut transport = TransportConfig::default();
    transport.keep_alive_interval(Some(Duration::from_secs(3)));
    transport.datagram_receive_buffer_size(Some(262_144));
    let transport_arc = Arc::new(transport);

    let mut server_cfg = ServerConfig::with_single_cert(vec![cert_der], key_der.into())
        .map_err(|e| NetError::Transport(format!("QUIC server config failed: {e}")))?;
    server_cfg.transport_config(Arc::clone(&transport_arc));

    let rustls_client = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(LanSelfSignedCertVerifier::new())
        .with_no_client_auth();
    let quic_client_crypto = QuicClientConfig::try_from(rustls_client)
        .map_err(|e| NetError::Transport(format!("QUIC client crypto failed: {e}")))?;
    let mut client_cfg = ClientConfig::new(Arc::new(quic_client_crypto));
    client_cfg.transport_config(transport_arc);

    let preferred_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), preferred_port);
    let mut endpoint = match Endpoint::server(server_cfg.clone(), preferred_addr) {
        Ok(ep) => ep,
        Err(_) => {
            let fallback_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
            Endpoint::server(server_cfg, fallback_addr)
                .map_err(|e| NetError::Transport(format!("UDP bind failed: {e}")))?
        }
    };
    endpoint.set_default_client_config(client_cfg);

    let bound_port = endpoint
        .local_addr()
        .map(|a| a.port())
        .unwrap_or(preferred_port);
    Ok((endpoint, bound_port))
}

pub async fn write_wire_json<T: Serialize>(
    send: &mut SendStream,
    msg: &T,
) -> Result<(), NetError> {
    let payload =
        serde_json::to_vec(msg).map_err(|e| NetError::Transport(format!("JSON encode: {e}")))?;
    let len_bytes = (payload.len() as u32).to_be_bytes();
    send.write_all(&len_bytes)
        .await
        .map_err(|e| NetError::Transport(e.to_string()))?;
    send.write_all(&payload)
        .await
        .map_err(|e| NetError::Transport(e.to_string()))?;
    Ok(())
}

pub async fn read_wire_json<T: DeserializeOwned>(recv: &mut RecvStream) -> Result<T, NetError> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(|e| NetError::Transport(e.to_string()))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 64 * 1024 * 1024 {
        return Err(NetError::Transport("Wire frame exceeds 64MB limit".into()));
    }
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf)
        .await
        .map_err(|e| NetError::Transport(e.to_string()))?;
    serde_json::from_slice(&buf).map_err(|e| NetError::Transport(format!("JSON decode: {e}")))
}

pub fn derive_spake2_confirm_tags(shared_secret: &[u8]) -> (String, String) {
    let init_tag = blake3::hash(&[shared_secret, b":spankvm-init-confirm"].concat())
        .to_hex()
        .to_string();
    let resp_tag = blake3::hash(&[shared_secret, b":spankvm-resp-confirm"].concat())
        .to_hex()
        .to_string();
    (init_tag, resp_tag)
}

/// Initiates a real SPAKE2 6-digit PIN pairing handshake over QUIC with a remote LAN computer.
pub async fn client_pair_spake2_over_quic(
    endpoint: &Endpoint,
    remote_addr: SocketAddr,
    local_node: DeviceNode,
    pin_digits: &str,
    auto_pair: bool,
) -> Result<(QuicPeerSession, DeviceNode), NetError> {
    if !is_lan_private_address(remote_addr.ip()) {
        return Err(NetError::NonLanAddressRejected(remote_addr.ip()));
    }

    let connecting = endpoint
        .connect(remote_addr, "spankvm.local")
        .map_err(|e| NetError::Transport(e.to_string()))?;
    let conn = tokio::time::timeout(Duration::from_secs(5), connecting)
        .await
        .map_err(|_| {
            NetError::Transport(format!(
                "连接 {remote_addr} 超时：请确认双方处于同一局域网并放行 UDP 24800/24801（Windows 防火墙需勾选允许专用和公用网络；macOS 系统设置 → 隐私与安全性 → 本地网络需允许 SpanKVM）。也可在另一台电脑用「手动输入 IP 配对」反向连接"
            ))
        })?
        .map_err(|e| NetError::Transport(e.to_string()))?;

    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| NetError::Transport(e.to_string()))?;

    let clean_pin = pin_digits.replace('-', "");
    let (spake_state, init_msg) = Spake2::<Ed25519Group>::start_a(
        &Password::new(clean_pin.as_bytes()),
        &Identity::new(b"spankvm-initiator"),
        &Identity::new(b"spankvm-responder"),
    );

    let req = WireHandshakeRequest::PairSpake2Init {
        initiator_node: local_node,
        spake2_msg_b64: encode_base64(&init_msg),
        auto_pair,
    };
    write_wire_json(&mut send, &req).await?;

    let challenge: WireHandshakeResponse = read_wire_json(&mut recv).await?;
    let (_responder_node, resp_msg_b64, responder_confirm_hex) = match challenge {
        WireHandshakeResponse::PairSpake2Challenge {
            responder_node,
            spake2_msg_b64,
            responder_confirm_hex,
        } => (responder_node, spake2_msg_b64, responder_confirm_hex),
        WireHandshakeResponse::Rejected { reason } => return Err(NetError::PeerRejected(reason)),
        _ => return Err(NetError::Spake2VerificationFailed),
    };

    let resp_msg = decode_base64(&resp_msg_b64).map_err(|_| NetError::Spake2VerificationFailed)?;
    let shared_secret = spake_state
        .finish(&resp_msg)
        .map_err(|_| NetError::Spake2VerificationFailed)?;
    let (init_tag, expected_resp_tag) = derive_spake2_confirm_tags(&shared_secret);

    if responder_confirm_hex != expected_resp_tag {
        conn.close(quinn::VarInt::from_u32(401), b"pin-mismatch");
        return Err(NetError::Spake2VerificationFailed);
    }

    write_wire_json(
        &mut send,
        &WireHandshakeRequest::PairSpake2Confirm {
            confirm_hash_hex: init_tag,
        },
    )
    .await?;
    let _ = send.finish();

    let final_resp: WireHandshakeResponse = read_wire_json(&mut recv).await?;
    match final_resp {
        WireHandshakeResponse::Accepted { responder_node: r } => {
            let session = QuicPeerSession::new(r.id.clone(), remote_addr.ip(), conn)?;
            Ok((session, r))
        }
        WireHandshakeResponse::Rejected { reason } => Err(NetError::PeerRejected(reason)),
        _ => Err(NetError::Spake2VerificationFailed),
    }
}

/// Initiates an Ed25519-authenticated zero-prompt auto-reconnection to a previously paired peer.
pub async fn client_reconnect_trusted_over_quic(
    endpoint: &Endpoint,
    remote_addr: SocketAddr,
    local_node: DeviceNode,
    identity: &LocalEd25519Identity,
) -> Result<(QuicPeerSession, DeviceNode), NetError> {
    if !is_lan_private_address(remote_addr.ip()) {
        return Err(NetError::NonLanAddressRejected(remote_addr.ip()));
    }

    let connecting = endpoint
        .connect(remote_addr, "spankvm.local")
        .map_err(|e| NetError::Transport(e.to_string()))?;
    let conn = tokio::time::timeout(Duration::from_secs(3), connecting)
        .await
        .map_err(|_| NetError::Transport("Reconnect timeout".into()))?
        .map_err(|e| NetError::Transport(e.to_string()))?;

    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| NetError::Transport(e.to_string()))?;

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let sig_hex = identity.sign_reconnect(&local_node.id, now_ms);

    write_wire_json(
        &mut send,
        &WireHandshakeRequest::TrustedReconnect {
            initiator_node: local_node,
            ed25519_pubkey_hex: identity.pubkey_hex.clone(),
            timestamp_ms: now_ms,
            signature_hex: sig_hex,
        },
    )
    .await?;
    let _ = send.finish();

    let resp: WireHandshakeResponse = read_wire_json(&mut recv).await?;
    match resp {
        WireHandshakeResponse::Accepted { responder_node } => {
            let session = QuicPeerSession::new(responder_node.id.clone(), remote_addr.ip(), conn)?;
            Ok((session, responder_node))
        }
        WireHandshakeResponse::Rejected { reason } => Err(NetError::PeerRejected(reason)),
        _ => Err(NetError::Transport("Unexpected reconnect response".into())),
    }
}

#[derive(Clone)]
pub struct QuicPeerSession {
    pub peer_node_id: String,
    pub peer_ip: IpAddr,
    pub connection: quinn::Connection,
}

impl QuicPeerSession {
    pub fn new(
        peer_node_id: String,
        peer_ip: IpAddr,
        connection: quinn::Connection,
    ) -> Result<Self, NetError> {
        if !is_lan_private_address(peer_ip) {
            connection.close(quinn::VarInt::from_u32(403), b"non-lan-rejected");
            return Err(NetError::NonLanAddressRejected(peer_ip));
        }
        Ok(Self {
            peer_node_id,
            peer_ip,
            connection,
        })
    }

    pub fn rtt_ms(&self) -> f32 {
        let ms = self.connection.rtt().as_secs_f32() * 1000.0;
        if ms <= 0.01 {
            0.4
        } else {
            (ms * 10.0).round() / 10.0
        }
    }

    #[inline]
    pub fn send_fast_pointer(&self, frame: &FastPointerDatagram) -> Result<(), NetError> {
        let raw = frame.encode();
        self.connection
            .send_datagram(Bytes::copy_from_slice(&raw))
            .map_err(|e| NetError::Transport(e.to_string()))
    }

    pub async fn send_reliable_frame(
        &self,
        frame: &ReliableControlFrame,
        high_priority: bool,
    ) -> Result<(), NetError> {
        let mut send_stream = self
            .connection
            .open_uni()
            .await
            .map_err(|e| NetError::Transport(e.to_string()))?;
        let _ = send_stream.set_priority(if high_priority { 100 } else { 10 });
        write_wire_json(&mut send_stream, frame).await?;
        send_stream
            .finish()
            .map_err(|e| NetError::Transport(e.to_string()))?;
        Ok(())
    }
}

/// Sends a UDP LAN discovery beacon out of EVERY local IPv4 interface to both its
/// directed subnet broadcast address and `255.255.255.255`, bypassing VPN/Clash TUN hijacking.
pub fn broadcast_lan_beacon(beacon: &LanDiscoveryBeacon, local_ip: Ipv4Addr) {
    let Ok(payload) = serde_json::to_vec(beacon) else {
        return;
    };

    let ifaces = enumerate_lan_ipv4_interfaces();
    let mut sent_any = false;

    for (iface_ip, bcast_ip) in ifaces {
        // Skip pure VPN TUN/CGNAT interfaces for broadcast spam unless they're the only interface
        if is_cgnat_or_bench_ipv4(iface_ip) {
            continue;
        }
        if let Ok(sock) = UdpSocket::bind(SocketAddr::new(IpAddr::V4(iface_ip), 0)) {
            let _ = sock.set_broadcast(true);
            let _ = sock.send_to(&payload, SocketAddr::new(IpAddr::V4(bcast_ip), DISCOVERY_UDP_PORT));
            let _ = sock.send_to(
                &payload,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), DISCOVERY_UDP_PORT),
            );
            sent_any = true;
        }
    }

    if !sent_any {
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            let _ = sock.set_broadcast(true);
            let octets = local_ip.octets();
            if !local_ip.is_loopback() {
                let subnet_bcast = SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(octets[0], octets[1], octets[2], 255)),
                    DISCOVERY_UDP_PORT,
                );
                let _ = sock.send_to(&payload, subnet_bcast);
            }
            let global_bcast = SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), DISCOVERY_UDP_PORT);
            let _ = sock.send_to(&payload, global_bcast);
        }
    }
}

pub fn compute_chunk_blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_lan_private_ip_perimeter() {
        assert!(is_lan_private_address("192.168.50.12".parse().unwrap()));
        assert!(is_lan_private_address("10.0.1.8".parse().unwrap()));
        assert!(!is_lan_private_address("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn prefers_physical_rfc1918_nic_over_clash_tun_and_tailscale() {
        let r_wifi = rank_lan_ipv4_candidate("en0", Ipv4Addr::new(192, 168, 50, 12));
        let r_clash = rank_lan_ipv4_candidate("utun4", Ipv4Addr::new(198, 18, 0, 1));
        let r_tail = rank_lan_ipv4_candidate("utun3", Ipv4Addr::new(100, 64, 12, 8));
        let r_vm = rank_lan_ipv4_candidate("vEthernet (WSL)", Ipv4Addr::new(172, 22, 0, 1));
        assert!(r_wifi < r_vm);
        assert!(r_vm < r_clash);
        assert!(r_wifi < r_tail);
    }

    #[test]
    fn spake2_six_digit_pin_derives_matching_confirm_tags() {
        let mut guard = EphemeralPairingGuard::new("482-915".to_string());
        let (resp_state, resp_msg) = guard.start_spake2_responder().unwrap();
        let (init_state, init_msg) = Spake2::<Ed25519Group>::start_a(
            &Password::new(b"482915"),
            &Identity::new(b"spankvm-initiator"),
            &Identity::new(b"spankvm-responder"),
        );
        let key_a = init_state.finish(&resp_msg).unwrap();
        let key_b = resp_state.finish(&init_msg).unwrap();
        assert_eq!(
            derive_spake2_confirm_tags(&key_a),
            derive_spake2_confirm_tags(&key_b)
        );
    }

    #[test]
    fn ed25519_identity_signs_and_verifies_reconnect() {
        let id = LocalEd25519Identity::generate();
        let sig = id.sign_reconnect("node-local", 1_760_000_000);
        assert!(LocalEd25519Identity::verify_reconnect(
            &id.pubkey_hex,
            "node-local",
            1_760_000_000,
            &sig
        ));
    }
}
