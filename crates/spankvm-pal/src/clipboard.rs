//! Cross-platform OS clipboard engine (`spankvm-pal::clipboard`).
//!
//! Handles text and lossless screenshot/image synchronization across macOS, Windows, and Linux:
//! - Fast zero-copy change detection via `NSPasteboard.changeCount` (macOS) and
//!   `GetClipboardSequenceNumber` (Windows) so the clipboard is never locked or re-decoded while idle.
//! - Reads screenshots from Snipaste, macOS Screenshot, Windows `Win+Shift+S`, browsers, and copied
//!   image files (`public.png`, `public.tiff`, `PNG`, `CF_DIBV5`, `CF_DIB`, and image file URLs).
//! - Writes multi-format native image representations (`CF_DIB` `BI_RGB` + `CF_BITMAP` synthesis + `PNG`
//!   on Windows; `public.tiff` `NSImage` + `public.png` on macOS) after clearing stale text so apps like
//!   WeChat (`微信`), QQ, Feishu, DingTalk, Office, and Preview paste images cleanly just like RDP.

use std::borrow::Cow;
use std::path::Path;

use arboard::{Clipboard, ImageData};
use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder, ImageFormat};

use crate::PalError;

#[derive(Debug, Clone)]
pub struct RgbaClipboardImage {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum ClipboardSnapshot {
    Image {
        width: u32,
        height: u32,
        png_bytes: Vec<u8>,
        blake3_hex: String,
    },
    Text {
        text: String,
        blake3_hex: String,
    },
    Empty,
}

/// Computes a canonical format-independent digest over RGB pixels + alpha so OS container
/// conversions (`PNG <-> CF_DIB <-> TIFF`) never trigger echo loops.
pub fn compute_rgba_canonical_blake3(width: u32, height: u32, rgba: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"spankvm-img-v2:");
    hasher.update(&width.to_le_bytes());
    hasher.update(&height.to_le_bytes());
    // Hash RGB channels (since Win32 BI_RGB CF_DIB synthesis may normalize opaque alpha bytes)
    for px in rgba.chunks_exact(4) {
        hasher.update(&px[0..3]);
    }
    hasher.finalize().to_hex().to_string()
}

pub fn encode_rgba_to_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, PalError> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| PalError::InjectionFailed("Invalid image dimensions".into()))?;
    if rgba.len() < expected {
        return Err(PalError::InjectionFailed("Truncated RGBA buffer".into()));
    }
    let mut png_buf = Vec::with_capacity((expected / 6).max(1024));
    let encoder = PngEncoder::new(&mut png_buf);
    encoder
        .write_image(&rgba[..expected], width, height, ColorType::Rgba8.into())
        .map_err(|e| PalError::InjectionFailed(format!("PNG encode failed: {e}")))?;
    Ok(png_buf)
}

pub fn decode_png_to_rgba(png_bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), PalError> {
    let dyn_img = image::load_from_memory_with_format(png_bytes, ImageFormat::Png)
        .or_else(|_| image::load_from_memory(png_bytes))
        .map_err(|e| PalError::InjectionFailed(format!("Image decode failed: {e}")))?;
    let rgba = dyn_img.into_rgba8();
    let (w, h) = rgba.dimensions();
    Ok((w, h, rgba.into_raw()))
}

/// Returns the lightweight OS clipboard sequence/change counter without locking or decoding data.
pub fn os_clipboard_sequence_token() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        use objc2::rc::autoreleasepool;
        use objc2_app_kit::NSPasteboard;
        return Some(autoreleasepool(|_| {
            let pb = NSPasteboard::generalPasteboard();
            pb.changeCount() as u64
        }));
    }
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
        let seq = unsafe { GetClipboardSequenceNumber() };
        return if seq == 0 { None } else { Some(seq as u64) };
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

fn is_supported_image_file_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "tiff" | "tif" | "gif"
    )
}

#[cfg(target_os = "macos")]
fn macos_pasteboard_prefers_image() -> bool {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSPasteboard;
    autoreleasepool(|_| {
        let pb = NSPasteboard::generalPasteboard();
        let Some(types) = pb.types() else {
            return false;
        };
        let mut img_rank: Option<usize> = None;
        let mut txt_rank: Option<usize> = None;
        for (idx, uti) in types.iter().enumerate() {
            let s = uti.to_string();
            if img_rank.is_none()
                && (s == "public.png"
                    || s == "public.tiff"
                    || s == "public.jpeg"
                    || s == "com.apple.pict"
                    || s.contains("image"))
            {
                img_rank = Some(idx);
            }
            if txt_rank.is_none() && (s == "public.utf8-plain-text" || s == "NSStringPboardType") {
                txt_rank = Some(idx);
            }
        }
        match (img_rank, txt_rank) {
            (Some(_), None) => true,
            (Some(i), Some(t)) => i <= t,
            _ => false,
        }
    })
}

#[cfg(target_os = "macos")]
fn macos_try_read_public_png() -> Option<(u32, u32, Vec<u8>, Vec<u8>)> {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::ns_string;

    let raw_png: Vec<u8> = autoreleasepool(|_| unsafe {
        let pb = NSPasteboard::generalPasteboard();
        let data = pb.dataForType(ns_string!("public.png"))?;
        let slice = data.as_bytes_unchecked();
        if slice.is_empty() {
            None
        } else {
            Some(slice.to_vec())
        }
    })?;

    let (w, h, rgba) = decode_png_to_rgba(&raw_png).ok()?;
    if w == 0 || h == 0 {
        return None;
    }
    Some((w, h, raw_png, rgba))
}

#[cfg(target_os = "macos")]
fn macos_attach_public_png(png_bytes: &[u8]) {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{ns_string, NSData};

    autoreleasepool(|_| {
        let pb = NSPasteboard::generalPasteboard();
        let ns_data = NSData::with_bytes(png_bytes);
        let _ = pb.setData_forType(Some(&ns_data), ns_string!("public.png"));
    });
}

#[cfg(target_os = "windows")]
fn windows_clipboard_prefers_image() -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EnumClipboardFormats, OpenClipboard, RegisterClipboardFormatW,
    };
    use windows::core::w;

    const CF_BITMAP_ID: u32 = 2;
    const CF_DIB_ID: u32 = 8;
    const CF_UNICODETEXT_ID: u32 = 13;
    const CF_DIBV5_ID: u32 = 17;

    unsafe {
        let png_fmt = RegisterClipboardFormatW(w!("PNG"));
        if OpenClipboard(HWND::default()).is_err() {
            return false;
        }
        let mut img_rank: Option<usize> = None;
        let mut txt_rank: Option<usize> = None;
        let mut cur = 0u32;
        let mut idx = 0usize;
        loop {
            cur = EnumClipboardFormats(cur);
            if cur == 0 {
                break;
            }
            if img_rank.is_none()
                && (cur == png_fmt
                    || cur == CF_DIB_ID
                    || cur == CF_DIBV5_ID
                    || cur == CF_BITMAP_ID)
            {
                img_rank = Some(idx);
            }
            if txt_rank.is_none() && cur == CF_UNICODETEXT_ID {
                txt_rank = Some(idx);
            }
            idx += 1;
        }
        let _ = CloseClipboard();
        match (img_rank, txt_rank) {
            (Some(_), None) => true,
            (Some(i), Some(t)) => i <= t,
            _ => false,
        }
    }
}

/// Writes both standard 40-byte `CF_DIB` (`BI_RGB`, from which Windows natively synthesizes
/// `CF_BITMAP` and `CF_DIBV5` for WeChat/QQ/Office/Paint) AND registered `"PNG"` format after
/// calling `EmptyClipboard()` so zero stale text remains on the Windows clipboard.
#[cfg(target_os = "windows")]
fn windows_write_dib_and_png(width: u32, height: u32, rgba: &[u8], png_bytes: &[u8]) -> Result<(), PalError> {
    use std::mem::size_of;
    use std::ptr::copy_nonoverlapping;
    use windows::core::w;
    use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
    use windows::Win32::Graphics::Gdi::{BITMAPINFOHEADER, BI_RGB};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    const CF_DIB_ID: u32 = 8;

    let pixel_bytes_len = (width as usize) * (height as usize) * 4;
    if rgba.len() < pixel_bytes_len {
        return Err(PalError::InjectionFailed("RGBA buffer too short".into()));
    }

    // Build bottom-up BGRA buffer for standard 32-bit BI_RGB CF_DIB
    let mut bgra_bottom_up = vec![0u8; pixel_bytes_len];
    let row_bytes = (width as usize) * 4;
    for y in 0..(height as usize) {
        let src_row = &rgba[y * row_bytes..(y + 1) * row_bytes];
        let dst_y = (height as usize) - 1 - y;
        let dst_row = &mut bgra_bottom_up[dst_y * row_bytes..(dst_y + 1) * row_bytes];
        for (src_px, dst_px) in src_row.chunks_exact(4).zip(dst_row.chunks_exact_mut(4)) {
            dst_px[0] = src_px[2]; // B
            dst_px[1] = src_px[1]; // G
            dst_px[2] = src_px[0]; // R
            dst_px[3] = src_px[3]; // A
        }
    }

    let bmi = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width as i32,
        biHeight: height as i32, // positive = standard bottom-up DIB required by WeChat/RichEdit/Word
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: pixel_bytes_len as u32,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };

    unsafe {
        let mut opened = false;
        for _ in 0..8 {
            if OpenClipboard(HWND::default()).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        if !opened {
            return Err(PalError::InjectionFailed("OpenClipboard busy".into()));
        }

        let _ = EmptyClipboard();

        // 1. Place CF_DIB (8) so Windows GDI synthesizes CF_BITMAP (2) & CF_DIBV5 (17) automatically
        let dib_total = size_of::<BITMAPINFOHEADER>() + bgra_bottom_up.len();
        if let Ok(hdib) = GlobalAlloc(GMEM_MOVEABLE, dib_total) {
            let ptr = GlobalLock(hdib) as *mut u8;
            if !ptr.is_null() {
                copy_nonoverlapping(
                    (&bmi as *const BITMAPINFOHEADER).cast::<u8>(),
                    ptr,
                    size_of::<BITMAPINFOHEADER>(),
                );
                copy_nonoverlapping(
                    bgra_bottom_up.as_ptr(),
                    ptr.add(size_of::<BITMAPINFOHEADER>()),
                    bgra_bottom_up.len(),
                );
                let _ = GlobalUnlock(hdib);
                let _ = SetClipboardData(CF_DIB_ID, HANDLE(hdib.0));
            }
        }

        // 2. Also place lossless "PNG" format on the same clipboard for modern Chromium/Electron apps
        let png_fmt = RegisterClipboardFormatW(w!("PNG"));
        if png_fmt != 0 && !png_bytes.is_empty() {
            if let Ok(hpng) = GlobalAlloc(GMEM_MOVEABLE, png_bytes.len()) {
                let ptr = GlobalLock(hpng) as *mut u8;
                if !ptr.is_null() {
                    copy_nonoverlapping(png_bytes.as_ptr(), ptr, png_bytes.len());
                    let _ = GlobalUnlock(hpng);
                    let _ = SetClipboardData(png_fmt, HANDLE(hpng.0));
                }
            }
        }
        let _ = HGLOBAL::default();

        let _ = CloseClipboard();
    }
    Ok(())
}

fn try_read_clipboard_image_any(cb: &mut Clipboard) -> Option<(u32, u32, Vec<u8>, Vec<u8>)> {
    // 1. On macOS, check `public.png` first (Snipaste for Mac / Qt / Electron apps often write PNG directly)
    #[cfg(target_os = "macos")]
    if let Some(found) = macos_try_read_public_png() {
        return Some(found);
    }

    // 2. Standard arboard bitmap/PNG/TIFF/Wayland reader
    if let Ok(img) = cb.get_image() {
        if img.width > 0 && img.height > 0 && !img.bytes.is_empty() {
            let w = img.width as u32;
            let h = img.height as u32;
            let rgba = img.bytes.into_owned();
            if let Ok(png_bytes) = encode_rgba_to_png(w, h, &rgba) {
                return Some((w, h, png_bytes, rgba));
            }
        }
    }

    // 3. Single copied image file on clipboard (e.g. Finder / Explorer / screenshot file URL)
    if let Ok(files) = cb.get().file_list() {
        if files.len() == 1 && is_supported_image_file_path(&files[0]) {
            if let Ok(meta) = std::fs::metadata(&files[0]) {
                if meta.is_file() && meta.len() > 0 && meta.len() <= 32 * 1024 * 1024 {
                    if let Ok(raw_file) = std::fs::read(&files[0]) {
                        if let Ok(dyn_img) = image::load_from_memory(&raw_file) {
                            let rgba = dyn_img.into_rgba8();
                            let (w, h) = rgba.dimensions();
                            let raw_rgba = rgba.into_raw();
                            if let Ok(png_bytes) = encode_rgba_to_png(w, h, &raw_rgba) {
                                return Some((w, h, png_bytes, raw_rgba));
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Atomically inspects the OS clipboard and returns either an `Image` (prioritized when screenshots
/// or copied image files are on the clipboard) or `Text`.
pub fn read_os_clipboard_snapshot(
    allow_text: bool,
    allow_images: bool,
) -> Result<ClipboardSnapshot, PalError> {
    let mut cb = Clipboard::new()
        .map_err(|e| PalError::DriverInitFailed(format!("Clipboard open failed: {e}")))?;

    let image_preferred = {
        #[cfg(target_os = "macos")]
        {
            macos_pasteboard_prefers_image()
        }
        #[cfg(target_os = "windows")]
        {
            windows_clipboard_prefers_image()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            true
        }
    };

    // Check if clipboard holds a single copied image file (which also populates filename text on macOS)
    let has_single_image_file = cb
        .get()
        .file_list()
        .map(|f| f.len() == 1 && is_supported_image_file_path(&f[0]))
        .unwrap_or(false);

    if allow_images && (image_preferred || has_single_image_file) {
        if let Some((width, height, png_bytes, rgba)) = try_read_clipboard_image_any(&mut cb) {
            let blake3_hex = compute_rgba_canonical_blake3(width, height, &rgba);
            return Ok(ClipboardSnapshot::Image {
                width,
                height,
                png_bytes,
                blake3_hex,
            });
        }
    }

    if allow_text {
        if let Ok(text) = cb.get_text() {
            if !text.is_empty() {
                let blake3_hex = blake3::hash(text.as_bytes()).to_hex().to_string();
                return Ok(ClipboardSnapshot::Text { text, blake3_hex });
            }
        }
    }

    if allow_images {
        if let Some((width, height, png_bytes, rgba)) = try_read_clipboard_image_any(&mut cb) {
            let blake3_hex = compute_rgba_canonical_blake3(width, height, &rgba);
            return Ok(ClipboardSnapshot::Image {
                width,
                height,
                png_bytes,
                blake3_hex,
            });
        }
    }

    Ok(ClipboardSnapshot::Empty)
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
    if let Some((width, height, _png, bytes)) = try_read_clipboard_image_any(&mut cb) {
        return Ok(Some(RgbaClipboardImage {
            width,
            height,
            bytes,
        }));
    }
    Ok(None)
}

pub fn write_os_clipboard_image(width: u32, height: u32, rgba: Vec<u8>) -> Result<(), PalError> {
    let png_bytes = encode_rgba_to_png(width, height, &rgba)?;
    let _ = write_os_clipboard_png_or_rgba(width, height, Some(&png_bytes), Some(&rgba))?;
    Ok(())
}

/// Writes an incoming remote screenshot/image to the local OS clipboard in native multi-format
/// (`CF_DIB` + `CF_BITMAP` + `PNG` on Windows; `NSImage` TIFF + `public.png` on macOS) and returns
/// `(width, height, canonical_blake3_hex, png_byte_len)`.
pub fn write_os_clipboard_png_or_rgba(
    width: u32,
    height: u32,
    png_bytes_opt: Option<&[u8]>,
    rgba_opt: Option<&[u8]>,
) -> Result<(u32, u32, String, usize), PalError> {
    let (w, h, rgba, png_bytes) = if let Some(png) = png_bytes_opt.filter(|b| !b.is_empty()) {
        let (dw, dh, decoded_rgba) = decode_png_to_rgba(png)?;
        (dw, dh, decoded_rgba, png.to_vec())
    } else if let Some(raw_rgba) = rgba_opt.filter(|b| !b.is_empty()) {
        let encoded_png = encode_rgba_to_png(width, height, raw_rgba)?;
        (width, height, raw_rgba.to_vec(), encoded_png)
    } else {
        return Err(PalError::InjectionFailed("Empty clipboard image payload".into()));
    };

    let canonical_hash = compute_rgba_canonical_blake3(w, h, &rgba);
    let png_len = png_bytes.len();

    #[cfg(target_os = "windows")]
    {
        if windows_write_dib_and_png(w, h, &rgba, &png_bytes).is_ok() {
            return Ok((w, h, canonical_hash, png_len));
        }
    }

    let mut cb = Clipboard::new()
        .map_err(|e| PalError::DriverInitFailed(format!("Clipboard open failed: {e}")))?;
    let img = ImageData {
        width: w as usize,
        height: h as usize,
        bytes: Cow::Owned(rgba),
    };
    cb.set_image(img)
        .map_err(|e| PalError::InjectionFailed(format!("Clipboard set_image failed: {e}")))?;

    #[cfg(target_os = "macos")]
    {
        macos_attach_public_png(&png_bytes);
    }

    Ok((w, h, canonical_hash, png_len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_codec_and_canonical_hash_roundtrip_cleanly() {
        let width = 4u32;
        let height = 3u32;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for i in 0..(width * height) {
            rgba.extend_from_slice(&[(i * 17) as u8, (i * 31) as u8, (i * 47) as u8, 255u8]);
        }

        let hash_before = compute_rgba_canonical_blake3(width, height, &rgba);
        let png = encode_rgba_to_png(width, height, &rgba).expect("encode PNG");
        assert!(!png.is_empty());

        let (dw, dh, decoded_rgba) = decode_png_to_rgba(&png).expect("decode PNG");
        assert_eq!((dw, dh), (width, height));
        assert_eq!(decoded_rgba, rgba);

        let hash_after = compute_rgba_canonical_blake3(dw, dh, &decoded_rgba);
        assert_eq!(hash_before, hash_after);
    }
}
