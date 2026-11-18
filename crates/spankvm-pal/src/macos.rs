//! macOS CoreGraphics (`CGEventTapCreate` + `CGEventPost`) exclusive capture & injection driver.
//!
//! - Uses direct `ApplicationServices` C FFI for `CGEventTapCreate` so suppressed events return
//!   `NULL` (`std::ptr::null_mut()`) directly to macOS WindowServer (working around `core-graphics`
//!   0.24's wrapper which returns the original `CGEventRef` on `None`).
//! - Configures `CGEventSourceSetLocalEventsSuppressionInterval(0.0)` and locks the local cursor
//!   to the primary display center via `CGWarpMouseCursorPosition` + `CGAssociateMouseAndMouseCursorPosition(false)`
//!   while remote control is active, ensuring Machine A's cursor and keyboard never leak locally
//!   even when SpanKVM is in the background.
//! - Automatically re-enables the tap on `kCGEventTapDisabledByTimeout` / `kCGEventTapDisabledByUserInput`.
//! - Always honors `Ctrl + Option + Shift + Esc` emergency focus reclaim.

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

/// Returns `(width_points, height_points, refresh_rate_hz)` in global Quartz display coordinates.
pub fn probe_macos_display() -> (u32, u32, u16) {
    #[cfg(target_os = "macos")]
    {
        use core_graphics::display::CGDisplay;
        let main = CGDisplay::main();
        let bounds = main.bounds();
        let w = (bounds.size.width.round() as u32).max(1280);
        let h = (bounds.size.height.round() as u32).max(720);
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
    pub screen_origin: (f64, f64),
    pub screen_size: (f64, f64),
    pub left_button_down: bool,
    pub right_button_down: bool,
}

impl MacOsCoreGraphicsDriver {
    pub fn initialize() -> Result<Self, PalError> {
        #[cfg(target_os = "macos")]
        {
            use core_graphics::display::CGDisplay;
            let bounds = CGDisplay::main().bounds();
            let ox = bounds.origin.x;
            let oy = bounds.origin.y;
            let w = bounds.size.width.max(1280.0);
            let h = bounds.size.height.max(720.0);
            configure_zero_suppression_interval();
            return Ok(Self {
                suppressing_local: false,
                cursor_pos: (ox + w * 0.5, oy + h * 0.5),
                screen_origin: (ox, oy),
                screen_size: (w, h),
                left_button_down: false,
                right_button_down: false,
            });
        }
        #[cfg(not(target_os = "macos"))]
        {
            let (w, h, _) = probe_macos_display();
            Ok(Self {
                suppressing_local: false,
                cursor_pos: (w as f64 * 0.5, h as f64 * 0.5),
                screen_origin: (0.0, 0.0),
                screen_size: (w as f64, h as f64),
                left_button_down: false,
                right_button_down: false,
            })
        }
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

            let (ox, oy) = self.screen_origin;
            let (w, h) = self.screen_size;
            let nx = (self.cursor_pos.0 + dx as f64).clamp(ox, ox + (w - 1.0).max(1.0));
            let ny = (self.cursor_pos.1 + dy as f64).clamp(oy, oy + (h - 1.0).max(1.0));
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
        #[cfg(target_os = "macos")]
        {
            use core_graphics::display::CGDisplay;
            use core_graphics::geometry::CGPoint;
            let bounds = CGDisplay::main().bounds();
            let ox = bounds.origin.x;
            let oy = bounds.origin.y;
            let w = bounds.size.width.max(1.0);
            let h = bounds.size.height.max(1.0);
            self.screen_origin = (ox, oy);
            self.screen_size = (w, h);
            let px = ox + (norm_x.clamp(0.0, 1.0) as f64) * (w - 1.0).max(1.0);
            let py = oy + (norm_y.clamp(0.0, 1.0) as f64) * (h - 1.0).max(1.0);
            self.cursor_pos = (px, py);
            let _ = CGDisplay::associate_mouse_and_mouse_cursor_position(true);
            let _ = CGDisplay::warp_mouse_cursor_position(CGPoint::new(px, py));
            return Ok(());
        }
        #[cfg(not(target_os = "macos"))]
        {
            let (w, h, _) = probe_macos_display();
            self.screen_size = (w as f64, h as f64);
            let px = (norm_x.clamp(0.0, 1.0) as f64) * ((w.saturating_sub(1)) as f64);
            let py = (norm_y.clamp(0.0, 1.0) as f64) * ((h.saturating_sub(1)) as f64);
            self.cursor_pos = (px, py);
            Ok(())
        }
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

#[cfg(target_os = "macos")]
fn configure_zero_suppression_interval() {
    use std::ffi::c_void;
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGEventSourceCreate(state_id: i32) -> *mut c_void;
        fn CGEventSourceSetLocalEventsSuppressionInterval(source: *mut c_void, seconds: f64);
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }
    unsafe {
        // kCGEventSourceStateCombinedSessionState = 0, kCGEventSourceStateHIDSystemState = 1
        for state_id in [0i32, 1i32] {
            let src = CGEventSourceCreate(state_id);
            if !src.is_null() {
                CGEventSourceSetLocalEventsSuppressionInterval(src, 0.0);
                CFRelease(src.cast());
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod raw_tap_impl {
    use super::*;
    use crate::keymap::canonical_from_macos_cg;
    use core_foundation::base::TCFType;
    use core_foundation::mach_port::{CFMachPort, CFMachPortRef};
    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
    use core_graphics::display::CGDisplay;
    use core_graphics::event::{CGEventFlags, EventField, KeyCode};
    use core_graphics::geometry::CGPoint;
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicPtr, Ordering};

    const K_CG_HID_EVENT_TAP: u32 = 0;
    const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
    const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;

    // CGEventType raw discriminant values from <CoreGraphics/CGEventTypes.h>
    const EV_LEFT_MOUSE_DOWN: u32 = 1;
    const EV_LEFT_MOUSE_UP: u32 = 2;
    const EV_RIGHT_MOUSE_DOWN: u32 = 3;
    const EV_RIGHT_MOUSE_UP: u32 = 4;
    const EV_MOUSE_MOVED: u32 = 5;
    const EV_LEFT_MOUSE_DRAGGED: u32 = 6;
    const EV_RIGHT_MOUSE_DRAGGED: u32 = 7;
    const EV_KEY_DOWN: u32 = 10;
    const EV_KEY_UP: u32 = 11;
    const EV_FLAGS_CHANGED: u32 = 12;
    const EV_NS_SYSTEM_DEFINED: u32 = 14;
    const EV_SCROLL_WHEEL: u32 = 22;
    const EV_OTHER_MOUSE_DOWN: u32 = 25;
    const EV_OTHER_MOUSE_UP: u32 = 26;
    const EV_OTHER_MOUSE_DRAGGED: u32 = 27;
    const EV_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
    const EV_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

    type RawTapCallback = unsafe extern "C" fn(
        proxy: *mut c_void,
        etype: u32,
        event: *mut c_void,
        user_info: *mut c_void,
    ) -> *mut c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: RawTapCallback,
            user_info: *mut c_void,
        ) -> CFMachPortRef;
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventGetLocation(event: *const c_void) -> CGPoint;
        fn CGEventGetFlags(event: *const c_void) -> u64;
        fn CGEventGetIntegerValueField(event: *const c_void, field: u32) -> i64;
    }

    struct TapContext {
        ctrl: Arc<CaptureControlState>,
        tx: UnboundedSender<CapturedInputEvent>,
        prev_flags: Cell<u64>,
        tap_port: AtomicPtr<c_void>,
    }

    #[inline]
    fn primary_center_point() -> (CGPoint, f64, f64, f64, f64) {
        let bounds = CGDisplay::main().bounds();
        let ox = bounds.origin.x;
        let oy = bounds.origin.y;
        let w = bounds.size.width.max(1.0);
        let h = bounds.size.height.max(1.0);
        (CGPoint::new(ox + w * 0.5, oy + h * 0.5), ox, oy, w, h)
    }

    #[inline]
    fn lock_cursor_at_center() {
        let (center, _, _, _, _) = primary_center_point();
        let _ = CGDisplay::warp_mouse_cursor_position(center);
        let _ = CGDisplay::associate_mouse_and_mouse_cursor_position(false);
    }

    unsafe extern "C" fn event_tap_callback(
        _proxy: *mut c_void,
        etype: u32,
        event: *mut c_void,
        user_info: *mut c_void,
    ) -> *mut c_void {
        if user_info.is_null() {
            return event;
        }
        let ctx = &*(user_info as *const TapContext);

        // Re-enable immediately if macOS temporarily disabled the tap due to timeout
        if etype == EV_TAP_DISABLED_BY_TIMEOUT || etype == EV_TAP_DISABLED_BY_USER_INPUT {
            let port = ctx.tap_port.load(Ordering::Acquire);
            if !port.is_null() {
                CGEventTapEnable(port.cast(), true);
            }
            return event;
        }

        if event.is_null() {
            return event;
        }

        // Never intercept synthetic events injected by SpanKVM itself
        if CGEventGetIntegerValueField(event, EventField::EVENT_SOURCE_USER_DATA)
            == SPANKVM_INJECT_MAGIC
        {
            return event;
        }

        if !ctx.ctrl.is_daemon_enabled() {
            return event;
        }

        // Emergency failsafe hotkey: Ctrl + Option + Shift + Esc
        if etype == EV_KEY_DOWN {
            let keycode =
                CGEventGetIntegerValueField(event, EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            let flags = CGEventFlags::from_bits_truncate(CGEventGetFlags(event));
            if keycode == KeyCode::ESCAPE
                && flags.contains(CGEventFlags::CGEventFlagControl)
                && flags.contains(CGEventFlags::CGEventFlagAlternate)
                && flags.contains(CGEventFlags::CGEventFlagShift)
            {
                ctx.ctrl.set_suppressing(false);
                let _ = CGDisplay::associate_mouse_and_mouse_cursor_position(true);
                let _ = ctx.tx.send(CapturedInputEvent::EmergencyReturn);
                return std::ptr::null_mut();
            }
        }

        if !ctx.ctrl.is_suppressing() {
            // Local Mac currently owns focus: check if pointer hits an armed edge
            if matches!(
                etype,
                EV_MOUSE_MOVED | EV_LEFT_MOUSE_DRAGGED | EV_RIGHT_MOUSE_DRAGGED
            ) {
                let loc = CGEventGetLocation(event);
                let (_, ox, oy, w, h) = primary_center_point();
                let rel_x = loc.x - ox;
                let rel_y = loc.y - oy;
                let dx = CGEventGetIntegerValueField(event, EventField::MOUSE_EVENT_DELTA_X);
                let dy = CGEventGetIntegerValueField(event, EventField::MOUSE_EVENT_DELTA_Y);
                let armed = ctx.ctrl.armed_edges(); // [up, down, left, right]

                let nx = (rel_x / w).clamp(0.0, 1.0) as f32;
                let ny = (rel_y / h).clamp(0.0, 1.0) as f32;

                let crossed = if rel_x <= 1.5 && dx < 0 && armed[2] {
                    Some(("left", 0.98f32, ny))
                } else if rel_x >= w - 2.5 && dx > 0 && armed[3] {
                    Some(("right", 0.02f32, ny))
                } else if rel_y <= 1.5 && dy < 0 && armed[0] {
                    Some(("up", nx, 0.98f32))
                } else if rel_y >= h - 2.5 && dy > 0 && armed[1] {
                    Some(("down", nx, 0.02f32))
                } else {
                    None
                };

                if let Some((edge, target_nx, target_ny)) = crossed {
                    // Engage suppression atomically right inside the hook so zero local events leak
                    ctx.ctrl.set_suppressing_from_hook(true);
                    lock_cursor_at_center();
                    let _ = ctx.tx.send(CapturedInputEvent::EdgeCrossed {
                        edge,
                        norm_x: target_nx,
                        norm_y: target_ny,
                    });
                    return std::ptr::null_mut();
                }
            }
            return event;
        }

        // Remote computer owns focus: forward events to remote AND return NULL so Mac receives NOTHING
        match etype {
            EV_MOUSE_MOVED
            | EV_LEFT_MOUSE_DRAGGED
            | EV_RIGHT_MOUSE_DRAGGED
            | EV_OTHER_MOUSE_DRAGGED => {
                let dx = CGEventGetIntegerValueField(event, EventField::MOUSE_EVENT_DELTA_X) as i32;
                let dy = CGEventGetIntegerValueField(event, EventField::MOUSE_EVENT_DELTA_Y) as i32;
                // Keep Mac cursor locked motionless at center even when SpanKVM is in background
                lock_cursor_at_center();
                if dx != 0 || dy != 0 {
                    let _ = ctx.tx.send(CapturedInputEvent::MouseDelta { dx, dy });
                }
                std::ptr::null_mut()
            }
            EV_LEFT_MOUSE_DOWN => {
                lock_cursor_at_center();
                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                    button: 1,
                    pressed: true,
                });
                std::ptr::null_mut()
            }
            EV_LEFT_MOUSE_UP => {
                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                    button: 1,
                    pressed: false,
                });
                std::ptr::null_mut()
            }
            EV_RIGHT_MOUSE_DOWN => {
                lock_cursor_at_center();
                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                    button: 2,
                    pressed: true,
                });
                std::ptr::null_mut()
            }
            EV_RIGHT_MOUSE_UP => {
                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                    button: 2,
                    pressed: false,
                });
                std::ptr::null_mut()
            }
            EV_OTHER_MOUSE_DOWN => {
                lock_cursor_at_center();
                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                    button: 3,
                    pressed: true,
                });
                std::ptr::null_mut()
            }
            EV_OTHER_MOUSE_UP => {
                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                    button: 3,
                    pressed: false,
                });
                std::ptr::null_mut()
            }
            EV_SCROLL_WHEEL => {
                let mut v = CGEventGetIntegerValueField(
                    event,
                    EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1,
                ) as i32;
                let mut h = CGEventGetIntegerValueField(
                    event,
                    EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_2,
                ) as i32;
                if v == 0 && h == 0 {
                    v = (CGEventGetIntegerValueField(
                        event,
                        EventField::SCROLL_WHEEL_EVENT_DELTA_AXIS_1,
                    ) as i32)
                        * 30;
                    h = (CGEventGetIntegerValueField(
                        event,
                        EventField::SCROLL_WHEEL_EVENT_DELTA_AXIS_2,
                    ) as i32)
                        * 30;
                }
                if v != 0 || h != 0 {
                    let _ = ctx.tx.send(CapturedInputEvent::WheelHiRes {
                        vertical_120: v * 4,
                        horizontal_120: h * 4,
                    });
                }
                std::ptr::null_mut()
            }
            EV_KEY_DOWN | EV_KEY_UP => {
                let keycode =
                    CGEventGetIntegerValueField(event, EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                let pressed = etype == EV_KEY_DOWN;
                if let Some(entry) = canonical_from_macos_cg(keycode) {
                    let _ = ctx.tx.send(CapturedInputEvent::SymbolicKey {
                        symbolic_key: entry.canonical.to_string(),
                        scancode: entry.linux_evdev,
                        pressed,
                    });
                }
                // Swallow ALL keystrokes locally on Mac while remote machine is active
                std::ptr::null_mut()
            }
            EV_FLAGS_CHANGED => {
                let keycode =
                    CGEventGetIntegerValueField(event, EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                let raw_flags = CGEventGetFlags(event);
                let old_raw = ctx.prev_flags.get();
                ctx.prev_flags.set(raw_flags);

                let flags = CGEventFlags::from_bits_truncate(raw_flags);
                let old = CGEventFlags::from_bits_truncate(old_raw);

                let mask = match keycode {
                    KeyCode::COMMAND | KeyCode::RIGHT_COMMAND => {
                        Some(CGEventFlags::CGEventFlagCommand)
                    }
                    KeyCode::SHIFT | KeyCode::RIGHT_SHIFT => Some(CGEventFlags::CGEventFlagShift),
                    KeyCode::OPTION | KeyCode::RIGHT_OPTION => {
                        Some(CGEventFlags::CGEventFlagAlternate)
                    }
                    KeyCode::CONTROL | KeyCode::RIGHT_CONTROL => {
                        Some(CGEventFlags::CGEventFlagControl)
                    }
                    KeyCode::CAPS_LOCK => Some(CGEventFlags::CGEventFlagAlphaShift),
                    _ => None,
                };

                if let (Some(bit), Some(entry)) = (mask, canonical_from_macos_cg(keycode)) {
                    let now_on = flags.contains(bit);
                    let was_on = old.contains(bit);
                    if now_on != was_on {
                        let _ = ctx.tx.send(CapturedInputEvent::SymbolicKey {
                            symbolic_key: entry.canonical.to_string(),
                            scancode: entry.linux_evdev,
                            pressed: now_on,
                        });
                    }
                }
                std::ptr::null_mut()
            }
            EV_NS_SYSTEM_DEFINED => std::ptr::null_mut(),
            _ => event,
        }
    }

    pub fn spawn(ctrl: Arc<CaptureControlState>, tx: UnboundedSender<CapturedInputEvent>) {
        std::thread::Builder::new()
            .name("spankvm-macos-eventtap".into())
            .spawn(move || {
                configure_zero_suppression_interval();

                let event_types = [
                    EV_LEFT_MOUSE_DOWN,
                    EV_LEFT_MOUSE_UP,
                    EV_RIGHT_MOUSE_DOWN,
                    EV_RIGHT_MOUSE_UP,
                    EV_MOUSE_MOVED,
                    EV_LEFT_MOUSE_DRAGGED,
                    EV_RIGHT_MOUSE_DRAGGED,
                    EV_KEY_DOWN,
                    EV_KEY_UP,
                    EV_FLAGS_CHANGED,
                    EV_NS_SYSTEM_DEFINED,
                    EV_SCROLL_WHEEL,
                    EV_OTHER_MOUSE_DOWN,
                    EV_OTHER_MOUSE_UP,
                    EV_OTHER_MOUSE_DRAGGED,
                ];
                let mask: u64 = event_types
                    .iter()
                    .fold(0u64, |acc, &et| acc | (1u64 << et));

                loop {
                    if !is_accessibility_trusted() {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        continue;
                    }

                    let ctx = Box::new(TapContext {
                        ctrl: Arc::clone(&ctrl),
                        tx: tx.clone(),
                        prev_flags: Cell::new(0),
                        tap_port: AtomicPtr::new(std::ptr::null_mut()),
                    });
                    let ctx_ptr = Box::into_raw(ctx);

                    unsafe {
                        let tap_ref = CGEventTapCreate(
                            K_CG_HID_EVENT_TAP,
                            K_CG_HEAD_INSERT_EVENT_TAP,
                            K_CG_EVENT_TAP_OPTION_DEFAULT,
                            mask,
                            event_tap_callback,
                            ctx_ptr.cast(),
                        );

                        if tap_ref.is_null() {
                            drop(Box::from_raw(ctx_ptr));
                            std::thread::sleep(std::time::Duration::from_secs(2));
                            continue;
                        }

                        (*ctx_ptr).tap_port.store(tap_ref.cast(), Ordering::Release);
                        let mach_port = CFMachPort::wrap_under_create_rule(tap_ref);
                        if let Ok(source) = mach_port.create_runloop_source(0) {
                            let rl = CFRunLoop::get_current();
                            rl.add_source(&source, kCFRunLoopCommonModes);
                            CGEventTapEnable(tap_ref, true);
                            CFRunLoop::run_current();
                        }
                        drop(Box::from_raw(ctx_ptr));
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            })
            .ok();
    }
}

pub fn spawn_macos_capture_thread(
    ctrl: Arc<CaptureControlState>,
    tx: UnboundedSender<CapturedInputEvent>,
) {
    #[cfg(target_os = "macos")]
    {
        raw_tap_impl::spawn(ctrl, tx);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ctrl, tx);
    }
}
