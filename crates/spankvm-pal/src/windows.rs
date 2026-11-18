//! Windows 10 / 11 Low-Level Hook (`WH_MOUSE_LL` / `WH_KEYBOARD_LL`) + `SendInput` driver.
//!
//! - Captures hardware mouse movement, buttons, wheel, and keyboard events globally.
//! - Suppresses local Windows cursor/keyboard (`LRESULT(1)`) when focus crosses onto a remote computer,
//!   while always honoring `Ctrl + Alt + Shift + Esc` emergency return.
//! - Injects incoming remote events via Win32 `SendInput` tagged with `SPANKVM_INJECT_MAGIC`.

use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    CaptureControlState, CapturedInputEvent, InputCaptureBackend, InputInjectorBackend, PalError,
    RawInputEvent,
};

#[cfg(target_os = "windows")]
const WIN_INJECT_MAGIC: usize = 0x5350_4B56;

pub fn probe_windows_display() -> (u32, u32, u16) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        let w = GetSystemMetrics(SM_CXSCREEN).max(1280) as u32;
        let h = GetSystemMetrics(SM_CYSCREEN).max(720) as u32;
        (w, h, 120)
    }
    #[cfg(not(target_os = "windows"))]
    {
        (1920, 1080, 120)
    }
}

pub struct WindowsRawInputDriver {
    pub suppressing_local: bool,
}

impl WindowsRawInputDriver {
    pub fn initialize() -> Result<Self, PalError> {
        Ok(Self {
            suppressing_local: false,
        })
    }

    pub fn inject_mouse_button(&mut self, button: u8, pressed: bool) -> Result<(), PalError> {
        #[cfg(target_os = "windows")]
        unsafe {
            use std::mem::size_of;
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
                MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN,
                MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
            };

            let flags = match (button, pressed) {
                (1, true) => MOUSEEVENTF_LEFTDOWN,
                (1, false) => MOUSEEVENTF_LEFTUP,
                (2, true) => MOUSEEVENTF_RIGHTDOWN,
                (2, false) => MOUSEEVENTF_RIGHTUP,
                (_, true) => MOUSEEVENTF_MIDDLEDOWN,
                (_, false) => MOUSEEVENTF_MIDDLEUP,
            };

            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: WIN_INJECT_MAGIC,
                    },
                },
            };
            SendInput(&[input], size_of::<INPUT>() as i32);
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (button, pressed);
        }
        Ok(())
    }

    pub fn inject_symbolic_key(&mut self, symbolic_key: &str, pressed: bool) -> Result<(), PalError> {
        #[cfg(target_os = "windows")]
        unsafe {
            use crate::keymap::lookup_by_canonical;
            use std::mem::size_of;
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
                KEYEVENTF_KEYUP, VIRTUAL_KEY,
            };

            if let Some(entry) = lookup_by_canonical(symbolic_key) {
                let flags = if pressed {
                    KEYBD_EVENT_FLAGS(0)
                } else {
                    KEYEVENTF_KEYUP
                };
                let input = INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(entry.win_vk),
                            wScan: entry.win_scan,
                            dwFlags: flags,
                            time: 0,
                            dwExtraInfo: WIN_INJECT_MAGIC,
                        },
                    },
                };
                SendInput(&[input], size_of::<INPUT>() as i32);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (symbolic_key, pressed);
        }
        Ok(())
    }
}

impl InputCaptureBackend for WindowsRawInputDriver {
    fn set_suppress_local_input(&mut self, suppress: bool) -> Result<(), PalError> {
        self.suppressing_local = suppress;
        Ok(())
    }

    fn poll_raw_events(&mut self, _out: &mut Vec<RawInputEvent>) -> Result<(), PalError> {
        Ok(())
    }
}

impl InputInjectorBackend for WindowsRawInputDriver {
    fn inject_mouse_delta(&mut self, dx: i32, dy: i32) -> Result<(), PalError> {
        #[cfg(target_os = "windows")]
        unsafe {
            use std::mem::size_of;
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT,
            };

            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx,
                        dy,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_MOVE,
                        time: 0,
                        dwExtraInfo: WIN_INJECT_MAGIC,
                    },
                },
            };
            SendInput(&[input], size_of::<INPUT>() as i32);
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (dx, dy);
        }
        Ok(())
    }

    fn inject_mouse_warp_normalized(&mut self, norm_x: f32, norm_y: f32) -> Result<(), PalError> {
        #[cfg(target_os = "windows")]
        unsafe {
            use std::mem::size_of;
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
                MOUSEINPUT,
            };

            let ax = (norm_x.clamp(0.0, 1.0) * 65535.0).round() as i32;
            let ay = (norm_y.clamp(0.0, 1.0) * 65535.0).round() as i32;
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: ax,
                        dy: ay,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                        time: 0,
                        dwExtraInfo: WIN_INJECT_MAGIC,
                    },
                },
            };
            SendInput(&[input], size_of::<INPUT>() as i32);
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (norm_x, norm_y);
        }
        Ok(())
    }

    fn inject_wheel_hires(&mut self, vertical_120: i32, horizontal_120: i32) -> Result<(), PalError> {
        #[cfg(target_os = "windows")]
        unsafe {
            use std::mem::size_of;
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL,
                MOUSEINPUT,
            };

            if vertical_120 != 0 {
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: vertical_120 as u32,
                            dwFlags: MOUSEEVENTF_WHEEL,
                            time: 0,
                            dwExtraInfo: WIN_INJECT_MAGIC,
                        },
                    },
                };
                SendInput(&[input], size_of::<INPUT>() as i32);
            }
            if horizontal_120 != 0 {
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: horizontal_120 as u32,
                            dwFlags: MOUSEEVENTF_HWHEEL,
                            time: 0,
                            dwExtraInfo: WIN_INJECT_MAGIC,
                        },
                    },
                };
                SendInput(&[input], size_of::<INPUT>() as i32);
            }
        }
        #[cfg(not(target_os = "windows"))]
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

#[cfg(target_os = "windows")]
mod win_hook_impl {
    use super::*;
    use crate::keymap::canonical_from_windows_vk;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::OnceLock;
    use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics,
        SetWindowsHookExW, TranslateMessage, HHOOK, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT,
        SM_CXSCREEN, SM_CYSCREEN, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
        WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    struct HookCtx {
        ctrl: Arc<CaptureControlState>,
        tx: UnboundedSender<CapturedInputEvent>,
        last_x: AtomicI32,
        last_y: AtomicI32,
    }

    static HOOK_CTX: OnceLock<HookCtx> = OnceLock::new();

    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            if let Some(ctx) = HOOK_CTX.get() {
                let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
                if info.dwExtraInfo != WIN_INJECT_MAGIC && ctx.ctrl.is_daemon_enabled() {
                    let msg = wparam.0 as u32;
                    let w = GetSystemMetrics(SM_CXSCREEN).max(1);
                    let h = GetSystemMetrics(SM_CYSCREEN).max(1);
                    let prev_x = ctx.last_x.swap(info.pt.x, Ordering::Relaxed);
                    let prev_y = ctx.last_y.swap(info.pt.y, Ordering::Relaxed);

                    if !ctx.ctrl.is_suppressing() {
                        if msg == WM_MOUSEMOVE {
                            let dx = info.pt.x - prev_x;
                            let dy = info.pt.y - prev_y;
                            let armed = ctx.ctrl.armed_edges();
                            let nx = (info.pt.x as f32 / w as f32).clamp(0.0, 1.0);
                            let ny = (info.pt.y as f32 / h as f32).clamp(0.0, 1.0);

                            if info.pt.x <= 1 && dx < 0 && armed[2] {
                                let _ = ctx.tx.send(CapturedInputEvent::EdgeCrossed {
                                    edge: "left",
                                    norm_x: 0.98,
                                    norm_y: ny,
                                });
                            } else if info.pt.x >= w - 2 && dx > 0 && armed[3] {
                                let _ = ctx.tx.send(CapturedInputEvent::EdgeCrossed {
                                    edge: "right",
                                    norm_x: 0.02,
                                    norm_y: ny,
                                });
                            } else if info.pt.y <= 1 && dy < 0 && armed[0] {
                                let _ = ctx.tx.send(CapturedInputEvent::EdgeCrossed {
                                    edge: "up",
                                    norm_x: nx,
                                    norm_y: 0.98,
                                });
                            } else if info.pt.y >= h - 2 && dy > 0 && armed[1] {
                                let _ = ctx.tx.send(CapturedInputEvent::EdgeCrossed {
                                    edge: "down",
                                    norm_x: nx,
                                    norm_y: 0.02,
                                });
                            }
                        }
                    } else {
                        match msg {
                            WM_MOUSEMOVE => {
                                // When suppressed on Windows, cursor stays fixed so info.pt - prev is raw delta
                                let mut cur = POINT::default();
                                let _ = GetCursorPos(&mut cur);
                                let dx = info.pt.x - cur.x;
                                let dy = info.pt.y - cur.y;
                                if dx != 0 || dy != 0 {
                                    let _ = ctx.tx.send(CapturedInputEvent::MouseDelta { dx, dy });
                                }
                                return LRESULT(1);
                            }
                            WM_LBUTTONDOWN => {
                                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                                    button: 1,
                                    pressed: true,
                                });
                                return LRESULT(1);
                            }
                            WM_LBUTTONUP => {
                                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                                    button: 1,
                                    pressed: false,
                                });
                                return LRESULT(1);
                            }
                            WM_RBUTTONDOWN => {
                                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                                    button: 2,
                                    pressed: true,
                                });
                                return LRESULT(1);
                            }
                            WM_RBUTTONUP => {
                                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                                    button: 2,
                                    pressed: false,
                                });
                                return LRESULT(1);
                            }
                            WM_MBUTTONDOWN => {
                                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                                    button: 3,
                                    pressed: true,
                                });
                                return LRESULT(1);
                            }
                            WM_MBUTTONUP => {
                                let _ = ctx.tx.send(CapturedInputEvent::MouseButton {
                                    button: 3,
                                    pressed: false,
                                });
                                return LRESULT(1);
                            }
                            WM_MOUSEWHEEL => {
                                let delta = ((info.mouseData >> 16) as i16) as i32;
                                let _ = ctx.tx.send(CapturedInputEvent::WheelHiRes {
                                    vertical_120: delta,
                                    horizontal_120: 0,
                                });
                                return LRESULT(1);
                            }
                            WM_MOUSEHWHEEL => {
                                let delta = ((info.mouseData >> 16) as i16) as i32;
                                let _ = ctx.tx.send(CapturedInputEvent::WheelHiRes {
                                    vertical_120: 0,
                                    horizontal_120: delta,
                                });
                                return LRESULT(1);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        CallNextHookEx(Option::<HHOOK>::None, code, wparam, lparam)
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            if let Some(ctx) = HOOK_CTX.get() {
                let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
                if info.dwExtraInfo != WIN_INJECT_MAGIC && ctx.ctrl.is_daemon_enabled() {
                    let msg = wparam.0 as u32;
                    let pressed = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
                    let released = msg == WM_KEYUP || msg == WM_SYSKEYUP;

                    if pressed && info.vkCode == 0x1B {
                        let ctrl = GetAsyncKeyState(0x11) < 0;
                        let alt = GetAsyncKeyState(0x12) < 0;
                        let shift = GetAsyncKeyState(0x10) < 0;
                        if ctrl && alt && shift {
                            ctx.ctrl.set_suppressing(false);
                            let _ = ctx.tx.send(CapturedInputEvent::EmergencyReturn);
                            return CallNextHookEx(Option::<HHOOK>::None, code, wparam, lparam);
                        }
                    }

                    if ctx.ctrl.is_suppressing() && (pressed || released) {
                        if let Some(entry) = canonical_from_windows_vk(info.vkCode as u16) {
                            let _ = ctx.tx.send(CapturedInputEvent::SymbolicKey {
                                symbolic_key: entry.canonical.to_string(),
                                scancode: entry.linux_evdev,
                                pressed,
                            });
                        }
                        return LRESULT(1);
                    }
                }
            }
        }
        CallNextHookEx(Option::<HHOOK>::None, code, wparam, lparam)
    }

    pub fn spawn(ctrl: Arc<CaptureControlState>, tx: UnboundedSender<CapturedInputEvent>) {
        let _ = HOOK_CTX.set(HookCtx {
            ctrl,
            tx,
            last_x: AtomicI32::new(500),
            last_y: AtomicI32::new(500),
        });

        std::thread::Builder::new()
            .name("spankvm-win32-llhook".into())
            .spawn(move || unsafe {
                let _m_hook = SetWindowsHookExW(
                    WH_MOUSE_LL,
                    Some(mouse_proc),
                    Option::<HINSTANCE>::None,
                    0,
                );
                let _k_hook = SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(keyboard_proc),
                    Option::<HINSTANCE>::None,
                    0,
                );
                let _ = HMODULE::default();

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, Option::<HWND>::None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .ok();
    }
}

pub fn spawn_windows_capture_thread(
    ctrl: Arc<CaptureControlState>,
    tx: UnboundedSender<CapturedInputEvent>,
) {
    #[cfg(target_os = "windows")]
    {
        win_hook_impl::spawn(ctrl, tx);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (ctrl, tx);
    }
}
