//! Cross-platform OS clipboard reader and writer (`arboard`).

use std::borrow::Cow;
use arboard::{Clipboard, ImageData};
use crate::PalError;

#[derive(Debug, Clone)]
pub struct RgbaClipboardImage {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

pub fn read_os_clipboard_text() -> Result<Option<String>, PalError> {
    let mut cb = Clipboard::new()
        .map_err(|e| PalError::DriverInitFailed(format!("Clipboard open failed: {e}")))?;
    match cb.get_text() {
        Ok(t) if !t.is_empty() => Ok(Some(t)),
        _ => Ok(None),
    }
}

pub fn write_os_clipboard_text(text: &str) -> Result<(), PalError> {
    let mut cb = Clipboard::new()
        .map_err(|e| PalError::DriverInitFailed(format!("Clipboard open failed: {e}")))?;
    cb.set_text(text.to_owned())
        .map_err(|e| PalError::InjectionFailed(format!("Clipboard set_text failed: {e}")))
}

pub fn read_os_clipboard_image() -> Result<Option<RgbaClipboardImage>, PalError> {
    let mut cb = Clipboard::new()
        .map_err(|e| PalError::DriverInitFailed(format!("Clipboard open failed: {e}")))?;
    match cb.get_image() {
        Ok(img) if img.width > 0 && img.height > 0 && !img.bytes.is_empty() => {
            Ok(Some(RgbaClipboardImage {
                width: img.width as u32,
                height: img.height as u32,
                bytes: img.bytes.into_owned(),
            }))
        }
        _ => Ok(None),
    }
}

pub fn write_os_clipboard_image(width: u32, height: u32, rgba: Vec<u8>) -> Result<(), PalError> {
    let mut cb = Clipboard::new()
        .map_err(|e| PalError::DriverInitFailed(format!("Clipboard open failed: {e}")))?;
    let img = ImageData {
        width: width as usize,
        height: height as usize,
        bytes: Cow::Owned(rgba),
    };
    cb.set_image(img)
        .map_err(|e| PalError::InjectionFailed(format!("Clipboard set_image failed: {e}")))
}
