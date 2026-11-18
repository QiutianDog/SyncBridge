//! Canonical cross-OS keyboard keycode translation table.
//!
//! Bridges macOS Quartz `CGKeyCode`, Windows Win32 `(VK, ScanCode)`,
//! and Linux `evdev` keycode (where X11 XKB keycode = `evdev + 8`).

#[derive(Debug, Clone, Copy)]
pub struct CrossOsKeyEntry {
    pub canonical: &'static str,
    pub macos_cg: u16,
    pub win_vk: u16,
    pub win_scan: u16,
    pub linux_evdev: u16,
}

pub const KEY_TABLE: &[CrossOsKeyEntry] = &[
    // Modifiers (matches UI key remapping options directly)
    CrossOsKeyEntry { canonical: "Command (⌘)", macos_cg: 0x37, win_vk: 0x5B, win_scan: 0x5B, linux_evdev: 125 },
    CrossOsKeyEntry { canonical: "Win / Super", macos_cg: 0x37, win_vk: 0x5B, win_scan: 0x5B, linux_evdev: 125 },
    CrossOsKeyEntry { canonical: "RightSuper", macos_cg: 0x36, win_vk: 0x5C, win_scan: 0x5C, linux_evdev: 126 },
    CrossOsKeyEntry { canonical: "Control (⌃)", macos_cg: 0x3B, win_vk: 0xA2, win_scan: 0x1D, linux_evdev: 29 },
    CrossOsKeyEntry { canonical: "Ctrl", macos_cg: 0x3B, win_vk: 0xA2, win_scan: 0x1D, linux_evdev: 29 },
    CrossOsKeyEntry { canonical: "RightCtrl", macos_cg: 0x3E, win_vk: 0xA3, win_scan: 0x1D, linux_evdev: 97 },
    CrossOsKeyEntry { canonical: "Option (⌥)", macos_cg: 0x3A, win_vk: 0xA4, win_scan: 0x38, linux_evdev: 56 },
    CrossOsKeyEntry { canonical: "Alt", macos_cg: 0x3A, win_vk: 0xA4, win_scan: 0x38, linux_evdev: 56 },
    CrossOsKeyEntry { canonical: "RightAlt", macos_cg: 0x3D, win_vk: 0xA5, win_scan: 0x38, linux_evdev: 100 },
    CrossOsKeyEntry { canonical: "Shift", macos_cg: 0x38, win_vk: 0xA0, win_scan: 0x2A, linux_evdev: 42 },
    CrossOsKeyEntry { canonical: "RightShift", macos_cg: 0x3C, win_vk: 0xA1, win_scan: 0x36, linux_evdev: 54 },
    CrossOsKeyEntry { canonical: "CapsLock", macos_cg: 0x39, win_vk: 0x14, win_scan: 0x3A, linux_evdev: 58 },

    // Navigation & Editing
    CrossOsKeyEntry { canonical: "Escape", macos_cg: 0x35, win_vk: 0x1B, win_scan: 0x01, linux_evdev: 1 },
    CrossOsKeyEntry { canonical: "Enter", macos_cg: 0x24, win_vk: 0x0D, win_scan: 0x1C, linux_evdev: 28 },
    CrossOsKeyEntry { canonical: "Tab", macos_cg: 0x30, win_vk: 0x09, win_scan: 0x0F, linux_evdev: 15 },
    CrossOsKeyEntry { canonical: "Space", macos_cg: 0x31, win_vk: 0x20, win_scan: 0x39, linux_evdev: 57 },
    CrossOsKeyEntry { canonical: "Backspace", macos_cg: 0x33, win_vk: 0x08, win_scan: 0x0E, linux_evdev: 14 },
    CrossOsKeyEntry { canonical: "Delete", macos_cg: 0x75, win_vk: 0x2E, win_scan: 0x53, linux_evdev: 111 },
    CrossOsKeyEntry { canonical: "PrintScreen", macos_cg: 0x69, win_vk: 0x2C, win_scan: 0x37, linux_evdev: 99 },
    CrossOsKeyEntry { canonical: "Home", macos_cg: 0x73, win_vk: 0x24, win_scan: 0x47, linux_evdev: 102 },
    CrossOsKeyEntry { canonical: "End", macos_cg: 0x77, win_vk: 0x23, win_scan: 0x4F, linux_evdev: 107 },
    CrossOsKeyEntry { canonical: "PageUp", macos_cg: 0x74, win_vk: 0x21, win_scan: 0x49, linux_evdev: 104 },
    CrossOsKeyEntry { canonical: "PageDown", macos_cg: 0x79, win_vk: 0x22, win_scan: 0x51, linux_evdev: 109 },
    CrossOsKeyEntry { canonical: "ArrowLeft", macos_cg: 0x7B, win_vk: 0x25, win_scan: 0x4B, linux_evdev: 105 },
    CrossOsKeyEntry { canonical: "ArrowRight", macos_cg: 0x7C, win_vk: 0x27, win_scan: 0x4D, linux_evdev: 106 },
    CrossOsKeyEntry { canonical: "ArrowDown", macos_cg: 0x7D, win_vk: 0x28, win_scan: 0x50, linux_evdev: 108 },
    CrossOsKeyEntry { canonical: "ArrowUp", macos_cg: 0x7E, win_vk: 0x26, win_scan: 0x48, linux_evdev: 103 },

    // Letters A-Z
    CrossOsKeyEntry { canonical: "KeyA", macos_cg: 0x00, win_vk: 0x41, win_scan: 0x1E, linux_evdev: 30 },
    CrossOsKeyEntry { canonical: "KeyB", macos_cg: 0x0B, win_vk: 0x42, win_scan: 0x30, linux_evdev: 48 },
    CrossOsKeyEntry { canonical: "KeyC", macos_cg: 0x08, win_vk: 0x43, win_scan: 0x2E, linux_evdev: 46 },
    CrossOsKeyEntry { canonical: "KeyD", macos_cg: 0x02, win_vk: 0x44, win_scan: 0x20, linux_evdev: 32 },
    CrossOsKeyEntry { canonical: "KeyE", macos_cg: 0x0E, win_vk: 0x45, win_scan: 0x12, linux_evdev: 18 },
    CrossOsKeyEntry { canonical: "KeyF", macos_cg: 0x03, win_vk: 0x46, win_scan: 0x21, linux_evdev: 33 },
    CrossOsKeyEntry { canonical: "KeyG", macos_cg: 0x05, win_vk: 0x47, win_scan: 0x22, linux_evdev: 34 },
    CrossOsKeyEntry { canonical: "KeyH", macos_cg: 0x04, win_vk: 0x48, win_scan: 0x23, linux_evdev: 35 },
    CrossOsKeyEntry { canonical: "KeyI", macos_cg: 0x22, win_vk: 0x49, win_scan: 0x17, linux_evdev: 23 },
    CrossOsKeyEntry { canonical: "KeyJ", macos_cg: 0x26, win_vk: 0x4A, win_scan: 0x24, linux_evdev: 36 },
    CrossOsKeyEntry { canonical: "KeyK", macos_cg: 0x28, win_vk: 0x4B, win_scan: 0x25, linux_evdev: 37 },
    CrossOsKeyEntry { canonical: "KeyL", macos_cg: 0x25, win_vk: 0x4C, win_scan: 0x26, linux_evdev: 38 },
    CrossOsKeyEntry { canonical: "KeyM", macos_cg: 0x2E, win_vk: 0x4D, win_scan: 0x32, linux_evdev: 50 },
    CrossOsKeyEntry { canonical: "KeyN", macos_cg: 0x2D, win_vk: 0x4E, win_scan: 0x31, linux_evdev: 49 },
    CrossOsKeyEntry { canonical: "KeyO", macos_cg: 0x1F, win_vk: 0x4F, win_scan: 0x18, linux_evdev: 24 },
    CrossOsKeyEntry { canonical: "KeyP", macos_cg: 0x23, win_vk: 0x50, win_scan: 0x19, linux_evdev: 25 },
    CrossOsKeyEntry { canonical: "KeyQ", macos_cg: 0x0C, win_vk: 0x51, win_scan: 0x10, linux_evdev: 16 },
    CrossOsKeyEntry { canonical: "KeyR", macos_cg: 0x0F, win_vk: 0x52, win_scan: 0x13, linux_evdev: 19 },
    CrossOsKeyEntry { canonical: "KeyS", macos_cg: 0x01, win_vk: 0x53, win_scan: 0x1F, linux_evdev: 31 },
    CrossOsKeyEntry { canonical: "KeyT", macos_cg: 0x11, win_vk: 0x54, win_scan: 0x14, linux_evdev: 20 },
    CrossOsKeyEntry { canonical: "KeyU", macos_cg: 0x20, win_vk: 0x55, win_scan: 0x16, linux_evdev: 22 },
    CrossOsKeyEntry { canonical: "KeyV", macos_cg: 0x09, win_vk: 0x56, win_scan: 0x2F, linux_evdev: 47 },
    CrossOsKeyEntry { canonical: "KeyW", macos_cg: 0x0D, win_vk: 0x57, win_scan: 0x11, linux_evdev: 17 },
    CrossOsKeyEntry { canonical: "KeyX", macos_cg: 0x07, win_vk: 0x58, win_scan: 0x2D, linux_evdev: 45 },
    CrossOsKeyEntry { canonical: "KeyY", macos_cg: 0x10, win_vk: 0x59, win_scan: 0x15, linux_evdev: 21 },
    CrossOsKeyEntry { canonical: "KeyZ", macos_cg: 0x06, win_vk: 0x5A, win_scan: 0x2C, linux_evdev: 44 },

    // Digits 0-9
    CrossOsKeyEntry { canonical: "Digit1", macos_cg: 0x12, win_vk: 0x31, win_scan: 0x02, linux_evdev: 2 },
    CrossOsKeyEntry { canonical: "Digit2", macos_cg: 0x13, win_vk: 0x32, win_scan: 0x03, linux_evdev: 3 },
    CrossOsKeyEntry { canonical: "Digit3", macos_cg: 0x14, win_vk: 0x33, win_scan: 0x04, linux_evdev: 4 },
    CrossOsKeyEntry { canonical: "Digit4", macos_cg: 0x15, win_vk: 0x34, win_scan: 0x05, linux_evdev: 5 },
    CrossOsKeyEntry { canonical: "Digit5", macos_cg: 0x17, win_vk: 0x35, win_scan: 0x06, linux_evdev: 6 },
    CrossOsKeyEntry { canonical: "Digit6", macos_cg: 0x16, win_vk: 0x36, win_scan: 0x07, linux_evdev: 7 },
    CrossOsKeyEntry { canonical: "Digit7", macos_cg: 0x1A, win_vk: 0x37, win_scan: 0x08, linux_evdev: 8 },
    CrossOsKeyEntry { canonical: "Digit8", macos_cg: 0x1C, win_vk: 0x38, win_scan: 0x09, linux_evdev: 9 },
    CrossOsKeyEntry { canonical: "Digit9", macos_cg: 0x19, win_vk: 0x39, win_scan: 0x0A, linux_evdev: 10 },
    CrossOsKeyEntry { canonical: "Digit0", macos_cg: 0x1D, win_vk: 0x30, win_scan: 0x0B, linux_evdev: 11 },

    // Punctuation
    CrossOsKeyEntry { canonical: "Minus", macos_cg: 0x1B, win_vk: 0xBD, win_scan: 0x0C, linux_evdev: 12 },
    CrossOsKeyEntry { canonical: "Equal", macos_cg: 0x18, win_vk: 0xBB, win_scan: 0x0D, linux_evdev: 13 },
    CrossOsKeyEntry { canonical: "BracketLeft", macos_cg: 0x21, win_vk: 0xDB, win_scan: 0x1A, linux_evdev: 26 },
    CrossOsKeyEntry { canonical: "BracketRight", macos_cg: 0x1E, win_vk: 0xDD, win_scan: 0x1B, linux_evdev: 27 },
    CrossOsKeyEntry { canonical: "Backslash", macos_cg: 0x2A, win_vk: 0xDC, win_scan: 0x2B, linux_evdev: 43 },
    CrossOsKeyEntry { canonical: "Semicolon", macos_cg: 0x29, win_vk: 0xBA, win_scan: 0x27, linux_evdev: 39 },
    CrossOsKeyEntry { canonical: "Quote", macos_cg: 0x27, win_vk: 0xDE, win_scan: 0x28, linux_evdev: 40 },
    CrossOsKeyEntry { canonical: "Backquote", macos_cg: 0x32, win_vk: 0xC0, win_scan: 0x29, linux_evdev: 41 },
    CrossOsKeyEntry { canonical: "Comma", macos_cg: 0x2B, win_vk: 0xBC, win_scan: 0x33, linux_evdev: 51 },
    CrossOsKeyEntry { canonical: "Period", macos_cg: 0x2F, win_vk: 0xBE, win_scan: 0x34, linux_evdev: 52 },
    CrossOsKeyEntry { canonical: "Slash", macos_cg: 0x2C, win_vk: 0xBF, win_scan: 0x35, linux_evdev: 53 },

    // Function Keys
    CrossOsKeyEntry { canonical: "F1", macos_cg: 0x7A, win_vk: 0x70, win_scan: 0x3B, linux_evdev: 59 },
    CrossOsKeyEntry { canonical: "F2", macos_cg: 0x78, win_vk: 0x71, win_scan: 0x3C, linux_evdev: 60 },
    CrossOsKeyEntry { canonical: "F3", macos_cg: 0x63, win_vk: 0x72, win_scan: 0x3D, linux_evdev: 61 },
    CrossOsKeyEntry { canonical: "F4", macos_cg: 0x76, win_vk: 0x73, win_scan: 0x3E, linux_evdev: 62 },
    CrossOsKeyEntry { canonical: "F5", macos_cg: 0x60, win_vk: 0x74, win_scan: 0x3F, linux_evdev: 63 },
    CrossOsKeyEntry { canonical: "F6", macos_cg: 0x61, win_vk: 0x75, win_scan: 0x40, linux_evdev: 64 },
    CrossOsKeyEntry { canonical: "F7", macos_cg: 0x62, win_vk: 0x76, win_scan: 0x41, linux_evdev: 65 },
    CrossOsKeyEntry { canonical: "F8", macos_cg: 0x64, win_vk: 0x77, win_scan: 0x42, linux_evdev: 66 },
    CrossOsKeyEntry { canonical: "F9", macos_cg: 0x65, win_vk: 0x78, win_scan: 0x43, linux_evdev: 67 },
    CrossOsKeyEntry { canonical: "F10", macos_cg: 0x6D, win_vk: 0x79, win_scan: 0x44, linux_evdev: 68 },
    CrossOsKeyEntry { canonical: "F11", macos_cg: 0x67, win_vk: 0x7A, win_scan: 0x57, linux_evdev: 87 },
    CrossOsKeyEntry { canonical: "F12", macos_cg: 0x6F, win_vk: 0x7B, win_scan: 0x58, linux_evdev: 88 },
    CrossOsKeyEntry { canonical: "F13", macos_cg: 0x69, win_vk: 0x7C, win_scan: 0x64, linux_evdev: 183 },
    CrossOsKeyEntry { canonical: "F14", macos_cg: 0x6B, win_vk: 0x7D, win_scan: 0x65, linux_evdev: 184 },
];

pub fn lookup_by_canonical(name: &str) -> Option<&'static CrossOsKeyEntry> {
    KEY_TABLE
        .iter()
        .find(|e| e.canonical.eq_ignore_ascii_case(name))
}

pub fn canonical_from_macos_cg(cg: u16) -> Option<&'static CrossOsKeyEntry> {
    KEY_TABLE.iter().find(|e| e.macos_cg == cg)
}

pub fn canonical_from_windows_vk(vk: u16) -> Option<&'static CrossOsKeyEntry> {
    let normalized_vk = match vk {
        0x11 => 0xA2, // VK_CONTROL -> VK_LCONTROL
        0x12 => 0xA4, // VK_MENU -> VK_LMENU
        0x10 => 0xA0, // VK_SHIFT -> VK_LSHIFT
        other => other,
    };
    KEY_TABLE.iter().find(|e| e.win_vk == normalized_vk)
}

pub fn canonical_from_linux_evdev(code: u16) -> Option<&'static CrossOsKeyEntry> {
    KEY_TABLE.iter().find(|e| e.linux_evdev == code)
}
