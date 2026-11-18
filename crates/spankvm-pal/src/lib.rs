//! Cross-platform OS driver abstraction layer (`spankvm-pal`).
//!
//! Unifies high-frequency raw input capture, local suppression when cursor leaves primary screen,
//! remote input injection, clipboard watching, and screen-edge file drag-drop hooks across:
//! - macOS (`CGEventTap` / `CGEventPost`)
//! - Windows (`WM_INPUT` Raw Input + `WH_MOUSE_LL` / `SendInput`)
//! - Linux Wayland (`ashpd` InputCapture & RemoteDesktop Portals + `reis` libei + `/dev/uinput` fallback) & X11

pub mod linux_wayland;
pub mod macos;
pub mod windows;

use spankvm_proto::LinuxWaylandStrategy;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PalError {
    #[error("系统权限未就绪: {0}")]
    PermissionDenied(String),
    #[error("底层输入驱动初始化失败: {0}")]
    DriverInitFailed(String),
    #[error("输入注入失败: {0}")]
    InjectionFailed(String),
}

#[derive(Debug, Clone, Copy)]
pub enum RawInputEvent {
    MouseDelta { dx: i32, dy: i32, timestamp_us: u32 },
    WheelHiRes { vertical_120: i32, horizontal_120: i32 },
    MouseButton { button: u8, pressed: bool },
    KeyScancode { scancode: u16, pressed: bool },
}

pub trait InputCaptureBackend: Send {
    /// Toggle whether local OS desktop consumes physical keyboard/mouse events (`false`)
    /// or suppresses them because focus has crossed onto a remote screen (`true`).
    fn set_suppress_local_input(&mut self, suppress: bool) -> Result<(), PalError>;

    /// Poll next batch of high-frequency raw hardware events (up to 1000Hz).
    fn poll_raw_events(&mut self, out: &mut Vec<RawInputEvent>) -> Result<(), PalError>;
}

pub trait InputInjectorBackend: Send {
    fn inject_mouse_delta(&mut self, dx: i32, dy: i32) -> Result<(), PalError>;
    fn inject_mouse_warp_normalized(&mut self, norm_x: f32, norm_y: f32) -> Result<(), PalError>;
    fn inject_wheel_hires(&mut self, vertical_120: i32, horizontal_120: i32) -> Result<(), PalError>;
    fn inject_key_scancode(&mut self, scancode: u16, pressed: bool) -> Result<(), PalError>;
}

#[derive(Debug, Clone)]
pub struct PlatformDriverSummary {
    pub os_backend_name: &'static str,
    pub wayland_active: bool,
    pub requires_permission_prompt: bool,
}

pub fn detect_active_driver_stack(strategy: LinuxWaylandStrategy) -> PlatformDriverSummary {
    if cfg!(target_os = "macos") {
        PlatformDriverSummary {
            os_backend_name: "macOS CoreGraphics CGEventTap + CGEventPost",
            wayland_active: false,
            requires_permission_prompt: true,
        }
    } else if cfg!(target_os = "windows") {
        PlatformDriverSummary {
            os_backend_name: "Windows Win32 RawInput (WM_INPUT) + SendInput",
            wayland_active: false,
            requires_permission_prompt: false,
        }
    } else {
        let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        match (is_wayland, strategy) {
            (true, LinuxWaylandStrategy::PortalAndUinputFallback) => PlatformDriverSummary {
                os_backend_name: "Linux Wayland Portal (libei) + /dev/uinput 双栈兼容",
                wayland_active: true,
                requires_permission_prompt: true,
            },
            (true, LinuxWaylandStrategy::PortalOnly) => PlatformDriverSummary {
                os_backend_name: "Linux Wayland XDG InputCapture/RemoteDesktop Portal (libei)",
                wayland_active: true,
                requires_permission_prompt: true,
            },
            (false, _) => PlatformDriverSummary {
                os_backend_name: "Linux X11 XI2 RawMotion + XTest",
                wayland_active: false,
                requires_permission_prompt: false,
            },
        }
    }
}
