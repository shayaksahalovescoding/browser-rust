use std::ptr::{null, null_mut};
use std::sync::Once;

#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[repr(C)]
struct GdiplusStartupInput {
    gdiplus_version: u32,
    debug_event_callback: *const u8,
    suppress_background_thread: i32,
    suppress_external_codecs: i32,
}

#[repr(C)]
struct GpRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[repr(C)]
struct BitmapData {
    width: u32,
    height: u32,
    stride: i32,
    pixel_format: i32,
    scan0: *mut u8,
    reserved: usize,
}

const GMEM_MOVEABLE: u32 = 0x0002;
const IMAGE_LOCK_MODE_READ: u32 = 1;
const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x26200A;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(u_flags: u32, dw_bytes: usize) -> isize;
    fn GlobalLock(h_mem: isize) -> *mut u8;
    fn GlobalUnlock(h_mem: isize) -> i32;
    fn GlobalFree(h_mem: isize) -> isize;
}

#[link(name = "ole32")]
unsafe extern "system" {
    fn CreateStreamOnHGlobal(
        h_global: isize,
        f_delete_on_release: i32,
        ppstm: *mut *mut u8,
    ) -> i32;
}

#[link(name = "gdiplus")]
unsafe extern "system" {
    fn GdiplusStartup(
        token: *mut usize,
        input: *const GdiplusStartupInput,
        output: *mut u8,
    ) -> i32;
    fn GdipCreateBitmapFromStream(stream: *mut u8, bitmap: *mut *mut u8) -> i32;
    fn GdipGetImageWidth(image: *mut u8, width: *mut u32) -> i32;
    fn GdipGetImageHeight(image: *mut u8, height: *mut u32) -> i32;
    fn GdipBitmapLockBits(
        bitmap: *mut u8,
        rect: *const GpRect,
        flags: u32,
        format: i32,
        locked_bitmap_data: *mut BitmapData,
    ) -> i32;
    fn GdipBitmapUnlockBits(bitmap: *mut u8, locked_bitmap_data: *mut BitmapData) -> i32;
    fn GdipDisposeImage(image: *mut u8) -> i32;
}

static GDIPLUS_INIT: Once = Once::new();
static mut GDIPLUS_TOKEN: usize = 0;

fn ensure_gdiplus() {
    GDIPLUS_INIT.call_once(|| {
        let input = GdiplusStartupInput {
            gdiplus_version: 1,
            debug_event_callback: null(),
            suppress_background_thread: 0,
            suppress_external_codecs: 0,
        };
        unsafe {
            let mut token = 0usize;
            if GdiplusStartup(&mut token, &input, null_mut()) == 0 {
                GDIPLUS_TOKEN = token;
            }
        }
    });
}

#[repr(C)]
struct IUnknownVtbl {
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(*mut u8) -> u32,
}

/// Decodes PNG, JPEG, GIF, BMP, TIFF, or ICO bytes into a BGRA8 image using Windows GDI+.
pub fn decode_image(bytes: &[u8]) -> Option<DecodedImage> {
    if bytes.is_empty() {
        return None;
    }
    ensure_gdiplus();

    unsafe {
        let hglob = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
        if hglob == 0 {
            return None;
        }
        let dst = GlobalLock(hglob);
        if dst.is_null() {
            GlobalFree(hglob);
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
        GlobalUnlock(hglob);

        let mut stream: *mut u8 = null_mut();
        if CreateStreamOnHGlobal(hglob, 1, &mut stream) != 0 || stream.is_null() {
            GlobalFree(hglob);
            return None;
        }

        let mut bitmap: *mut u8 = null_mut();
        let status = GdipCreateBitmapFromStream(stream, &mut bitmap);
        let vtbl = *(stream as *const *const IUnknownVtbl);
        ((*vtbl).release)(stream);

        if status != 0 || bitmap.is_null() {
            return None;
        }

        let mut w = 0u32;
        let mut h = 0u32;
        if GdipGetImageWidth(bitmap, &mut w) != 0
            || GdipGetImageHeight(bitmap, &mut h) != 0
            || w == 0
            || h == 0
            || w > 4096
            || h > 4096
        {
            GdipDisposeImage(bitmap);
            return None;
        }

        let rect = GpRect {
            x: 0,
            y: 0,
            width: w as i32,
            height: h as i32,
        };
        let mut data = BitmapData {
            width: 0,
            height: 0,
            stride: 0,
            pixel_format: 0,
            scan0: null_mut(),
            reserved: 0,
        };
        if GdipBitmapLockBits(
            bitmap,
            &rect,
            IMAGE_LOCK_MODE_READ,
            PIXEL_FORMAT_32BPP_ARGB,
            &mut data,
        ) != 0
            || data.scan0.is_null()
        {
            GdipDisposeImage(bitmap);
            return None;
        }

        let mut bgra = vec![0u8; (w as usize) * (h as usize) * 4];
        let row_bytes = (w as usize) * 4;
        for row in 0..(h as usize) {
            let src_row = if data.stride >= 0 {
                data.scan0.add(row * data.stride as usize)
            } else {
                data.scan0.sub(row * (-data.stride) as usize)
            };
            std::ptr::copy_nonoverlapping(
                src_row,
                bgra.as_mut_ptr().add(row * row_bytes),
                row_bytes,
            );
        }

        GdipBitmapUnlockBits(bitmap, &mut data);
        GdipDisposeImage(bitmap);
        Some(DecodedImage {
            width: w,
            height: h,
            bgra,
        })
    }
}

/// Decodes a `data:image/...;base64,<payload>` inline image URI via base64 + GDI+.
pub fn decode_data_uri(uri: &str) -> Option<DecodedImage> {
    let trimmed = uri.trim();
    if !trimmed.to_ascii_lowercase().starts_with("data:image/") {
        return None;
    }
    let (meta, data_part) = trimmed.split_once(',')?;
    if !meta.to_ascii_lowercase().contains(";base64") {
        return None;
    }
    let bytes = decode_base64(data_part)?;
    decode_image(&bytes)
}

pub fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity((input.len() * 3) / 4);
    let mut buf = 0u32;
    let mut bits = 0u8;
    for b in input.bytes() {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\r' | b'\n' | b'\t' => continue,
            _ => return None,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}
