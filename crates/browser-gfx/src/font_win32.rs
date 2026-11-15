use std::cell::RefCell;
use std::collections::HashMap;
use std::ptr::{null, null_mut};

use crate::canvas::{CanvasView, Rect};
use crate::color::Color;

const BI_RGB: u32 = 0;
const DIB_RGB_COLORS: u32 = 0;
const TRANSPARENT: i32 = 1;
const DEFAULT_CHARSET: u32 = 1;
const OUT_DEFAULT_PRECIS: u32 = 0;
const CLIP_DEFAULT_PRECIS: u32 = 0;
const ANTIALIASED_QUALITY: u32 = 4;
const DEFAULT_PITCH: u32 = 0;
const FW_NORMAL: i32 = 400;
const FW_SEMIBOLD: i32 = 600;
const FW_BOLD: i32 = 700;

#[repr(C)]
struct BitmapInfoHeader {
    bi_size: u32,
    bi_width: i32,
    bi_height: i32,
    bi_planes: u16,
    bi_bit_count: u16,
    bi_compression: u32,
    bi_size_image: u32,
    bi_x_pels_per_meter: i32,
    bi_y_pels_per_meter: i32,
    bi_clr_used: u32,
    bi_clr_important: u32,
}

#[repr(C)]
struct BitmapInfo {
    bmi_header: BitmapInfoHeader,
    bmi_colors: [u32; 1],
}

#[repr(C)]
#[derive(Default)]
struct SizeWin {
    cx: i32,
    cy: i32,
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateCompatibleDC(hdc: isize) -> isize;
    fn DeleteDC(hdc: isize) -> i32;
    fn CreateDIBSection(
        hdc: isize,
        pbmi: *const BitmapInfo,
        usage: u32,
        ppv_bits: *mut *mut u8,
        h_section: isize,
        offset: u32,
    ) -> isize;
    fn SelectObject(hdc: isize, h: isize) -> isize;
    fn DeleteObject(ho: isize) -> i32;
    fn CreateFontW(
        c_height: i32,
        c_width: i32,
        c_escapement: i32,
        c_orientation: i32,
        c_weight: i32,
        b_italic: u32,
        b_underline: u32,
        b_strike_out: u32,
        i_char_set: u32,
        i_out_precision: u32,
        i_clip_precision: u32,
        i_quality: u32,
        i_pitch_and_family: u32,
        psz_face_name: *const u16,
    ) -> isize;
    fn SetBkMode(hdc: isize, mode: i32) -> i32;
    fn SetTextColor(hdc: isize, color: u32) -> u32;
    fn ExtTextOutW(
        hdc: isize,
        x: i32,
        y: i32,
        options: u32,
        lprect: *const u8,
        lp_string: *const u16,
        c: u32,
        lp_dx: *const i32,
    ) -> i32;
    fn GetTextExtentPoint32W(
        hdc: isize,
        lp_string: *const u16,
        c: i32,
        psizl: *mut SizeWin,
    ) -> i32;
    fn GdiFlush() -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontStyle {
    pub px: i32,
    pub bold: bool,
    pub monospace: bool,
}

impl FontStyle {
    pub const fn new(px: i32, bold: bool, monospace: bool) -> Self {
        Self {
            px,
            bold,
            monospace,
        }
    }
}

struct FontScratch {
    hdc: isize,
    dib: isize,
    bits: *mut u8,
    dib_w: i32,
    dib_h: i32,
    fonts: HashMap<FontStyle, isize>,
}

impl FontScratch {
    fn new() -> Self {
        let hdc = unsafe { CreateCompatibleDC(0) };
        let mut s = Self {
            hdc,
            dib: 0,
            bits: null_mut(),
            dib_w: 0,
            dib_h: 0,
            fonts: HashMap::new(),
        };
        s.ensure_dib(512, 64);
        s
    }

    fn ensure_dib(&mut self, w: i32, h: i32) {
        let nw = w.max(self.dib_w).max(64);
        let nh = h.max(self.dib_h).max(64);
        if nw <= self.dib_w && nh <= self.dib_h && self.dib != 0 {
            return;
        }
        if self.dib != 0 {
            unsafe { DeleteObject(self.dib) };
            self.dib = 0;
        }
        let bmi = BitmapInfo {
            bmi_header: BitmapInfoHeader {
                bi_size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                bi_width: nw,
                bi_height: -nh,
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: BI_RGB,
                bi_size_image: 0,
                bi_x_pels_per_meter: 0,
                bi_y_pels_per_meter: 0,
                bi_clr_used: 0,
                bi_clr_important: 0,
            },
            bmi_colors: [0],
        };
        let mut ptr: *mut u8 = null_mut();
        let dib = unsafe { CreateDIBSection(self.hdc, &bmi, DIB_RGB_COLORS, &mut ptr, 0, 0) };
        if dib != 0 && !ptr.is_null() {
            unsafe { SelectObject(self.hdc, dib) };
            self.dib = dib;
            self.bits = ptr;
            self.dib_w = nw;
            self.dib_h = nh;
        }
    }

    fn get_font(&mut self, style: FontStyle) -> isize {
        if let Some(&h) = self.fonts.get(&style) {
            return h;
        }
        let face_name = if style.monospace {
            "Consolas"
        } else {
            "Segoe UI"
        };
        let wide: Vec<u16> = face_name.encode_utf16().chain(std::iter::once(0)).collect();
        let weight = if style.bold { FW_BOLD } else { FW_NORMAL };
        let h = unsafe {
            CreateFontW(
                -style.px.max(8),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                DEFAULT_PITCH,
                wide.as_ptr(),
            )
        };
        self.fonts.insert(style, h);
        h
    }

    fn measure(&mut self, style: FontStyle, text: &str) -> (i32, i32) {
        if text.is_empty() {
            return (0, style.px.max(10));
        }
        let font = self.get_font(style);
        unsafe { SelectObject(self.hdc, font) };
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut sz = SizeWin::default();
        unsafe { GetTextExtentPoint32W(self.hdc, wide.as_ptr(), wide.len() as i32, &mut sz) };
        (sz.cx.max(0), sz.cy.max(style.px))
    }

    fn draw(&mut self, canvas: &mut CanvasView<'_>, x: i32, y: i32, style: FontStyle, color: Color, text: &str) {
        if text.is_empty() || color.a == 0 {
            return;
        }
        let (tw, th) = self.measure(style, text);
        if tw <= 0 || th <= 0 {
            return;
        }
        let pad_w = (tw + 8).min(canvas.width() as i32 + 16).max(16);
        let pad_h = (th + 4).max(16);
        self.ensure_dib(pad_w, pad_h);
        if self.bits.is_null() {
            return;
        }

        // Clear only the sub-rectangle we will draw into
        let stride = self.dib_w as usize;
        let clear_w = pad_w.min(self.dib_w) as usize;
        let clear_h = pad_h.min(self.dib_h) as usize;
        unsafe {
            for row in 0..clear_h {
                std::ptr::write_bytes(self.bits.add(row * stride * 4), 0, clear_w * 4);
            }
            let font = self.get_font(style);
            SelectObject(self.hdc, font);
            SetBkMode(self.hdc, TRANSPARENT);
            SetTextColor(self.hdc, 0x00FF_FFFF);
            let wide: Vec<u16> = text.encode_utf16().collect();
            ExtTextOutW(
                self.hdc,
                0,
                0,
                0,
                null(),
                wide.as_ptr(),
                wide.len() as u32,
                null(),
            );
            GdiFlush();
        }

        let src_slice =
            unsafe { std::slice::from_raw_parts(self.bits, (self.dib_w as usize) * (self.dib_h as usize) * 4) };
        let dst_w = canvas.width() as i32;
        let dst_h = canvas.height() as i32;

        for row in 0..clear_h as i32 {
            let dy = y + row;
            if dy < 0 || dy >= dst_h {
                continue;
            }
            for col in 0..clear_w as i32 {
                let dx = x + col;
                if dx < 0 || dx >= dst_w {
                    continue;
                }
                let sidx = ((row as usize) * stride + (col as usize)) * 4;
                // White text on black background -> channel intensity is coverage 0..255
                let cov = src_slice[sidx].max(src_slice[sidx + 1]).max(src_slice[sidx + 2]);
                if cov > 0 {
                    let eff_a = ((cov as u16 * color.a as u16 + 127) / 255) as u8;
                    if eff_a > 0 {
                        canvas.set_pixel(dx, dy, Color::rgba(color.r, color.g, color.b, eff_a));
                    }
                }
            }
        }
    }
}

impl Drop for FontScratch {
    fn drop(&mut self) {
        unsafe {
            for (_, h) in self.fonts.drain() {
                if h != 0 {
                    DeleteObject(h);
                }
            }
            if self.dib != 0 {
                DeleteObject(self.dib);
            }
            if self.hdc != 0 {
                DeleteDC(self.hdc);
            }
        }
    }
}

thread_local! {
    static FONT_SCRATCH: RefCell<FontScratch> = RefCell::new(FontScratch::new());
}

pub fn measure_text_styled(style: FontStyle, text: &str) -> (i32, i32) {
    FONT_SCRATCH.with(|fs| fs.borrow_mut().measure(style, text))
}

pub fn draw_text_styled(
    canvas: &mut CanvasView<'_>,
    x: i32,
    y: i32,
    style: FontStyle,
    color: Color,
    text: &str,
) {
    FONT_SCRATCH.with(|fs| fs.borrow_mut().draw(canvas, x, y, style, color, text));
}

pub fn draw_text_clipped_styled(
    canvas: &mut CanvasView<'_>,
    x: i32,
    y: i32,
    style: FontStyle,
    color: Color,
    text: &str,
    max_width: i32,
) {
    if max_width <= 8 {
        return;
    }
    let (full_w, _) = measure_text_styled(style, text);
    if full_w <= max_width {
        draw_text_styled(canvas, x, y, style, color, text);
        return;
    }
    let chars: Vec<char> = text.chars().collect();
    let mut lo = 0usize;
    let mut hi = chars.len();
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let cand: String = chars[..mid].iter().collect();
        let cand_ell = format!("{cand}...");
        let (cw, _) = measure_text_styled(style, &cand_ell);
        if cw <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if lo == 0 {
        return;
    }
    let out: String = chars[..lo].iter().collect::<String>() + "...";
    draw_text_styled(canvas, x, y, style, color, &out);
}

#[allow(dead_code)]
const _FW_SEMIBOLD: i32 = FW_SEMIBOLD;
const _RECT_UNUSED: Option<Rect> = None;
