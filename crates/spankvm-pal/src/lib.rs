//! Cross-platform OS driver abstraction layer (`spankvm-pal`).
//!
//! Unifies high-frequency raw input capture, local suppression when cursor leaves primary screen,
//! remote input injection, and OS clipboard access across:
//! - macOS (`CGEventTap` / `CGEventPost` + `AXIsProcessTrusted`)
//! - Windows (`WH_MOUSE_LL` / `WH_KEYBOARD_LL` + `SendInput`)
//! - Linux Wayland (`ashpd` InputCapture Portal + `/dev/uinput` VirtualDevice) & X11 (`xtest`)

pub mod clipboard;
pub mod keymap;
pub mod linux_wayland;
pub mod macos;
pub mod windows;

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use spankvm_proto::{LinuxWaylandStrategy, OsKind, Resolution};
use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;

pub const SPANKVM_INJECT_MAGIC: i64 = 0x5350_4B56_4D31;

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

#[derive(Debug, Clone)]
pub enum CapturedInputEvent {
    EdgeCrossed {
        edge: &'static str,
        norm_x: f32,
        norm_y: f32,
    },
    MouseDelta {
        dx: i32,
        dy: i32,
    },
    MouseButton {
        button: u8,
        pressed: bool,
    },
    WheelHiRes {
        vertical_120: i32,
        horizontal_120: i32,
    },
    SymbolicKey {
        symbolic_key: String,
        scancode: u16,
        pressed: bool,
    },
    EmergencyReturn,
}

/// Lock-free atomic state shared with real-time OS input hook callbacks.
#[derive(Debug)]
pub struct CaptureControlState {
    daemon_enabled: AtomicBool,
    suppressing_local: AtomicBool,
    /// Bitmask: bit0=up, bit1=down, bit2=left, bit3=right
    armed_edges_mask: AtomicU8,
}

impl CaptureControlState {
    pub fn new(daemon_enabled: bool) -> Self {
        Self {
            daemon_enabled: AtomicBool::new(daemon_enabled),
            suppressing_local: AtomicBool::new(false),
            armed_edges_mask: AtomicU8::new(0),
        }
    }

    #[inline]
    pub fn is_daemon_enabled(&self) -> bool {
        self.daemon_enabled.load(Ordering::Acquire)
    }

    pub fn set_daemon_enabled(&self, enabled: bool) {
        self.daemon_enabled.store(enabled, Ordering::Release);
        if !enabled {
            self.set_suppressing(false);
        }
    }

    #[inline]
    pub fn is_suppressing(&self) -> bool {
        self.suppressing_local.load(Ordering::Acquire)
    }

    pub fn set_suppressing(&self, suppress: bool) {
        self.suppressing_local.store(suppress, Ordering::Release);
        macos::set_local_cursor_associated(!suppress);
    }

    /// Called directly inside real-time OS input hook callbacks the instant the pointer crosses an armed edge,
    /// eliminating any async handoff window where local mouse/keyboard events could leak to Machine A.
    #[inline]
    pub fn set_suppressing_from_hook(&self, suppress: bool) {
        self.suppressing_local.store(suppress, Ordering::Release);
    }

    /// Sets armed screen edges `[up, down, left, right]`.
    pub fn set_armed_edges(&self, edges: [bool; 4]) {
        let mut mask = 0u8;
        if edges[0] {
            mask |= 1 << 0;
        }
        if edges[1] {
            mask |= 1 << 1;
        }
        if edges[2] {
            mask |= 1 << 2;
        }
        if edges[3] {
            mask |= 1 << 3;
        }
        self.armed_edges_mask.store(mask, Ordering::Release);
    }

    #[inline]
    pub fn armed_edges(&self) -> [bool; 4] {
        let m = self.armed_edges_mask.load(Ordering::Acquire);
        [
            (m & (1 << 0)) != 0,
            (m & (1 << 1)) != 0,
            (m & (1 << 2)) != 0,
            (m & (1 << 3)) != 0,
        ]
    }
}

pub trait InputCaptureBackend: Send {
    fn set_suppress_local_input(&mut self, suppress: bool) -> Result<(), PalError>;
    fn poll_raw_events(&mut self, out: &mut Vec<RawInputEvent>) -> Result<(), PalError>;
}

pub trait InputInjectorBackend: Send {
    fn inject_mouse_delta(&mut self, dx: i32, dy: i32) -> Result<(), PalError>;
    fn inject_mouse_warp_normalized(&mut self, norm_x: f32, norm_y: f32) -> Result<(), PalError>;
    fn inject_wheel_hires(&mut self, vertical_120: i32, horizontal_120: i32) -> Result<(), PalError>;
    fn inject_key_scancode(&mut self, scancode: u16, pressed: bool) -> Result<(), PalError>;
}

pub enum CrossPlatformInjector {
    MacOs(macos::MacOsCoreGraphicsDriver),
    Windows(windows::WindowsRawInputDriver),
    Linux(linux_wayland::LinuxDualStackDriver),
}

impl CrossPlatformInjector {
    pub fn initialize(strategy: LinuxWaylandStrategy) -> Result<Self, PalError> {
        if cfg!(target_os = "macos") {
            Ok(Self::MacOs(macos::MacOsCoreGraphicsDriver::initialize()?))
        } else if cfg!(target_os = "windows") {
            Ok(Self::Windows(windows::WindowsRawInputDriver::initialize()?))
        } else {
            Ok(Self::Linux(
                linux_wayland::LinuxDualStackDriver::initialize_sync(strategy)?,
            ))
        }
    }

    pub fn inject_mouse_delta(&mut self, dx: i32, dy: i32) -> Result<(), PalError> {
        match self {
            Self::MacOs(d) => d.inject_mouse_delta(dx, dy),
            Self::Windows(d) => d.inject_mouse_delta(dx, dy),
            Self::Linux(d) => d.inject_mouse_delta(dx, dy),
        }
    }

    pub fn inject_mouse_warp(&mut self, norm_x: f32, norm_y: f32) -> Result<(), PalError> {
        match self {
            Self::MacOs(d) => d.inject_mouse_warp_normalized(norm_x, norm_y),
            Self::Windows(d) => d.inject_mouse_warp_normalized(norm_x, norm_y),
            Self::Linux(d) => d.inject_mouse_warp_normalized(norm_x, norm_y),
        }
    }

    pub fn inject_mouse_button(&mut self, button: u8, pressed: bool) -> Result<(), PalError> {
        match self {
            Self::MacOs(d) => d.inject_mouse_button(button, pressed),
            Self::Windows(d) => d.inject_mouse_button(button, pressed),
            Self::Linux(d) => d.inject_mouse_button(button, pressed),
        }
    }

    pub fn inject_wheel(&mut self, vertical_120: i32, horizontal_120: i32) -> Result<(), PalError> {
        match self {
            Self::MacOs(d) => d.inject_wheel_hires(vertical_120, horizontal_120),
            Self::Windows(d) => d.inject_wheel_hires(vertical_120, horizontal_120),
            Self::Linux(d) => d.inject_wheel_hires(vertical_120, horizontal_120),
        }
    }

    pub fn inject_symbolic_key(&mut self, symbolic_key: &str, pressed: bool) -> Result<(), PalError> {
        match self {
            Self::MacOs(d) => d.inject_symbolic_key(symbolic_key, pressed),
            Self::Windows(d) => d.inject_symbolic_key(symbolic_key, pressed),
            Self::Linux(d) => d.inject_symbolic_key(symbolic_key, pressed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlatformDriverSummary {
    pub os_backend_name: &'static str,
    pub wayland_active: bool,
    pub requires_permission_prompt: bool,
    pub permission_granted: bool,
}

pub fn detect_local_os_kind() -> OsKind {
    if cfg!(target_os = "macos") {
        OsKind::MacOs
    } else if cfg!(target_os = "windows") {
        OsKind::Windows
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        OsKind::LinuxWayland
    } else {
        OsKind::LinuxX11
    }
}

pub fn probe_local_display() -> (Resolution, u16) {
    let (w, h, hz) = if cfg!(target_os = "macos") {
        macos::probe_macos_display()
    } else if cfg!(target_os = "windows") {
        windows::probe_windows_display()
    } else {
        linux_wayland::probe_linux_display()
    };
    (Resolution { width: w, height: h }, hz)
}

pub fn is_input_permission_granted() -> bool {
    if cfg!(target_os = "macos") {
        macos::is_accessibility_trusted()
    } else if cfg!(target_os = "linux") {
        linux_wayland::is_linux_input_permitted()
    } else {
        true
    }
}

pub fn prompt_input_permission() {
    if cfg!(target_os = "macos") {
        macos::request_accessibility_permission();
    }
}

pub fn spawn_platform_capture_thread(
    ctrl: Arc<CaptureControlState>,
    tx: UnboundedSender<CapturedInputEvent>,
) {
    if cfg!(target_os = "macos") {
        macos::spawn_macos_capture_thread(ctrl, tx);
    } else if cfg!(target_os = "windows") {
        windows::spawn_windows_capture_thread(ctrl, tx);
    } else {
        linux_wayland::spawn_linux_capture_thread(ctrl, tx);
    }
}

pub fn detect_active_driver_stack(strategy: LinuxWaylandStrategy) -> PlatformDriverSummary {
    let granted = is_input_permission_granted();
    if cfg!(target_os = "macos") {
        PlatformDriverSummary {
            os_backend_name: "macOS CoreGraphics CGEventTap + CGEventPost",
            wayland_active: false,
            requires_permission_prompt: true,
            permission_granted: granted,
        }
    } else if cfg!(target_os = "windows") {
        PlatformDriverSummary {
            os_backend_name: "Windows Win32 WH_MOUSE_LL / WH_KEYBOARD_LL + SendInput",
            wayland_active: false,
            requires_permission_prompt: false,
            permission_granted: true,
        }
    } else {
        let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        match (is_wayland, strategy) {
            (true, LinuxWaylandStrategy::PortalAndUinputFallback) => PlatformDriverSummary {
                os_backend_name: "Linux Wayland Portal (libei) + /dev/uinput 双栈兼容",
                wayland_active: true,
                requires_permission_prompt: true,
                permission_granted: granted,
            },
            (true, LinuxWaylandStrategy::PortalOnly) => PlatformDriverSummary {
                os_backend_name: "Linux Wayland XDG InputCapture/RemoteDesktop Portal (libei)",
                wayland_active: true,
                requires_permission_prompt: true,
                permission_granted: granted,
            },
            (false, _) => PlatformDriverSummary {
                os_backend_name: "Linux X11 XI2 + XTest",
                wayland_active: false,
                requires_permission_prompt: false,
                permission_granted: granted,
            },
        }
    }
}
