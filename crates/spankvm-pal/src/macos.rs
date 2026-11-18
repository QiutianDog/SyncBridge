//! macOS CoreGraphics (`CGEventTap` + `CGEventPost`) low-latency capture & injection driver.
//!
//! - Checks/prompts macOS Accessibility permission via `AXIsProcessTrusted` / `AXIsProcessTrustedWithOptions`.
//! - Intercepts raw mouse deltas, buttons, smooth scroll wheel, and keyboard/modifier events via `kCGHIDEventTap`.
//! - Disconnects local cursor movement via `CGAssociateMouseAndMouseCursorPosition(false)` and suppresses local
//!   consumption when focus crosses onto a remote computer, while always honoring `Ctrl+Alt+Shift+Esc`.
//! - Injects incoming remote events tagged with `SPANKVM_INJECT_MAGIC` so `CGEventTap` never re-captures them.

use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    CaptureControlState, CapturedInputEvent, InputCaptureBackend, InputInjectorBackend, PalError,
    RawInputEvent,
};
#[cfg(target_os = "macos")]
use crate::SPANKVM_INJECT_MAGIC;

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

pub fn is_accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { AXIsProcessTrusted() != 0 }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn request_accessibility_permission() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

pub fn probe_macos_display() -> (u32, u32, u16) {
    #[cfg(target_os = "macos")]
    {
        use core_graphics::display::CGDisplay;
        let main = CGDisplay::main();
        let w = (main.pixels_wide() as u32).max(1280);
        let h = (main.pixels_high() as u32).max(720);
        let hz = main
            .display_mode()
            .map(|m| m.refresh_rate().round() as u16)
            .filter(|&r| r >= 30)
            .unwrap_or(120);
        (w, h, hz)
    }
    #[cfg(not(target_os = "macos"))]
    {
        (2560, 1440, 120)
    }
}

pub fn set_local_cursor_associated(associated: bool) {
    #[cfg(target_os = "macos")]
    {
        use core_graphics::display::CGDisplay;
        let _ = CGDisplay::associate_mouse_and_mouse_cursor_position(associated);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = associated;
    }
}

pub struct MacOsCoreGraphicsDriver {
    pub suppressing_local: bool,
    pub cursor_pos: (f64, f64),
    pub screen_size: (f64, f64),
    pub left_button_down: bool,
    pub right_button_down: bool,
}

impl MacOsCoreGraphicsDriver {
    pub fn initialize() -> Result<Self, PalError> {
        let (w, h, _) = probe_macos_display();
        Ok(Self {
            suppressing_local: false,
            cursor_pos: (w as f64 * 0.5, h as f64 * 0.5),
            screen_size: (w as f64, h as f64),
            left_button_down: false,
            right_button_down: false,
        })
    }

    pub fn inject_mouse_button(&mut self, button: u8, pressed: bool) -> Result<(), PalError> {
        #[cfg(target_os = "macos")]
        {
            use core_graphics::event::{
                CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, EventField,
            };
            use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
            use core_graphics::geometry::CGPoint;

            let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                .map_err(|_| PalError::InjectionFailed("CGEventSource failed".into()))?;
            let pt = CGPoint::new(self.cursor_pos.0, self.cursor_pos.1);

            let (etype, cg_btn) = match (button, pressed) {
                (1, true) => {
                    self.left_button_down = true;
                    (CGEventType::LeftMouseDown, CGMouseButton::Left)
                }
                (1, false) => {
                    self.left_button_down = false;
                    (CGEventType::LeftMouseUp, CGMouseButton::Left)
                }
                (2, true) => {
                    self.right_button_down = true;
                    (CGEventType::RightMouseDown, CGMouseButton::Right)
                }
                (2, false) => {
                    self.right_button_down = false;
                    (CGEventType::RightMouseUp, CGMouseButton::Right)
                }
                (_, true) => (CGEventType::OtherMouseDown, CGMouseButton::Center),
                (_, false) => (CGEventType::OtherMouseUp, CGMouseButton::Center),
            };

            let ev = CGEvent::new_mouse_event(source, etype, pt, cg_btn)
                .map_err(|_| PalError::InjectionFailed("CGEvent mouse button failed".into()))?;
            ev.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, SPANKVM_INJECT_MAGIC);
            ev.post(CGEventTapLocation::HID);
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (button, pressed);
        }
        Ok(())
    }

    pub fn inject_symbolic_key(&mut self, symbolic_key: &str, pressed: bool) -> Result<(), PalError> {
        #[cfg(target_os = "macos")]
        {
            use crate::keymap::lookup_by_canonical;
            use core_graphics::event::{CGEvent, CGEventTapLocation, EventField};
            use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

            if let Some(entry) = lookup_by_canonical(symbolic_key) {
                let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                    .map_err(|_| PalError::InjectionFailed("CGEventSource failed".into()))?;
                let ev = CGEvent::new_keyboard_event(source, entry.macos_cg, pressed)
                    .map_err(|_| PalError::InjectionFailed("CGEvent keyboard failed".into()))?;
                ev.set_integer_value_field(
                    EventField::EVENT_SOURCE_USER_DATA,
                    SPANKVM_INJECT_MAGIC,
                );
                ev.post(CGEventTapLocation::HID);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (symbolic_key, pressed);
        }
        Ok(())
    }
}

impl InputCaptureBackend for MacOsCoreGraphicsDriver {
    fn set_suppress_local_input(&mut self, suppress: bool) -> Result<(), PalError> {
        self.suppressing_local = suppress;
        set_local_cursor_associated(!suppress);
        Ok(())
    }

    fn poll_raw_events(&mut self, _out: &mut Vec<RawInputEvent>) -> Result<(), PalError> {
        Ok(())
    }
}

impl InputInjectorBackend for MacOsCoreGraphicsDriver {
    fn inject_mouse_delta(&mut self, dx: i32, dy: i32) -> Result<(), PalError> {
        #[cfg(target_os = "macos")]
        {
            use core_graphics::display::CGDisplay;
            use core_graphics::event::{
                CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, EventField,
            };
            use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
            use core_graphics::geometry::CGPoint;

            let (w, h) = self.screen_size;
            let nx = (self.cursor_pos.0 + dx as f64).clamp(0.0, (w - 1.0).max(1.0));
            let ny = (self.cursor_pos.1 + dy as f64).clamp(0.0, (h - 1.0).max(1.0));
            self.cursor_pos = (nx, ny);

            let pt = CGPoint::new(nx, ny);
            let _ = CGDisplay::warp_mouse_cursor_position(pt);

            if let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) {
                let (etype, btn) = if self.left_button_down {
                    (CGEventType::LeftMouseDragged, CGMouseButton::Left)
                } else if self.right_button_down {
                    (CGEventType::RightMouseDragged, CGMouseButton::Right)
                } else {
                    (CGEventType::MouseMoved, CGMouseButton::Left)
                };
                if let Ok(ev) = CGEvent::new_mouse_event(source, etype, pt, btn) {
                    ev.set_integer_value_field(
                        EventField::EVENT_SOURCE_USER_DATA,
                        SPANKVM_INJECT_MAGIC,
                    );
                    ev.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, dx as i64);
                    ev.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y, dy as i64);
                    ev.post(CGEventTapLocation::HID);
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (dx, dy);
        }
        Ok(())
    }

    fn inject_mouse_warp_normalized(&mut self, norm_x: f32, norm_y: f32) -> Result<(), PalError> {
        let (w, h, _) = probe_macos_display();
        self.screen_size = (w as f64, h as f64);
        let px = (norm_x.clamp(0.0, 1.0) as f64) * ((w.saturating_sub(1)) as f64);
        let py = (norm_y.clamp(0.0, 1.0) as f64) * ((h.saturating_sub(1)) as f64);
        self.cursor_pos = (px, py);

        #[cfg(target_os = "macos")]
        {
            use core_graphics::display::CGDisplay;
            use core_graphics::geometry::CGPoint;
            let _ = CGDisplay::warp_mouse_cursor_position(CGPoint::new(px, py));
        }
        Ok(())
    }

    fn inject_wheel_hires(&mut self, vertical_120: i32, horizontal_120: i32) -> Result<(), PalError> {
        #[cfg(target_os = "macos")]
        {
            use core_graphics::event::{
                CGEvent, CGEventTapLocation, EventField, ScrollEventUnit,
            };
            use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

            if let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) {
                let v_px = (vertical_120 / 4).clamp(-240, 240);
                let h_px = (horizontal_120 / 4).clamp(-240, 240);
                if let Ok(ev) =
                    CGEvent::new_scroll_event(source, ScrollEventUnit::PIXEL, 2, v_px, h_px, 0)
                {
                    ev.set_integer_value_field(
                        EventField::EVENT_SOURCE_USER_DATA,
                        SPANKVM_INJECT_MAGIC,
                    );
                    ev.post(CGEventTapLocation::HID);
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
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

/// Spawns a dedicated high-priority OS thread running a macOS `CFRunLoop` + `CGEventTap`.
/// Automatically waits for Accessibility permission if not yet granted at startup.
pub fn spawn_macos_capture_thread(
    ctrl: Arc<CaptureControlState>,
    tx: UnboundedSender<CapturedInputEvent>,
) {
    #[cfg(target_os = "macos")]
    {
        std::thread::Builder::new()
            .name("spankvm-macos-eventtap".into())
            .spawn(move || {
                use crate::keymap::canonical_from_macos_cg;
                use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
                use core_graphics::display::CGDisplay;
                use core_graphics::event::{
                    CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
                    CGEventTapPlacement, CGEventType, EventField, KeyCode,
                };
                use std::cell::Cell;

                loop {
                    if !is_accessibility_trusted() {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        continue;
                    }

                    let prev_flags = Cell::new(CGEventFlags::CGEventFlagNull);
                    let ctrl_ref = Arc::clone(&ctrl);
                    let tx_ref = tx.clone();

                    let events = vec![
                        CGEventType::MouseMoved,
                        CGEventType::LeftMouseDragged,
                        CGEventType::RightMouseDragged,
                        CGEventType::OtherMouseDragged,
                        CGEventType::LeftMouseDown,
                        CGEventType::LeftMouseUp,
                        CGEventType::RightMouseDown,
                        CGEventType::RightMouseUp,
                        CGEventType::OtherMouseDown,
                        CGEventType::OtherMouseUp,
                        CGEventType::ScrollWheel,
                        CGEventType::KeyDown,
                        CGEventType::KeyUp,
                        CGEventType::FlagsChanged,
                    ];

                    let tap_res = CGEventTap::new(
                        CGEventTapLocation::HID,
                        CGEventTapPlacement::HeadInsertEventTap,
                        CGEventTapOptions::Default,
                        events,
                        move |_proxy, etype, event| {
                            // Ignore synthetic events injected by SpanKVM itself
                            if event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA)
                                == SPANKVM_INJECT_MAGIC
                            {
                                return Some(event.clone());
                            }

                            if !ctrl_ref.is_daemon_enabled() {
                                return Some(event.clone());
                            }

                            // Emergency failsafe hotkey: Ctrl + Option + Shift + Esc
                            if matches!(etype, CGEventType::KeyDown) {
                                let keycode = event
                                    .get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE)
                                    as u16;
                                let flags = event.get_flags();
                                if keycode == KeyCode::ESCAPE
                                    && flags.contains(CGEventFlags::CGEventFlagControl)
                                    && flags.contains(CGEventFlags::CGEventFlagAlternate)
                                    && flags.contains(CGEventFlags::CGEventFlagShift)
                                {
                                    ctrl_ref.set_suppressing(false);
                                    let _ =
                                        CGDisplay::associate_mouse_and_mouse_cursor_position(true);
                                    let _ = tx_ref.send(CapturedInputEvent::EmergencyReturn);
                                    return Some(event.clone());
                                }
                            }

                            let suppressing = ctrl_ref.is_suppressing();

                            if !suppressing {
                                // Local computer currently owns focus: check if pointer hits an armed edge
                                if matches!(
                                    etype,
                                    CGEventType::MouseMoved
                                        | CGEventType::LeftMouseDragged
                                        | CGEventType::RightMouseDragged
                                ) {
                                    let loc = event.location();
                                    let main = CGDisplay::main();
                                    let w = (main.pixels_wide() as f64).max(1.0);
                                    let h = (main.pixels_high() as f64).max(1.0);
                                    let dx =
                                        event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_X);
                                    let dy =
                                        event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y);
                                    let armed = ctrl_ref.armed_edges(); // [up, down, left, right]

                                    let nx = (loc.x / w).clamp(0.0, 1.0) as f32;
                                    let ny = (loc.y / h).clamp(0.0, 1.0) as f32;

                                    if loc.x <= 1.0 && dx < 0 && armed[2] {
                                        let _ = tx_ref.send(CapturedInputEvent::EdgeCrossed {
                                            edge: "left",
                                            norm_x: 0.98,
                                            norm_y: ny,
                                        });
                                    } else if loc.x >= w - 2.0 && dx > 0 && armed[3] {
                                        let _ = tx_ref.send(CapturedInputEvent::EdgeCrossed {
                                            edge: "right",
                                            norm_x: 0.02,
                                            norm_y: ny,
                                        });
                                    } else if loc.y <= 1.0 && dy < 0 && armed[0] {
                                        let _ = tx_ref.send(CapturedInputEvent::EdgeCrossed {
                                            edge: "up",
                                            norm_x: nx,
                                            norm_y: 0.98,
                                        });
                                    } else if loc.y >= h - 2.0 && dy > 0 && armed[1] {
                                        let _ = tx_ref.send(CapturedInputEvent::EdgeCrossed {
                                            edge: "down",
                                            norm_x: nx,
                                            norm_y: 0.02,
                                        });
                                    }
                                }
                                return Some(event.clone());
                            }

                            // Remote computer currently owns focus: forward raw events & suppress locally
                            match etype {
                                CGEventType::MouseMoved
                                | CGEventType::LeftMouseDragged
                                | CGEventType::RightMouseDragged
                                | CGEventType::OtherMouseDragged => {
                                    let dx = event
                                        .get_integer_value_field(EventField::MOUSE_EVENT_DELTA_X)
                                        as i32;
                                    let dy = event
                                        .get_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y)
                                        as i32;
                                    if dx != 0 || dy != 0 {
                                        let _ = tx_ref
                                            .send(CapturedInputEvent::MouseDelta { dx, dy });
                                    }
                                    None
                                }
                                CGEventType::LeftMouseDown => {
                                    let _ = tx_ref.send(CapturedInputEvent::MouseButton {
                                        button: 1,
                                        pressed: true,
                                    });
                                    None
                                }
                                CGEventType::LeftMouseUp => {
                                    let _ = tx_ref.send(CapturedInputEvent::MouseButton {
                                        button: 1,
                                        pressed: false,
                                    });
                                    None
                                }
                                CGEventType::RightMouseDown => {
                                    let _ = tx_ref.send(CapturedInputEvent::MouseButton {
                                        button: 2,
                                        pressed: true,
                                    });
                                    None
                                }
                                CGEventType::RightMouseUp => {
                                    let _ = tx_ref.send(CapturedInputEvent::MouseButton {
                                        button: 2,
                                        pressed: false,
                                    });
                                    None
                                }
                                CGEventType::OtherMouseDown => {
                                    let _ = tx_ref.send(CapturedInputEvent::MouseButton {
                                        button: 3,
                                        pressed: true,
                                    });
                                    None
                                }
                                CGEventType::OtherMouseUp => {
                                    let _ = tx_ref.send(CapturedInputEvent::MouseButton {
                                        button: 3,
                                        pressed: false,
                                    });
                                    None
                                }
                                CGEventType::ScrollWheel => {
                                    let v = event.get_integer_value_field(
                                        EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1,
                                    ) as i32;
                                    let h = event.get_integer_value_field(
                                        EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_2,
                                    ) as i32;
                                    if v != 0 || h != 0 {
                                        let _ = tx_ref.send(CapturedInputEvent::WheelHiRes {
                                            vertical_120: v * 4,
                                            horizontal_120: h * 4,
                                        });
                                    }
                                    None
                                }
                                CGEventType::KeyDown | CGEventType::KeyUp => {
                                    let keycode = event.get_integer_value_field(
                                        EventField::KEYBOARD_EVENT_KEYCODE,
                                    ) as u16;
                                    let pressed = matches!(etype, CGEventType::KeyDown);
                                    if let Some(entry) = canonical_from_macos_cg(keycode) {
                                        let _ = tx_ref.send(CapturedInputEvent::SymbolicKey {
                                            symbolic_key: entry.canonical.to_string(),
                                            scancode: entry.linux_evdev,
                                            pressed,
                                        });
                                    }
                                    None
                                }
                                CGEventType::FlagsChanged => {
                                    let keycode = event.get_integer_value_field(
                                        EventField::KEYBOARD_EVENT_KEYCODE,
                                    ) as u16;
                                    let flags = event.get_flags();
                                    let old = prev_flags.get();
                                    prev_flags.set(flags);

                                    let mask = match keycode {
                                        KeyCode::COMMAND | KeyCode::RIGHT_COMMAND => {
                                            Some(CGEventFlags::CGEventFlagCommand)
                                        }
                                        KeyCode::SHIFT | KeyCode::RIGHT_SHIFT => {
                                            Some(CGEventFlags::CGEventFlagShift)
                                        }
                                        KeyCode::OPTION | KeyCode::RIGHT_OPTION => {
                                            Some(CGEventFlags::CGEventFlagAlternate)
                                        }
                                        KeyCode::CONTROL | KeyCode::RIGHT_CONTROL => {
                                            Some(CGEventFlags::CGEventFlagControl)
                                        }
                                        KeyCode::CAPS_LOCK => {
                                            Some(CGEventFlags::CGEventFlagAlphaShift)
                                        }
                                        _ => None,
                                    };

                                    if let (Some(bit), Some(entry)) =
                                        (mask, canonical_from_macos_cg(keycode))
                                    {
                                        let now_on = flags.contains(bit);
                                        let was_on = old.contains(bit);
                                        if now_on != was_on {
                                            let _ = tx_ref.send(CapturedInputEvent::SymbolicKey {
                                                symbolic_key: entry.canonical.to_string(),
                                                scancode: entry.linux_evdev,
                                                pressed: now_on,
                                            });
                                        }
                                    }
                                    None
                                }
                                _ => Some(event.clone()),
                            }
                        },
                    );

                    match tap_res {
                        Ok(tap) => unsafe {
                            if let Ok(source) = tap.mach_port.create_runloop_source(0) {
                                let rl = CFRunLoop::get_current();
                                rl.add_source(&source, kCFRunLoopCommonModes);
                                tap.enable();
                                CFRunLoop::run_current();
                            }
                        },
                        Err(()) => {
                            std::thread::sleep(std::time::Duration::from_secs(2));
                        }
                    }
                }
            })
            .ok();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ctrl, tx);
    }
}
