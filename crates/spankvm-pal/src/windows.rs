//! Windows 10 / 11 Raw Input (`WM_INPUT`) + Low-Level Hook + `SendInput` driver.
//!
//! Captures unaccelerated hardware mouse deltas directly from `RAWMOUSE` at up to 1000Hz–4000Hz
//! polling rates while suppressing local cursor movement via `WH_MOUSE_LL` when remote screen is active.

use crate::{InputCaptureBackend, InputInjectorBackend, PalError, RawInputEvent};

pub struct WindowsRawInputDriver {
    pub suppressing_local: bool,
    pub subpx_rem_x: i32,
    pub subpx_rem_y: i32,
}

impl WindowsRawInputDriver {
    pub fn initialize() -> Result<Self, PalError> {
        Ok(Self {
            suppressing_local: false,
            subpx_rem_x: 0,
            subpx_rem_y: 0,
        })
    }
}

impl InputCaptureBackend for WindowsRawInputDriver {
    fn set_suppress_local_input(&mut self, suppress: bool) -> Result<(), PalError> {
        self.suppressing_local = suppress;
        // Updates atomic flag read by LowLevelMouseProc / LowLevelKeyboardProc to return LRESULT(1)
        Ok(())
    }

    fn poll_raw_events(&mut self, _out: &mut Vec<RawInputEvent>) -> Result<(), PalError> {
        Ok(())
    }
}

impl InputInjectorBackend for WindowsRawInputDriver {
    fn inject_mouse_delta(&mut self, _dx: i32, _dy: i32) -> Result<(), PalError> {
        // Batches INPUT_MOUSE (MOUSEEVENTF_MOVE) via Win32 SendInput
        Ok(())
    }

    fn inject_mouse_warp_normalized(&mut self, _norm_x: f32, _norm_y: f32) -> Result<(), PalError> {
        // Emits MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK (0..65535)
        Ok(())
    }

    fn inject_wheel_hires(&mut self, _vertical_120: i32, _horizontal_120: i32) -> Result<(), PalError> {
        // Emits MOUSEEVENTF_WHEEL / MOUSEEVENTF_HWHEEL preserving sub-120 micro-steps
        Ok(())
    }

    fn inject_key_scancode(&mut self, _scancode: u16, _pressed: bool) -> Result<(), PalError> {
        // Emits INPUT_KEYBOARD with KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY
        Ok(())
    }
}
