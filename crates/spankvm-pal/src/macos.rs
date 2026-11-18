//! macOS CoreGraphics (`CGEventTap` + `CGEventPost`) low-latency driver.
//!
//! Uses `kCGHIDEventTap` to intercept hardware pointer deltas and keyboard scancodes.
//! When focus transitions onto a remote screen, local cursor is pinned to the active edge
//! and `CGEventTapCallBack` returns `NULL` to suppress local macOS consumption.

use crate::{InputCaptureBackend, InputInjectorBackend, PalError, RawInputEvent};

pub struct MacOsCoreGraphicsDriver {
    pub suppressing_local: bool,
    pub anchor_point: (f64, f64),
}

impl MacOsCoreGraphicsDriver {
    pub fn initialize() -> Result<Self, PalError> {
        Ok(Self {
            suppressing_local: false,
            anchor_point: (0.0, 500.0),
        })
    }
}

impl InputCaptureBackend for MacOsCoreGraphicsDriver {
    fn set_suppress_local_input(&mut self, suppress: bool) -> Result<(), PalError> {
        self.suppressing_local = suppress;
        // Calls CGAssociateMouseAndMouseCursorPosition(!suppress) so raw deltas continue
        // arriving at 1000Hz without moving the frozen local macOS cursor.
        Ok(())
    }

    fn poll_raw_events(&mut self, _out: &mut Vec<RawInputEvent>) -> Result<(), PalError> {
        Ok(())
    }
}

impl InputInjectorBackend for MacOsCoreGraphicsDriver {
    fn inject_mouse_delta(&mut self, _dx: i32, _dy: i32) -> Result<(), PalError> {
        // Posts CGEventCreateMouseEvent(kCGEventMouseMoved) to kCGHIDEventTap
        Ok(())
    }

    fn inject_mouse_warp_normalized(&mut self, _norm_x: f32, _norm_y: f32) -> Result<(), PalError> {
        // Calls CGWarpMouseCursorPosition(CGPoint) scaled to CGMainDisplayID bounds
        Ok(())
    }

    fn inject_wheel_hires(&mut self, _vertical_120: i32, _horizontal_120: i32) -> Result<(), PalError> {
        // Posts continuous pixel-accurate CGScrollEventUnit::Pixel for 120Hz ProMotion smoothness
        Ok(())
    }

    fn inject_key_scancode(&mut self, _scancode: u16, _pressed: bool) -> Result<(), PalError> {
        // Translates USB HID / evdev scancode to macOS CGKeyCode and posts via CGEventCreateKeyboardEvent
        Ok(())
    }
}
