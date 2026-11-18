use std::collections::HashMap;
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use spake2::{Ed25519Group, Identity, Password, Spake2};
use spankvm_core::{compute_neighbors, find_next_adjacent_slot, resolve_mapped_key};
use spankvm_net::{
    build_quic_dual_endpoint, client_reconnect_trusted_over_quic, compute_chunk_blake3_hex,
    derive_spake2_confirm_tags, is_lan_private_address, read_wire_json, write_wire_json,
    LocalEd25519Identity, QuicPeerSession,
};
use spankvm_pal::{
    clipboard::{
        os_clipboard_sequence_token, read_os_clipboard_snapshot, write_os_clipboard_png_or_rgba,
        write_os_clipboard_text, ClipboardSnapshot,
    },
    spawn_platform_capture_thread, CapturedInputEvent,
};
use spankvm_proto::{
    decode_base64, encode_base64, ClipboardImageWire, DeviceNode, FastPointerDatagram, GridPos,
    LanDiscoveryBeacon, ReliableControlFrame, WireHandshakeRequest, WireHandshakeResponse,
    DEFAULT_QUIC_PORT, DISCOVERY_UDP_PORT, MAX_DEVICES,
};
use tauri::AppHandle;
use tokio::net::UdpSocket as TokioUdpSocket;
use tokio::sync::mpsc::unbounded_channel;

use crate::commands::{
    expand_save_dir, format_bytes_human, format_pin_from_seed, now_epoch_millis, now_hms_string,
    DaemonHub, DiscoveredPeerDto, TransferRecordDto,
};

struct PendingFileBuffer {
    file_name: String,
    total_chunks: u32,
    received_chunks: HashMap<u32, Vec<u8>>,
}

pub fn start_background_services(hub: Arc<DaemonHub>, app_handle: AppHandle) {
    // 1. Start native OS input capture hook thread (macOS CGEventTap / Win32 LLHook / Linux X11+Wayland)
    let (cap_tx, mut cap_rx) = unbounded_channel::<CapturedInputEvent>();
    spawn_platform_capture_thread(Arc::clone(&hub.capture_ctrl), cap_tx);

    // 2. Start Tokio async network + routing + clipboard tasks via Tauri's managed runtime
    tauri::async_runtime::spawn(async move {
        hub.sync_capture_edges();

        // Bind QUIC TLS 1.3 endpoint
        if let Ok((endpoint, bound_port)) = build_quic_dual_endpoint(DEFAULT_QUIC_PORT) {
            hub.quic_port.store(bound_port as u32, Ordering::Relaxed);
            if let Ok(mut ep_guard) = hub.quic_endpoint.write() {
                *ep_guard = Some(endpoint.clone());
            }

            // 2a. QUIC Server Acceptor Task
            let hub_accept = Arc::clone(&hub);
            let app_accept = app_handle.clone();
            let ep_accept = endpoint.clone();
            tauri::async_runtime::spawn(async move {
                while let Some(incoming) = ep_accept.accept().await {
                    let remote_ip = incoming.remote_address().ip();
                    if !is_lan_private_address(remote_ip) {
                        incoming.refuse();
                        continue;
                    }
                    let hub_conn = Arc::clone(&hub_accept);
                    let app_conn = app_accept.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Ok(conn) = incoming.await {
                            handle_incoming_quic_connection(hub_conn, app_conn, conn).await;
                        }
                    });
                }
            });

            // 2b. UDP LAN Discovery Beacon Broadcaster Task
            let hub_bcast = Arc::clone(&hub);
            tauri::async_runtime::spawn(async move {
                loop {
                    hub_bcast.emit_beacon_now();
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            });

            // 2c. UDP LAN Discovery Listener & Trusted Auto-Reconnect Task
            let hub_disc = Arc::clone(&hub);
            let app_disc = app_handle.clone();
            let ep_disc = endpoint.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(sock) = TokioUdpSocket::bind(("0.0.0.0", DISCOVERY_UDP_PORT)).await {
                    let mut buf = [0u8; 4096];
                    loop {
                        let Ok((len, src_addr)) = sock.recv_from(&mut buf).await else {
                            continue;
                        };
                        if !is_lan_private_address(src_addr.ip()) {
                            continue;
                        }
                        let Ok(beacon) = serde_json::from_slice::<LanDiscoveryBeacon>(&buf[..len])
                        else {
                            continue;
                        };
                        if beacon.magic != LanDiscoveryBeacon::MAGIC {
                            continue;
                        }

                        let now_ms = now_epoch_millis() as u64;
                        let mut should_notify = false;
                        let mut auto_reconnect_target: Option<SocketAddr> = None;

                        {
                            let Ok(mut guard) = hub_disc.state.write() else {
                                continue;
                            };
                            if beacon.node_id == guard.data.local_node_id {
                                continue;
                            }

                            // Prune stale unpaired peers older than 15s
                            let before_len = guard.discovered_peers.len();
                            guard
                                .discovered_peers
                                .retain(|p| now_ms.saturating_sub(p.last_seen_ms) < 15_000);
                            if guard.discovered_peers.len() != before_len {
                                should_notify = true;
                            }

                            // Check if already paired
                            if let Some(paired) = guard
                                .data
                                .nodes
                                .iter_mut()
                                .find(|n| n.id == beacon.node_id)
                            {
                                paired.ip = src_addr.ip().to_string();
                                paired.resolution = beacon.resolution;
                                let already_connected = hub_disc
                                    .sessions
                                    .read()
                                    .map(|s| s.contains_key(&beacon.node_id))
                                    .unwrap_or(false);
                                if paired.auto_pair
                                    && paired.fingerprint == beacon.fingerprint
                                    && !already_connected
                                {
                                    auto_reconnect_target =
                                        Some(SocketAddr::new(src_addr.ip(), beacon.quic_port));
                                }
                            } else {
                                // Update or insert into discovered_peers
                                if let Some(existing) = guard
                                    .discovered_peers
                                    .iter_mut()
                                    .find(|p| p.id == beacon.node_id)
                                {
                                    existing.ip = src_addr.ip().to_string();
                                    existing.quic_port = beacon.quic_port;
                                    existing.name = beacon.name;
                                    existing.resolution = beacon.resolution;
                                    existing.refresh_rate_hz = beacon.refresh_rate_hz;
                                    existing.last_seen_ms = now_ms;
                                } else {
                                    guard.discovered_peers.push(DiscoveredPeerDto {
                                        id: beacon.node_id,
                                        name: beacon.name,
                                        hostname: beacon.hostname,
                                        os: beacon.os,
                                        ip: src_addr.ip().to_string(),
                                        quic_port: beacon.quic_port,
                                        resolution: beacon.resolution,
                                        refresh_rate_hz: beacon.refresh_rate_hz,
                                        latency_ms: 0.5,
                                        fingerprint: beacon.fingerprint,
                                        last_seen_ms: now_ms,
                                    });
                                    should_notify = true;
                                }
                            }
                        }

                        if should_notify {
                            hub_disc.notify_frontend(&app_disc);
                        }

                        if let Some(target_sock) = auto_reconnect_target {
                            let hub_rc = Arc::clone(&hub_disc);
                            let app_rc = app_disc.clone();
                            let ep_rc = ep_disc.clone();
                            tauri::async_runtime::spawn(async move {
                                let local_node = hub_rc.local_node_descriptor();
                                if let Ok((sess, remote_node)) = client_reconnect_trusted_over_quic(
                                    &ep_rc,
                                    target_sock,
                                    local_node,
                                    &hub_rc.identity,
                                )
                                .await
                                {
                                    if let Ok(mut g) = hub_rc.state.write() {
                                        if let Some(n) = g
                                            .data
                                            .nodes
                                            .iter_mut()
                                            .find(|n| n.id == remote_node.id)
                                        {
                                            n.online = true;
                                            n.ip = target_sock.ip().to_string();
                                        }
                                    }
                                    register_and_spawn_peer_session(hub_rc.clone(), app_rc.clone(), sess);
                                    hub_rc.notify_frontend(&app_rc);
                                }
                            });
                        }
                    }
                }
            });
        }

        // 3. OS Clipboard Change Watcher Task (Plain Text + Lossless PNG Screenshots / Images)
        let hub_cb = Arc::clone(&hub);
        let app_cb = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            let mut linux_poll_parity = false;
            loop {
                tokio::time::sleep(Duration::from_millis(300)).await;
                let (daemon_active, allow_text, allow_img, last_hash, last_seq, local_name) = {
                    let Ok(guard) = hub_cb.state.read() else {
                        continue;
                    };
                    (
                        guard.data.tuning.daemon_active,
                        guard.data.clipboard_policy.sync_plain_text,
                        guard.data.clipboard_policy.sync_images,
                        guard.last_synced_clipboard_hash.clone(),
                        guard.last_os_clipboard_seq,
                        guard.data.tuning.local_device_name.clone(),
                    )
                };
                if !daemon_active || (!allow_text && !allow_img) {
                    continue;
                }
                let sessions: Vec<QuicPeerSession> = hub_cb
                    .sessions
                    .read()
                    .map(|s| s.values().cloned().collect())
                    .unwrap_or_default();
                if sessions.is_empty() {
                    continue;
                }

                // Zero-copy sequence number check on macOS (NSPasteboard.changeCount) & Windows (GetClipboardSequenceNumber)
                let cur_seq = os_clipboard_sequence_token();
                if let Some(seq) = cur_seq {
                    if last_seq == Some(seq) {
                        continue;
                    }
                } else {
                    linux_poll_parity = !linux_poll_parity;
                    if !linux_poll_parity {
                        continue;
                    }
                }

                let snap_res = tokio::task::spawn_blocking(move || {
                    read_os_clipboard_snapshot(allow_text, allow_img)
                })
                .await;

                // Only commit `last_os_clipboard_seq` if reading succeeded (so transient OpenClipboard locks retry next tick)
                let Ok(Ok(snapshot)) = snap_res else {
                    continue;
                };

                match snapshot {
                    ClipboardSnapshot::Empty => {
                        if let Ok(mut guard) = hub_cb.state.write() {
                            guard.last_os_clipboard_seq = cur_seq;
                        }
                    }

                    ClipboardSnapshot::Image {
                        width,
                        height,
                        png_bytes,
                        blake3_hex,
                    } => {
                        if png_bytes.is_empty() || png_bytes.len() > 45 * 1024 * 1024 {
                            if let Ok(mut guard) = hub_cb.state.write() {
                                guard.last_os_clipboard_seq = cur_seq;
                            }
                            continue;
                        }
                        if blake3_hex == last_hash {
                            if let Ok(mut guard) = hub_cb.state.write() {
                                guard.last_os_clipboard_seq = cur_seq;
                            }
                            continue;
                        }

                        {
                            let Ok(mut guard) = hub_cb.state.write() else {
                                continue;
                            };
                            guard.last_os_clipboard_seq = cur_seq;
                            if guard.last_synced_clipboard_hash == blake3_hex {
                                continue;
                            }
                            guard.last_synced_clipboard_hash = blake3_hex.clone();
                            guard.data.transfers.insert(
                                0,
                                TransferRecordDto {
                                    id: format!("tr-{}", now_epoch_millis()),
                                    record_type: "clipboard-image".into(),
                                    title: format!("剪贴板截图 ({width}×{height})"),
                                    size_label: format_bytes_human(png_bytes.len()),
                                    from_node_name: local_name,
                                    to_node_name: format!("自动同步 ({} 台在线)", sessions.len()),
                                    timestamp: now_hms_string(),
                                    encrypted: true,
                                    status: "completed".into(),
                                    digest_hex: Some(
                                        blake3_hex[0..16.min(blake3_hex.len())].to_string(),
                                    ),
                                },
                            );
                        }

                        let png_b64 = encode_base64(&png_bytes);
                        for sess in sessions {
                            let frame = ReliableControlFrame::ClipboardSync {
                                mime: "image/png".into(),
                                blake3_hex: blake3_hex.clone(),
                                text: None,
                                image: Some(ClipboardImageWire {
                                    width,
                                    height,
                                    png_b64: png_b64.clone(),
                                    rgba_b64: String::new(),
                                }),
                            };
                            tauri::async_runtime::spawn(async move {
                                let _ = sess.send_reliable_frame(&frame, false).await;
                            });
                        }
                        hub_cb.persist();
                        hub_cb.notify_frontend(&app_cb);
                    }

                    ClipboardSnapshot::Text { text, blake3_hex } => {
                        if text.len() > 4 * 1024 * 1024 {
                            if let Ok(mut guard) = hub_cb.state.write() {
                                guard.last_os_clipboard_seq = cur_seq;
                            }
                            continue;
                        }
                        if blake3_hex == last_hash {
                            if let Ok(mut guard) = hub_cb.state.write() {
                                guard.last_os_clipboard_seq = cur_seq;
                            }
                            continue;
                        }

                        {
                            let Ok(mut guard) = hub_cb.state.write() else {
                                continue;
                            };
                            guard.last_os_clipboard_seq = cur_seq;
                            if guard.last_synced_clipboard_hash == blake3_hex {
                                continue;
                            }
                            guard.last_synced_clipboard_hash = blake3_hex.clone();

                            let title = if text.chars().count() > 48 {
                                let p: String = text.chars().take(48).collect();
                                format!("{p}...")
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
                                    from_node_name: local_name,
                                    to_node_name: format!("自动同步 ({} 台在线)", sessions.len()),
                                    timestamp: now_hms_string(),
                                    encrypted: true,
                                    status: "completed".into(),
                                    digest_hex: Some(
                                        blake3_hex[0..16.min(blake3_hex.len())].to_string(),
                                    ),
                                },
                            );
                        }

                        for sess in sessions {
                            let frame = ReliableControlFrame::ClipboardSync {
                                mime: "text/plain;charset=utf-8".into(),
                                blake3_hex: blake3_hex.clone(),
                                text: Some(text.clone()),
                                image: None,
                            };
                            tauri::async_runtime::spawn(async move {
                                let _ = sess.send_reliable_frame(&frame, false).await;
                            });
                        }
                        hub_cb.persist();
                        hub_cb.notify_frontend(&app_cb);
                    }
                }
            }
        });

        // 4. Real-Time Captured Input Event Router Loop
        while let Some(ev) = cap_rx.recv().await {
            handle_captured_local_input(&hub, &app_handle, ev).await;
        }
    });
}

async fn handle_captured_local_input(
    hub: &Arc<DaemonHub>,
    app_handle: &AppHandle,
    ev: CapturedInputEvent,
) {
    match ev {
        CapturedInputEvent::EmergencyReturn => {
            hub.capture_ctrl.set_suppressing(false);
            if let Ok(mut guard) = hub.state.write() {
                let local_id = guard.data.local_node_id.clone();
                let outcome = guard.router.force_return_to_primary(&local_id);
                guard.cursor.active_node_id = outcome.active_node_id;
                guard.cursor.x = 0.5;
                guard.cursor.y = 0.5;
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
            hub.notify_frontend(app_handle);
        }

        CapturedInputEvent::EdgeCrossed {
            edge,
            norm_x,
            norm_y,
        } => {
            enum HandoffDecision {
                Proceed(QuicPeerSession, String),
                AlreadyRemote,
                AbortReleaseLocal,
            }

            let decision = {
                let Ok(mut guard) = hub.state.write() else {
                    hub.capture_ctrl.set_suppressing(false);
                    return;
                };
                if !guard.data.tuning.daemon_active {
                    HandoffDecision::AbortReleaseLocal
                } else {
                    let local_id = guard.data.local_node_id.clone();
                    if guard.cursor.active_node_id != local_id {
                        let still_live = hub
                            .sessions
                            .read()
                            .map(|s| s.contains_key(&guard.cursor.active_node_id))
                            .unwrap_or(false);
                        if still_live {
                            HandoffDecision::AlreadyRemote
                        } else {
                            guard.cursor.active_node_id = local_id.clone();
                            guard.router.active_node_id = local_id;
                            HandoffDecision::AbortReleaseLocal
                        }
                    } else {
                        let neighbors = compute_neighbors(&guard.data.nodes, &local_id);
                        let cand = match edge {
                            "up" => neighbors.up,
                            "down" => neighbors.down,
                            "left" => neighbors.left,
                            "right" => neighbors.right,
                            _ => None,
                        };
                        if let Some(target_node) = cand {
                            let target_id = target_node.id.clone();
                            let sess_opt = hub
                                .sessions
                                .read()
                                .ok()
                                .and_then(|s| s.get(&target_id).cloned());
                            if let Some(sess) = sess_opt {
                                guard.cursor.active_node_id = target_id.clone();
                                guard.cursor.x = norm_x;
                                guard.cursor.y = norm_y;
                                guard.cursor.transition_count += 1;
                                guard.cursor.last_transition_edge = Some(edge.to_string());
                                guard.router.active_node_id = target_id.clone();
                                guard.router.norm_x = norm_x;
                                guard.router.norm_y = norm_y;
                                HandoffDecision::Proceed(sess, target_id)
                            } else {
                                HandoffDecision::AbortReleaseLocal
                            }
                        } else {
                            HandoffDecision::AbortReleaseLocal
                        }
                    }
                }
            };

            match decision {
                HandoffDecision::Proceed(sess, target_id) => {
                    hub.capture_ctrl.set_suppressing(true);
                    let frame = ReliableControlFrame::FocusEnter {
                        node_id: target_id,
                        normalized_x: norm_x,
                        normalized_y: norm_y,
                    };
                    if sess.send_reliable_frame(&frame, true).await.is_err() {
                        hub.capture_ctrl.set_suppressing(false);
                        if let Ok(mut guard) = hub.state.write() {
                            let local_id = guard.data.local_node_id.clone();
                            guard.cursor.active_node_id = local_id.clone();
                            guard.router.active_node_id = local_id;
                        }
                    }
                    hub.notify_frontend(app_handle);
                }
                HandoffDecision::AlreadyRemote => {}
                HandoffDecision::AbortReleaseLocal => {
                    hub.capture_ctrl.set_suppressing(false);
                    hub.sync_capture_edges();
                }
            }
        }

        CapturedInputEvent::MouseDelta { dx, dy } => {
            enum DeltaAction {
                SendFast(QuicPeerSession, FastPointerDatagram),
                ReturnLocal {
                    prev_sess: Option<QuicPeerSession>,
                    release_keys: Vec<String>,
                    nx: f32,
                    ny: f32,
                },
                SwitchRemote {
                    prev_sess: Option<QuicPeerSession>,
                    release_keys: Vec<String>,
                    next_sess: QuicPeerSession,
                    next_id: String,
                    nx: f32,
                    ny: f32,
                },
                None,
            }

            let action = {
                let Ok(mut guard) = hub.state.write() else {
                    return;
                };
                let Ok(sessions) = hub.sessions.read() else {
                    return;
                };
                let local_id = guard.data.local_node_id.clone();

                let mut live_nodes = guard.data.nodes.clone();
                for n in &mut live_nodes {
                    n.online = n.id == local_id || sessions.contains_key(&n.id);
                }

                let outcome = guard
                    .router
                    .step_pointer_delta(&live_nodes, dx as f32, dy as f32);
                guard.cursor.active_node_id = outcome.active_node_id.clone();
                guard.cursor.x = outcome.norm_x;
                guard.cursor.y = outcome.norm_y;
                if let Some(edge) = outcome.crossed_edge {
                    guard.cursor.transition_count = guard.router.transition_count;
                    guard.cursor.last_transition_edge = Some(edge.to_string());
                }

                if outcome.active_node_id != outcome.previous_node_id {
                    let prev_sess = sessions.get(&outcome.previous_node_id).cloned();
                    if outcome.active_node_id == local_id {
                        DeltaAction::ReturnLocal {
                            prev_sess,
                            release_keys: outcome.synthetic_symbolic_keyups,
                            nx: outcome.norm_x,
                            ny: outcome.norm_y,
                        }
                    } else if let Some(next_sess) = sessions.get(&outcome.active_node_id).cloned() {
                        DeltaAction::SwitchRemote {
                            prev_sess,
                            release_keys: outcome.synthetic_symbolic_keyups,
                            next_sess,
                            next_id: outcome.active_node_id,
                            nx: outcome.norm_x,
                            ny: outcome.norm_y,
                        }
                    } else {
                        guard.cursor.active_node_id = local_id.clone();
                        guard.router.active_node_id = local_id;
                        DeltaAction::ReturnLocal {
                            prev_sess,
                            release_keys: outcome.synthetic_symbolic_keyups,
                            nx: 0.5,
                            ny: 0.5,
                        }
                    }
                } else if let Some(sess) = sessions.get(&outcome.active_node_id).cloned() {
                    let seq = hub.datagram_seq.fetch_add(1, Ordering::Relaxed);
                    let dgram = FastPointerDatagram {
                        frame_type: FastPointerDatagram::TYPE_RELATIVE_DELTA,
                        target_node: 0,
                        seq,
                        timestamp_us: (now_epoch_millis() as u32),
                        dx_subpx: (dx.clamp(-2000, 2000) * 16) as i16,
                        dy_subpx: (dy.clamp(-2000, 2000) * 16) as i16,
                    };
                    DeltaAction::SendFast(sess, dgram)
                } else {
                    DeltaAction::ReturnLocal {
                        prev_sess: None,
                        release_keys: Vec::new(),
                        nx: 0.5,
                        ny: 0.5,
                    }
                }
            };

            match action {
                DeltaAction::SendFast(sess, dgram) => {
                    let _ = sess.send_fast_pointer(&dgram);
                }
                DeltaAction::ReturnLocal {
                    prev_sess,
                    release_keys,
                    nx,
                    ny,
                } => {
                    hub.capture_ctrl.set_suppressing(false);
                    if let Ok(mut inj_guard) = hub.injector.lock() {
                        if let Some(ref mut inj) = *inj_guard {
                            let _ = inj.inject_mouse_warp(nx, ny);
                        }
                    }
                    if let Some(ps) = prev_sess {
                        for k in release_keys {
                            let _ = ps
                                .send_reliable_frame(
                                    &ReliableControlFrame::KeyTransition {
                                        scancode: 0,
                                        symbolic_key: k,
                                        pressed: false,
                                    },
                                    true,
                                )
                                .await;
                        }
                    }
                    hub.notify_frontend(app_handle);
                }
                DeltaAction::SwitchRemote {
                    prev_sess,
                    release_keys,
                    next_sess,
                    next_id,
                    nx,
                    ny,
                } => {
                    if let Some(ps) = prev_sess {
                        for k in release_keys {
                            let _ = ps
                                .send_reliable_frame(
                                    &ReliableControlFrame::KeyTransition {
                                        scancode: 0,
                                        symbolic_key: k,
                                        pressed: false,
                                    },
                                    true,
                                )
                                .await;
                        }
                    }
                    let _ = next_sess
                        .send_reliable_frame(
                            &ReliableControlFrame::FocusEnter {
                                node_id: next_id,
                                normalized_x: nx,
                                normalized_y: ny,
                            },
                            true,
                        )
                        .await;
                    hub.notify_frontend(app_handle);
                }
                DeltaAction::None => {}
            }
        }

        CapturedInputEvent::MouseButton { button, pressed } => {
            let sess_opt = {
                let Ok(guard) = hub.state.read() else {
                    return;
                };
                let active = &guard.cursor.active_node_id;
                hub.sessions.read().ok().and_then(|s| s.get(active).cloned())
            };
            if let Some(sess) = sess_opt {
                let _ = sess
                    .send_reliable_frame(
                        &ReliableControlFrame::MouseButton { button, pressed },
                        true,
                    )
                    .await;
            }
        }

        CapturedInputEvent::WheelHiRes {
            vertical_120,
            horizontal_120,
        } => {
            let sess_opt = {
                let Ok(guard) = hub.state.read() else {
                    return;
                };
                let active = &guard.cursor.active_node_id;
                hub.sessions.read().ok().and_then(|s| s.get(active).cloned())
            };
            if let Some(sess) = sess_opt {
                let seq = hub.datagram_seq.fetch_add(1, Ordering::Relaxed);
                let dgram = FastPointerDatagram {
                    frame_type: FastPointerDatagram::TYPE_HIRES_WHEEL,
                    target_node: 0,
                    seq,
                    timestamp_us: now_epoch_millis() as u32,
                    dx_subpx: vertical_120.clamp(-30000, 30000) as i16,
                    dy_subpx: horizontal_120.clamp(-30000, 30000) as i16,
                };
                let _ = sess.send_fast_pointer(&dgram);
            }
        }

        CapturedInputEvent::SymbolicKey {
            symbolic_key,
            scancode,
            pressed,
        } => {
            let target = {
                let Ok(mut guard) = hub.state.write() else {
                    return;
                };
                let active_id = guard.cursor.active_node_id.clone();
                let Some(target_node) = guard.data.nodes.iter().find(|n| n.id == active_id) else {
                    return;
                };
                let target_os = target_node.os;
                let mapped =
                    resolve_mapped_key(&guard.data.key_rules, &symbolic_key, target_os).to_string();
                guard.router.record_symbolic_key(&mapped, pressed);
                let sess_opt = hub
                    .sessions
                    .read()
                    .ok()
                    .and_then(|s| s.get(&active_id).cloned());
                sess_opt.map(|s| (s, mapped))
            };

            if let Some((sess, mapped_key)) = target {
                let _ = sess
                    .send_reliable_frame(
                        &ReliableControlFrame::KeyTransition {
                            scancode,
                            symbolic_key: mapped_key,
                            pressed,
                        },
                        true,
                    )
                    .await;
            }
        }
    }
}

async fn handle_incoming_quic_connection(
    hub: Arc<DaemonHub>,
    app_handle: AppHandle,
    conn: quinn::Connection,
) {
    let remote_ip: IpAddr = conn.remote_address().ip();
    let Ok((mut send, mut recv)) = conn.accept_bi().await else {
        return;
    };

    let Ok(req) = read_wire_json::<WireHandshakeRequest>(&mut recv).await else {
        return;
    };

    match req {
        WireHandshakeRequest::PairSpake2Init {
            mut initiator_node,
            spake2_msg_b64,
            auto_pair,
        } => {
            let (current_pin, node_count, local_node) = {
                let Ok(guard) = hub.state.read() else {
                    return;
                };
                (
                    format_pin_from_seed(guard.data.pin_seed),
                    guard.data.nodes.len(),
                    hub.local_node_descriptor(),
                )
            };

            if node_count >= MAX_DEVICES {
                let _ = write_wire_json(
                    &mut send,
                    &WireHandshakeResponse::Rejected {
                        reason: format!("对方已达 {MAX_DEVICES} 台设备上限"),
                    },
                )
                .await;
                let _ = send.finish();
                return;
            }

            let Ok(init_msg) = decode_base64(&spake2_msg_b64) else {
                return;
            };
            let clean_pin = current_pin.replace('-', "");
            let (resp_state, resp_msg) = Spake2::<Ed25519Group>::start_b(
                &Password::new(clean_pin.as_bytes()),
                &Identity::new(b"spankvm-initiator"),
                &Identity::new(b"spankvm-responder"),
            );
            let Ok(shared_secret) = resp_state.finish(&init_msg) else {
                let _ = write_wire_json(
                    &mut send,
                    &WireHandshakeResponse::Rejected {
                        reason: "SPAKE2 握手参数无效".into(),
                    },
                )
                .await;
                let _ = send.finish();
                return;
            };
            let (expected_init_tag, resp_tag) = derive_spake2_confirm_tags(&shared_secret);

            if write_wire_json(
                &mut send,
                &WireHandshakeResponse::PairSpake2Challenge {
                    responder_node: local_node.clone(),
                    spake2_msg_b64: spankvm_proto::encode_base64(&resp_msg),
                    responder_confirm_hex: resp_tag,
                },
            )
            .await
            .is_err()
            {
                return;
            }

            let Ok(WireHandshakeRequest::PairSpake2Confirm { confirm_hash_hex }) =
                read_wire_json::<WireHandshakeRequest>(&mut recv).await
            else {
                return;
            };

            if confirm_hash_hex != expected_init_tag {
                let _ = write_wire_json(
                    &mut send,
                    &WireHandshakeResponse::Rejected {
                        reason: "6 位临时配对码不正确".into(),
                    },
                )
                .await;
                let _ = send.finish();
                return;
            }

            let _ = write_wire_json(
                &mut send,
                &WireHandshakeResponse::Accepted {
                    responder_node: local_node,
                },
            )
            .await;
            let _ = send.finish();

            let Ok(session) = QuicPeerSession::new(initiator_node.id.clone(), remote_ip, conn)
            else {
                return;
            };

            {
                if let Ok(mut guard) = hub.state.write() {
                    let slot = find_next_adjacent_slot(&guard.data.nodes)
                        .unwrap_or(GridPos { col: 1, row: 2 });
                    initiator_node.is_primary = false;
                    initiator_node.auto_pair = auto_pair;
                    initiator_node.online = true;
                    initiator_node.ip = remote_ip.to_string();
                    initiator_node.paired_at = format!("今日 {}", &now_hms_string()[0..5]);

                    if let Some(existing) = guard
                        .data
                        .nodes
                        .iter_mut()
                        .find(|n| n.id == initiator_node.id)
                    {
                        existing.ip = initiator_node.ip.clone();
                        existing.online = true;
                        existing.auto_pair = auto_pair;
                        existing.fingerprint = initiator_node.fingerprint.clone();
                    } else {
                        initiator_node.grid_pos = slot;
                        guard.data.nodes.push(initiator_node);
                    }
                    // Rotate temporary PIN immediately after successful one-time pairing
                    guard.data.pin_seed = guard.data.pin_seed.wrapping_add(271_828);
                }
            }

            register_and_spawn_peer_session(hub.clone(), app_handle.clone(), session);
            hub.persist();
            hub.notify_frontend(&app_handle);
        }

        WireHandshakeRequest::TrustedReconnect {
            initiator_node,
            ed25519_pubkey_hex,
            timestamp_ms,
            signature_hex,
        } => {
            let derived_fp = LocalEd25519Identity::fingerprint_from_pubkey_hex(&ed25519_pubkey_hex)
                .unwrap_or_default();
            let valid_sig = LocalEd25519Identity::verify_reconnect(
                &ed25519_pubkey_hex,
                &initiator_node.id,
                timestamp_ms,
                &signature_hex,
            );

            let trusted = {
                let Ok(guard) = hub.state.read() else {
                    return;
                };
                guard.data.nodes.iter().any(|n| {
                    n.id == initiator_node.id
                        && n.auto_pair
                        && !derived_fp.is_empty()
                        && n.fingerprint == derived_fp
                })
            };

            if !valid_sig || !trusted {
                let _ = write_wire_json(
                    &mut send,
                    &WireHandshakeResponse::Rejected {
                        reason: "未在自动互信列表中或签名校验失败".into(),
                    },
                )
                .await;
                let _ = send.finish();
                return;
            }

            let local_node = hub.local_node_descriptor();
            let _ = write_wire_json(
                &mut send,
                &WireHandshakeResponse::Accepted {
                    responder_node: local_node,
                },
            )
            .await;
            let _ = send.finish();

            let Ok(session) = QuicPeerSession::new(initiator_node.id.clone(), remote_ip, conn)
            else {
                return;
            };

            if let Ok(mut guard) = hub.state.write() {
                if let Some(n) = guard
                    .data
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == initiator_node.id)
                {
                    n.online = true;
                    n.ip = remote_ip.to_string();
                }
            }

            register_and_spawn_peer_session(hub.clone(), app_handle.clone(), session);
            hub.notify_frontend(&app_handle);
        }

        _ => {}
    }
}

pub fn register_and_spawn_peer_session(
    hub: Arc<DaemonHub>,
    app_handle: AppHandle,
    session: QuicPeerSession,
) {
    let peer_id = session.peer_node_id.clone();
    if let Ok(mut sessions) = hub.sessions.write() {
        sessions.insert(peer_id.clone(), session.clone());
    }
    hub.sync_capture_edges();

    // Task A: Fast 14-byte Pointer Datagram Receiver
    let hub_fast = Arc::clone(&hub);
    let conn_fast = session.connection.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_seq: u32 = 0;
        while let Ok(bytes) = conn_fast.read_datagram().await {
            let Some(frame) = FastPointerDatagram::decode(&bytes) else {
                continue;
            };
            if frame.seq < last_seq && (last_seq - frame.seq) < 1_000_000 {
                continue;
            }
            last_seq = frame.seq;

            if let Ok(mut inj_guard) = hub_fast.injector.lock() {
                if let Some(ref mut inj) = *inj_guard {
                    match frame.frame_type {
                        FastPointerDatagram::TYPE_RELATIVE_DELTA => {
                            let dx = (frame.dx_subpx / 16) as i32;
                            let dy = (frame.dy_subpx / 16) as i32;
                            let _ = inj.inject_mouse_delta(dx, dy);
                        }
                        FastPointerDatagram::TYPE_HIRES_WHEEL => {
                            let _ = inj.inject_wheel(frame.dx_subpx as i32, frame.dy_subpx as i32);
                        }
                        FastPointerDatagram::TYPE_MOUSE_BUTTON => {
                            let _ = inj.inject_mouse_button(
                                frame.dx_subpx as u8,
                                frame.dy_subpx != 0,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    // Task B: Reliable Control Stream Receiver + Disconnect Cleanup
    let hub_rel = Arc::clone(&hub);
    let app_rel = app_handle.clone();
    let conn_rel = session.connection.clone();
    let file_buffers: Arc<Mutex<HashMap<String, PendingFileBuffer>>> =
        Arc::new(Mutex::new(HashMap::new()));

    tauri::async_runtime::spawn(async move {
        while let Ok(mut recv) = conn_rel.accept_uni().await {
            let Ok(frame) = read_wire_json::<ReliableControlFrame>(&mut recv).await else {
                continue;
            };
            process_incoming_reliable_frame(&hub_rel, &app_rel, &peer_id, &file_buffers, frame);
        }

        // Peer disconnected: clean up session and reclaim focus if it was on that peer
        if let Ok(mut sessions) = hub_rel.sessions.write() {
            sessions.remove(&peer_id);
        }
        if let Ok(mut guard) = hub_rel.state.write() {
            for n in &mut guard.data.nodes {
                if n.id == peer_id {
                    n.online = false;
                }
            }
            if guard.cursor.active_node_id == peer_id {
                let local_id = guard.data.local_node_id.clone();
                guard.cursor.active_node_id = local_id.clone();
                guard.cursor.x = 0.5;
                guard.cursor.y = 0.5;
                guard.router.active_node_id = local_id;
                hub_rel.capture_ctrl.set_suppressing(false);
            }
        }
        hub_rel.notify_frontend(&app_rel);
    });
}

fn process_incoming_reliable_frame(
    hub: &Arc<DaemonHub>,
    app_handle: &AppHandle,
    peer_id: &str,
    file_buffers: &Arc<Mutex<HashMap<String, PendingFileBuffer>>>,
    frame: ReliableControlFrame,
) {
    match frame {
        ReliableControlFrame::KeyTransition {
            symbolic_key,
            pressed,
            ..
        } => {
            if let Ok(mut inj_guard) = hub.injector.lock() {
                if let Some(ref mut inj) = *inj_guard {
                    let _ = inj.inject_symbolic_key(&symbolic_key, pressed);
                }
            }
        }

        ReliableControlFrame::MouseButton { button, pressed } => {
            if let Ok(mut inj_guard) = hub.injector.lock() {
                if let Some(ref mut inj) = *inj_guard {
                    let _ = inj.inject_mouse_button(button, pressed);
                }
            }
        }

        ReliableControlFrame::WheelScroll {
            vertical_120,
            horizontal_120,
        } => {
            if let Ok(mut inj_guard) = hub.injector.lock() {
                if let Some(ref mut inj) = *inj_guard {
                    let _ = inj.inject_wheel(vertical_120, horizontal_120);
                }
            }
        }

        ReliableControlFrame::FocusEnter {
            normalized_x,
            normalized_y,
            ..
        }
        | ReliableControlFrame::FocusReturnPrimary {
            normalized_x,
            normalized_y,
            ..
        } => {
            hub.capture_ctrl.set_suppressing(false);
            if let Ok(mut guard) = hub.state.write() {
                let local_id = guard.data.local_node_id.clone();
                guard.cursor.active_node_id = local_id.clone();
                guard.cursor.x = normalized_x;
                guard.cursor.y = normalized_y;
                guard.router.active_node_id = local_id;
                guard.router.norm_x = normalized_x;
                guard.router.norm_y = normalized_y;
            }
            if let Ok(mut inj_guard) = hub.injector.lock() {
                if let Some(ref mut inj) = *inj_guard {
                    let _ = inj.inject_mouse_warp(normalized_x, normalized_y);
                }
            }
            hub.notify_frontend(app_handle);
        }

        ReliableControlFrame::ReleaseAllHeldKeys => {
            if let Ok(mut inj_guard) = hub.injector.lock() {
                if let Some(ref mut inj) = *inj_guard {
                    for modifier in [
                        "Command (⌘)",
                        "Ctrl",
                        "Option (⌥)",
                        "Alt",
                        "Shift",
                        "Win / Super",
                    ] {
                        let _ = inj.inject_symbolic_key(modifier, false);
                    }
                }
            }
        }

        ReliableControlFrame::ClipboardSync {
            blake3_hex,
            text,
            image,
            ..
        } => {
            let (allow_text, allow_img, peer_name, local_name) = {
                let Ok(guard) = hub.state.read() else {
                    return;
                };
                let pname = guard
                    .data
                    .nodes
                    .iter()
                    .find(|n| n.id == peer_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "远端设备".into());
                (
                    guard.data.clipboard_policy.sync_plain_text,
                    guard.data.clipboard_policy.sync_images,
                    pname,
                    guard.data.tuning.local_device_name.clone(),
                )
            };

            if let Some(txt) = text {
                if allow_text && compute_chunk_blake3_hex(txt.as_bytes()) == blake3_hex {
                    if write_os_clipboard_text(&txt).is_ok() {
                        let new_seq = os_clipboard_sequence_token();
                        if let Ok(mut guard) = hub.state.write() {
                            guard.last_synced_clipboard_hash = blake3_hex.clone();
                            guard.last_os_clipboard_seq = new_seq;
                            let title = if txt.chars().count() > 48 {
                                let p: String = txt.chars().take(48).collect();
                                format!("{p}...")
                            } else {
                                txt.clone()
                            };
                            guard.data.transfers.insert(
                                0,
                                TransferRecordDto {
                                    id: format!("tr-{}", now_epoch_millis()),
                                    record_type: "clipboard-text".into(),
                                    title,
                                    size_label: format_bytes_human(txt.len()),
                                    from_node_name: peer_name,
                                    to_node_name: local_name,
                                    timestamp: now_hms_string(),
                                    encrypted: true,
                                    status: "completed".into(),
                                    digest_hex: Some(
                                        blake3_hex[0..16.min(blake3_hex.len())].to_string(),
                                    ),
                                },
                            );
                        }
                        hub.persist();
                        hub.notify_frontend(app_handle);
                    }
                }
            } else if let Some(img) = image {
                if allow_img {
                    let png_decoded = if !img.png_b64.is_empty() {
                        decode_base64(&img.png_b64).ok()
                    } else {
                        None
                    };
                    let rgba_decoded = if png_decoded.is_none() && !img.rgba_b64.is_empty() {
                        decode_base64(&img.rgba_b64).ok()
                    } else {
                        None
                    };

                    if let Ok((w, h, canonical_hash, byte_len)) = write_os_clipboard_png_or_rgba(
                        img.width,
                        img.height,
                        png_decoded.as_deref(),
                        rgba_decoded.as_deref(),
                    ) {
                        let new_seq = os_clipboard_sequence_token();
                        if let Ok(mut guard) = hub.state.write() {
                            guard.last_synced_clipboard_hash = canonical_hash.clone();
                            guard.last_os_clipboard_seq = new_seq;
                            guard.data.transfers.insert(
                                0,
                                TransferRecordDto {
                                    id: format!("tr-{}", now_epoch_millis()),
                                    record_type: "clipboard-image".into(),
                                    title: format!("剪贴板截图 ({w}×{h})"),
                                    size_label: format_bytes_human(byte_len),
                                    from_node_name: peer_name,
                                    to_node_name: local_name,
                                    timestamp: now_hms_string(),
                                    encrypted: true,
                                    status: "completed".into(),
                                    digest_hex: Some(
                                        canonical_hash[0..16.min(canonical_hash.len())].to_string(),
                                    ),
                                },
                            );
                        }
                        hub.persist();
                        hub.notify_frontend(app_handle);
                    }
                }
            }
        }

        ReliableControlFrame::FileChunk {
            transfer_id,
            file_name,
            chunk_index,
            total_chunks,
            chunk_blake3_hex,
            bytes_b64,
            ..
        } => {
            let Ok(chunk_bytes) = decode_base64(&bytes_b64) else {
                return;
            };
            if compute_chunk_blake3_hex(&chunk_bytes) != chunk_blake3_hex {
                return;
            }

            let mut completed_file: Option<(String, Vec<u8>)> = None;
            if let Ok(mut map) = file_buffers.lock() {
                let entry = map.entry(transfer_id.clone()).or_insert_with(|| PendingFileBuffer {
                    file_name: file_name.clone(),
                    total_chunks,
                    received_chunks: HashMap::new(),
                });
                entry.received_chunks.insert(chunk_index, chunk_bytes);

                if entry.received_chunks.len() as u32 >= entry.total_chunks {
                    let mut full = Vec::new();
                    for idx in 0..entry.total_chunks {
                        if let Some(part) = entry.received_chunks.remove(&idx) {
                            full.extend_from_slice(&part);
                        }
                    }
                    let fname = entry.file_name.clone();
                    map.remove(&transfer_id);
                    completed_file = Some((fname, full));
                }
            }

            if let Some((fname, full_bytes)) = completed_file {
                let (enabled, save_dir, peer_name, local_name) = {
                    let Ok(guard) = hub.state.read() else {
                        return;
                    };
                    let pname = guard
                        .data
                        .nodes
                        .iter()
                        .find(|n| n.id == peer_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| "远端设备".into());
                    (
                        guard.data.clipboard_policy.enable_drag_drop_file,
                        guard.data.clipboard_policy.save_dir.clone(),
                        pname,
                        guard.data.tuning.local_device_name.clone(),
                    )
                };
                if !enabled {
                    return;
                }

                let safe_name = PathBuf::from(&fname)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "received.bin".into());
                let out_dir = expand_save_dir(&save_dir);
                let _ = fs::create_dir_all(&out_dir);
                let out_path = out_dir.join(&safe_name);
                if fs::write(&out_path, &full_bytes).is_ok() {
                    let digest = compute_chunk_blake3_hex(&full_bytes);
                    if let Ok(mut guard) = hub.state.write() {
                        guard.data.transfers.insert(
                            0,
                            TransferRecordDto {
                                id: format!("tr-{}", now_epoch_millis()),
                                record_type: "file-drop".into(),
                                title: safe_name,
                                size_label: format_bytes_human(full_bytes.len()),
                                from_node_name: peer_name,
                                to_node_name: local_name,
                                timestamp: now_hms_string(),
                                encrypted: true,
                                status: "completed".into(),
                                digest_hex: Some(digest[0..16].to_string()),
                            },
                        );
                    }
                    hub.persist();
                    hub.notify_frontend(app_handle);
                }
            }
        }

        ReliableControlFrame::TopologySync { nodes } => {
            if let Ok(mut guard) = hub.state.write() {
                for incoming in nodes {
                    if let Some(local_rec) =
                        guard.data.nodes.iter_mut().find(|n| n.id == incoming.id)
                    {
                        local_rec.grid_pos = incoming.grid_pos;
                        local_rec.is_primary = incoming.is_primary;
                    }
                }
            }
            hub.persist();
            hub.notify_frontend(app_handle);
        }
    }
}
