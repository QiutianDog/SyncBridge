//! Linux Wayland & X11 dual-stack input backend (Confirmed Option B).
//!
//! 1. **Wayland Portal (`ashpd` + `libei`)**: Negotiates `InputCapture` / `RemoteDesktop` Portal.
//! 2. **Kernel `/dev/uinput` Virtual HID**: Creates a real `SpanKVM Virtual HID` device via `evdev`
//!    supporting relative pointer, buttons, high-res wheel, and full keyboard scancode injection.
//! 3. **X11 (`x11rb` `xtest` + pointer polling)**: Injects via XTest extension and monitors X11 root
//!    screen edges when running under X11 or XWayland.

use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use spankvm_proto::LinuxWaylandStrategy;
use tracing::{info, warn};

use crate::{
    CaptureControlState, CapturedInputEvent, InputCaptureBackend, InputInjectorBackend, PalError,
    RawInputEvent,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveLinuxBackend {
    WaylandLibeiPortal,
    KernelUinputFallback,
    X11Xi2Xtest,
}

pub fn is_linux_input_permitted() -> bool {
    #[cfg(target_os = "linux")]
    {
        let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        if !is_wayland {
            return std::env::var_os("DISPLAY").is_some();
        }
        std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/uinput")
            .is_ok()
            || std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

pub fn probe_linux_display() -> (u32, u32, u16) {
    #[cfg(target_os = "linux")]
    {
        use x11rb::connection::Connection;
        if let Ok((conn, screen_num)) = x11rb::connect(None) {
            if let Some(screen) = conn.setup().roots.get(screen_num) {
                let w = (screen.width_in_pixels as u32).max(1280);
                let h = (screen.height_in_pixels as u32).max(720);
                return (w, h, 120);
            }
        }
    }
    (1920, 1080, 120)
}

pub struct LinuxDualStackDriver {
    pub backend: ActiveLinuxBackend,
    pub suppressing_local: bool,
    pub screen_edges_armed: [bool; 4],
    #[cfg(target_os = "linux")]
    uinput_dev: Option<evdev::uinput::VirtualDevice>,
    #[cfg(target_os = "linux")]
    x11_conn: Option<(x11rb::rust_connection::RustConnection, usize)>,
}

impl LinuxDualStackDriver {
    pub fn initialize_sync(strategy: LinuxWaylandStrategy) -> Result<Self, PalError> {
        let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();

        #[cfg(target_os = "linux")]
        {
            let uinput_dev = Self::try_build_uinput_virtual_hid().ok();
            let x11_conn = x11rb::connect(None).ok();

            let backend = if is_wayland {
                if uinput_dev.is_some() && strategy == LinuxWaylandStrategy::PortalAndUinputFallback
                {
                    ActiveLinuxBackend::KernelUinputFallback
                } else {
                    ActiveLinuxBackend::WaylandLibeiPortal
                }
            } else {
                ActiveLinuxBackend::X11Xi2Xtest
            };

            return Ok(Self {
                backend,
                suppressing_local: false,
                screen_edges_armed: [false; 4],
                uinput_dev,
                x11_conn,
            });
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (is_wayland, strategy);
            Ok(Self {
                backend: ActiveLinuxBackend::X11Xi2Xtest,
                suppressing_local: false,
                screen_edges_armed: [false; 4],
            })
        }
    }

    pub async fn initialize(
        strategy: LinuxWaylandStrategy,
        screen_edges_armed: [bool; 4],
    ) -> Result<Self, PalError> {
        let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let mut drv = Self::initialize_sync(strategy)?;
        drv.screen_edges_armed = screen_edges_armed;

        if is_wayland {
            match Self::try_connect_wayland_eis_portal().await {
                Ok(()) => {
                    info!("Connected to Wayland XDG InputCapture Portal (libei)");
                    drv.backend = ActiveLinuxBackend::WaylandLibeiPortal;
                }
                Err(portal_err) => {
                    if strategy == LinuxWaylandStrategy::PortalOnly {
                        return Err(portal_err);
                    }
                    warn!(
                        "Wayland InputCapture Portal unavailable ({}); using /dev/uinput + X11 fallback",
                        portal_err
                    );
                }
            }
        }
        Ok(drv)
    }

    #[cfg(target_os = "linux")]
    fn try_build_uinput_virtual_hid() -> Result<evdev::uinput::VirtualDevice, PalError> {
        use crate::keymap::KEY_TABLE;
        use evdev::{uinput::VirtualDeviceBuilder, AttributeSet, Key, RelativeAxisType};

        let mut keys = AttributeSet::<Key>::new();
        keys.insert(Key::BTN_LEFT);
        keys.insert(Key::BTN_RIGHT);
        keys.insert(Key::BTN_MIDDLE);
        for entry in KEY_TABLE {
            keys.insert(Key::new(entry.linux_evdev));
        }

        let mut axes = AttributeSet::<RelativeAxisType>::new();
        axes.insert(RelativeAxisType::REL_X);
        axes.insert(RelativeAxisType::REL_Y);
        axes.insert(RelativeAxisType::REL_WHEEL);
        axes.insert(RelativeAxisType::REL_HWHEEL);
        axes.insert(RelativeAxisType::REL_WHEEL_HI_RES);
        axes.insert(RelativeAxisType::REL_HWHEEL_HI_RES);

        VirtualDeviceBuilder::new()
            .map_err(|e| PalError::PermissionDenied(format!("/dev/uinput open failed: {e}")))?
            .name("SpanKVM Virtual HID")
            .with_keys(&keys)
            .map_err(|e| PalError::DriverInitFailed(format!("uinput keys failed: {e}")))?
            .with_relative_axes(&axes)
            .map_err(|e| PalError::DriverInitFailed(format!("uinput axes failed: {e}")))?
            .build()
            .map_err(|e| PalError::DriverInitFailed(format!("uinput build failed: {e}")))
    }

    #[cfg(target_os = "linux")]
    async fn try_connect_wayland_eis_portal() -> Result<(), PalError> {
        use ashpd::desktop::input_capture::{Capabilities, InputCapture};
        let proxy = InputCapture::new()
            .await
            .map_err(|e| PalError::DriverInitFailed(format!("DBus InputCapture portal: {e}")))?;
        let (_session, _caps) = proxy
            .create_session(
                None::<&ashpd::WindowIdentifier>,
                Capabilities::Keyboard | Capabilities::Pointer,
            )
            .await
            .map_err(|e| PalError::PermissionDenied(format!("Wayland InputCapture 授权未通过: {e}")))?;
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    async fn try_connect_wayland_eis_portal() -> Result<(), PalError> {
        Ok(())
    }

    pub fn inject_mouse_button(&mut self, button: u8, pressed: bool) -> Result<(), PalError> {
        #[cfg(target_os = "linux")]
        {
            if let Some(ref mut uinput) = self.uinput_dev {
                use evdev::{EventType, InputEvent, Key};
                let btn_key = match button {
                    1 => Key::BTN_LEFT,
                    2 => Key::BTN_RIGHT,
                    _ => Key::BTN_MIDDLE,
                };
                let ev = InputEvent::new(EventType::KEY, btn_key.code(), if pressed { 1 } else { 0 });
                let _ = uinput.emit(&[ev]);
                return Ok(());
            }
            if let Some((ref conn, _)) = self.x11_conn {
                use x11rb::connection::Connection;
                use x11rb::protocol::xproto::{BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT};
                use x11rb::protocol::xtest::fake_input;
                let detail = match button {
                    1 => 1,
                    2 => 3,
                    _ => 2,
                };
                let ev_type = if pressed {
                    BUTTON_PRESS_EVENT
                } else {
                    BUTTON_RELEASE_EVENT
                };
                let _ = fake_input(conn, ev_type, detail, 0, x11rb::NONE, 0, 0, 0);
                let _ = conn.flush();
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (button, pressed);
        }
        Ok(())
    }

    pub fn inject_symbolic_key(&mut self, symbolic_key: &str, pressed: bool) -> Result<(), PalError> {
        #[cfg(target_os = "linux")]
        {
            if let Some(entry) = crate::keymap::lookup_by_canonical(symbolic_key) {
                if let Some(ref mut uinput) = self.uinput_dev {
                    use evdev::{EventType, InputEvent};
                    let ev = InputEvent::new(
                        EventType::KEY,
                        entry.linux_evdev,
                        if pressed { 1 } else { 0 },
                    );
                    let _ = uinput.emit(&[ev]);
                    return Ok(());
                }
                if let Some((ref conn, _)) = self.x11_conn {
                    use x11rb::connection::Connection;
                    use x11rb::protocol::xproto::{KEY_PRESS_EVENT, KEY_RELEASE_EVENT};
                    use x11rb::protocol::xtest::fake_input;
                    let ev_type = if pressed {
                        KEY_PRESS_EVENT
                    } else {
                        KEY_RELEASE_EVENT
                    };
                    let x11_keycode = (entry.linux_evdev.saturating_add(8)).min(255) as u8;
                    let _ = fake_input(conn, ev_type, x11_keycode, 0, x11rb::NONE, 0, 0, 0);
                    let _ = conn.flush();
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (symbolic_key, pressed);
        }
        Ok(())
    }
}

impl InputCaptureBackend for LinuxDualStackDriver {
    fn set_suppress_local_input(&mut self, suppress: bool) -> Result<(), PalError> {
        self.suppressing_local = suppress;
        Ok(())
    }

    fn poll_raw_events(&mut self, _out: &mut Vec<RawInputEvent>) -> Result<(), PalError> {
        Ok(())
    }
}

impl InputInjectorBackend for LinuxDualStackDriver {
    fn inject_mouse_delta(&mut self, dx: i32, dy: i32) -> Result<(), PalError> {
        #[cfg(target_os = "linux")]
        {
            if let Some(ref mut uinput) = self.uinput_dev {
                use evdev::{EventType, InputEvent, RelativeAxisType};
                let events = [
                    InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_X.0, dx),
                    InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_Y.0, dy),
                ];
                let _ = uinput.emit(&events);
                return Ok(());
            }
            if let Some((ref conn, _)) = self.x11_conn {
                use x11rb::connection::Connection;
                use x11rb::protocol::xproto::MOTION_NOTIFY_EVENT;
                use x11rb::protocol::xtest::fake_input;
                let _ = fake_input(
                    conn,
                    MOTION_NOTIFY_EVENT,
                    1, // relative motion
                    0,
                    x11rb::NONE,
                    dx.clamp(-32767, 32767) as i16,
                    dy.clamp(-32767, 32767) as i16,
                    0,
                );
                let _ = conn.flush();
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (dx, dy);
        }
        Ok(())
    }

    fn inject_mouse_warp_normalized(&mut self, norm_x: f32, norm_y: f32) -> Result<(), PalError> {
        #[cfg(target_os = "linux")]
        {
            if let Some((ref conn, screen_num)) = self.x11_conn {
                use x11rb::connection::Connection;
                use x11rb::protocol::xproto::ConnectionExt;
                if let Some(screen) = conn.setup().roots.get(screen_num) {
                    let px = (norm_x.clamp(0.0, 1.0) * (screen.width_in_pixels as f32)) as i16;
                    let py = (norm_y.clamp(0.0, 1.0) * (screen.height_in_pixels as f32)) as i16;
                    let _ = conn.warp_pointer(x11rb::NONE, screen.root, 0, 0, 0, 0, px, py);
                    let _ = conn.flush();
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (norm_x, norm_y);
        }
        Ok(())
    }

    fn inject_wheel_hires(&mut self, vertical_120: i32, horizontal_120: i32) -> Result<(), PalError> {
        #[cfg(target_os = "linux")]
        {
            if let Some(ref mut uinput) = self.uinput_dev {
                use evdev::{EventType, InputEvent, RelativeAxisType};
                let events = [
                    InputEvent::new(
                        EventType::RELATIVE,
                        RelativeAxisType::REL_WHEEL_HI_RES.0,
                        vertical_120,
                    ),
                    InputEvent::new(
                        EventType::RELATIVE,
                        RelativeAxisType::REL_HWHEEL_HI_RES.0,
                        horizontal_120,
                    ),
                ];
                let _ = uinput.emit(&events);
                return Ok(());
            }
            if let Some((ref conn, _)) = self.x11_conn {
                use x11rb::connection::Connection;
                use x11rb::protocol::xproto::{BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT};
                use x11rb::protocol::xtest::fake_input;
                if vertical_120 != 0 {
                    let btn = if vertical_120 > 0 { 4 } else { 5 };
                    let _ = fake_input(conn, BUTTON_PRESS_EVENT, btn, 0, x11rb::NONE, 0, 0, 0);
                    let _ = fake_input(conn, BUTTON_RELEASE_EVENT, btn, 0, x11rb::NONE, 0, 0, 0);
                    let _ = conn.flush();
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (vertical_120, horizontal_120);
        }
        Ok(())
    }

    fn inject_key_scancode(&mut self, scancode: u16, pressed: bool) -> Result<(), PalError> {
        if let Some(entry) = crate::keymap::canonical_from_linux_evdev(scancode) {
            return self.inject_symbolic_key(entry.canonical, pressed);
        }
        Ok(())
    }
}

pub fn spawn_linux_capture_thread(
    ctrl: Arc<CaptureControlState>,
    tx: UnboundedSender<CapturedInputEvent>,
) {
    #[cfg(target_os = "linux")]
    {
        std::thread::Builder::new()
            .name("spankvm-linux-edge-monitor".into())
            .spawn(move || {
                let Ok((conn, screen_num)) = x11rb::connect(None) else {
                    return;
                };
                let Some(root) = conn.setup().roots.get(screen_num).map(|s| s.root) else {
                    return;
                };
                let (w, h) = conn
                    .setup()
                    .roots
                    .get(screen_num)
                    .map(|s| (s.width_in_pixels as i32, s.height_in_pixels as i32))
                    .unwrap_or((1920, 1080));

                use x11rb::connection::Connection;
                use x11rb::protocol::xproto::{ConnectionExt, EventMask, GrabMode};

                let mut prev_pos: Option<(i16, i16)> = None;
                let mut x11_grabbed = false;
                let mut warp_settling = false;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(4));
                    if !ctrl.is_daemon_enabled() {
                        if x11_grabbed {
                            let _ = conn.ungrab_pointer(x11rb::CURRENT_TIME);
                            let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
                            let _ = conn.flush();
                            x11_grabbed = false;
                        }
                        prev_pos = None;
                        warp_settling = false;
                        continue;
                    }

                    let suppressing = ctrl.is_suppressing();
                    if suppressing && !x11_grabbed {
                        let _ = conn.grab_pointer(
                            false,
                            root,
                            EventMask::BUTTON_PRESS
                                | EventMask::BUTTON_RELEASE
                                | EventMask::POINTER_MOTION,
                            GrabMode::ASYNC,
                            GrabMode::ASYNC,
                            root,
                            x11rb::NONE,
                            x11rb::CURRENT_TIME,
                        );
                        let _ = conn.grab_keyboard(
                            false,
                            root,
                            x11rb::CURRENT_TIME,
                            GrabMode::ASYNC,
                            GrabMode::ASYNC,
                        );
                        let _ = conn.flush();
                        x11_grabbed = true;
                    } else if !suppressing && x11_grabbed {
                        let _ = conn.ungrab_pointer(x11rb::CURRENT_TIME);
                        let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
                        let _ = conn.flush();
                        x11_grabbed = false;
                        warp_settling = false;
                    }

                    let Ok(cookie) = conn.query_pointer(root) else {
                        continue;
                    };
                    let Ok(reply) = cookie.reply() else {
                        continue;
                    };
                    let (cx, cy) = (reply.root_x, reply.root_y);
                    let (px, py) = prev_pos.unwrap_or((cx, cy));
                    prev_pos = Some((cx, cy));

                    let dx = (cx - px) as i32;
                    let dy = (cy - py) as i32;

                    if !suppressing {
                        warp_settling = false;
                        if !ctrl.is_in_edge_cooldown() {
                            let armed = ctrl.armed_edges();
                            let nx = (cx as f32 / w.max(1) as f32).clamp(0.0, 1.0);
                            let ny = (cy as f32 / h.max(1) as f32).clamp(0.0, 1.0);

                            let crossed = if cx <= 1 && dx < 0 && armed[2] {
                                Some(("left", 0.96f32, ny))
                            } else if cx as i32 >= w - 2 && dx > 0 && armed[3] {
                                Some(("right", 0.04f32, ny))
                            } else if cy <= 1 && dy < 0 && armed[0] {
                                Some(("up", nx, 0.96f32))
                            } else if cy as i32 >= h - 2 && dy > 0 && armed[1] {
                                Some(("down", nx, 0.04f32))
                            } else {
                                None
                            };

                            if let Some((edge, target_nx, target_ny)) = crossed {
                                ctrl.set_suppressing_from_hook(true);
                                warp_settling = true;
                                let center_x = (w / 2) as i16;
                                let center_y = (h / 2) as i16;
                                let _ = conn.warp_pointer(
                                    x11rb::NONE,
                                    root,
                                    0,
                                    0,
                                    0,
                                    0,
                                    center_x,
                                    center_y,
                                );
                                let _ = conn.flush();
                                prev_pos = Some((center_x, center_y));
                                let _ = tx.send(CapturedInputEvent::EdgeCrossed {
                                    edge,
                                    norm_x: target_nx,
                                    norm_y: target_ny,
                                });
                            }
                        }
                    } else {
                        let center_x = (w / 2) as i16;
                        let center_y = (h / 2) as i16;
                        if warp_settling {
                            if (cx - center_x).abs() <= 160 && (cy - center_y).abs() <= 160 {
                                warp_settling = false;
                                prev_pos = Some((center_x, center_y));
                            }
                            continue;
                        }
                        if dx != 0 || dy != 0 {
                            let _ = tx.send(CapturedInputEvent::MouseDelta { dx, dy });
                            let _ = conn.warp_pointer(
                                x11rb::NONE,
                                root,
                                0,
                                0,
                                0,
                                0,
                                center_x,
                                center_y,
                            );
                            let _ = conn.flush();
                            prev_pos = Some((center_x, center_y));
                        }
                    }
                }
            })
            .ok();
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (ctrl, tx);
    }
}
