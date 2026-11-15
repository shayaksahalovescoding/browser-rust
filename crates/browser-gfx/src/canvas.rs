use crate::color::Color;
use crate::font_win32::{FontStyle, draw_text_clipped_styled, draw_text_styled};
use crate::scale_to_px;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn contains(self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

/// Owned BGRA8 canvas buffer.
pub struct OwnedCanvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl OwnedCanvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.pixels.resize((width as usize) * (height as usize) * 4, 0);
        }
    }

    pub fn view_mut(&mut self) -> CanvasView<'_> {
        CanvasView::new(self.width, self.height, &mut self.pixels)
    }
}

/// Borrowed mutable 2D BGRA8 canvas view (writes directly to owned or shared-memory buffers).
pub struct CanvasView<'a> {
    width: u32,
    height: u32,
    pixels: &'a mut [u8],
}

impl<'a> CanvasView<'a> {
    pub fn new(width: u32, height: u32, pixels: &'a mut [u8]) -> Self {
        assert_eq!(pixels.len(), (width as usize) * (height as usize) * 4);
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        self.pixels
    }

    pub fn clear(&mut self, color: Color) {
        let bgra = color.to_bgra();
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&bgra);
        }
    }

    #[inline]
    pub fn set_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        let dst = [
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ];
        let blended = color.blend_over(dst);
        self.pixels[idx..idx + 4].copy_from_slice(&blended);
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        if rect.w <= 0 || rect.h <= 0 || color.a == 0 {
            return;
        }
        let x0 = rect.x.max(0);
        let y0 = rect.y.max(0);
        let x1 = (rect.x + rect.w).min(self.width as i32);
        let y1 = (rect.y + rect.h).min(self.height as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        if color.a == 255 {
            let bgra = color.to_bgra();
            for y in y0..y1 {
                let row_start = ((y as usize) * (self.width as usize) + (x0 as usize)) * 4;
                let row_end = ((y as usize) * (self.width as usize) + (x1 as usize)) * 4;
                for px in self.pixels[row_start..row_end].chunks_exact_mut(4) {
                    px.copy_from_slice(&bgra);
                }
            }
        } else {
            for y in y0..y1 {
                for x in x0..x1 {
                    self.set_pixel(x, y, color);
                }
            }
        }
    }

    pub fn stroke_rect(&mut self, rect: Rect, thickness: i32, color: Color) {
        if rect.w <= 0 || rect.h <= 0 || thickness <= 0 {
            return;
        }
        let t = thickness.min(rect.w).min(rect.h);
        self.fill_rect(Rect::new(rect.x, rect.y, rect.w, t), color);
        self.fill_rect(Rect::new(rect.x, rect.y + rect.h - t, rect.w, t), color);
        self.fill_rect(Rect::new(rect.x, rect.y + t, t, rect.h - 2 * t), color);
        self.fill_rect(
            Rect::new(rect.x + rect.w - t, rect.y + t, t, rect.h - 2 * t),
            color,
        );
    }

    /// Smooth 2x2 subpixel anti-aliased rounded rectangle fill.
    pub fn fill_rounded_rect(&mut self, rect: Rect, radius: i32, color: Color) {
        if rect.w <= 0 || rect.h <= 0 || color.a == 0 {
            return;
        }
        let r = radius.max(0).min(rect.w / 2).min(rect.h / 2);
        if r == 0 {
            self.fill_rect(rect, color);
            return;
        }
        let x0 = rect.x.max(0);
        let y0 = rect.y.max(0);
        let x1 = (rect.x + rect.w).min(self.width as i32);
        let y1 = (rect.y + rect.h).min(self.height as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        // Fast interior fill (non-corner band)
        if y0 + r < y1 - r {
            self.fill_rect(Rect::new(x0, (rect.y + r).max(y0), x1 - x0, (rect.y + rect.h - r).min(y1) - (rect.y + r).max(y0)), color);
        }

        let rf = r as f32;
        for y in y0..y1 {
            let in_top = y < rect.y + r;
            let in_bot = y >= rect.y + rect.h - r;
            if !in_top && !in_bot {
                continue;
            }
            let cy = if in_top {
                (rect.y + r) as f32 - 0.5
            } else {
                (rect.y + rect.h - r) as f32 - 0.5
            };
            for x in x0..x1 {
                let in_left = x < rect.x + r;
                let in_right = x >= rect.x + rect.w - r;
                if !in_left && !in_right {
                    self.set_pixel(x, y, color);
                    continue;
                }
                let cx = if in_left {
                    (rect.x + r) as f32 - 0.5
                } else {
                    (rect.x + rect.w - r) as f32 - 0.5
                };
                let dx = (x as f32) - cx;
                let dy = (y as f32) - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                let cov = (rf + 0.5 - dist).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let a = ((color.a as f32) * cov).round() as u8;
                    self.set_pixel(x, y, Color::rgba(color.r, color.g, color.b, a));
                }
            }
        }
    }

    pub fn stroke_rounded_rect(&mut self, rect: Rect, radius: i32, color: Color) {
        if rect.w <= 0 || rect.h <= 0 || color.a == 0 {
            return;
        }
        let r = radius.max(0).min(rect.w / 2).min(rect.h / 2);
        let x0 = rect.x.max(0);
        let y0 = rect.y.max(0);
        let x1 = (rect.x + rect.w).min(self.width as i32);
        let y1 = (rect.y + rect.h).min(self.height as i32);
        let rf = r as f32;

        for y in y0..y1 {
            let in_top = y < rect.y + r;
            let in_bot = y >= rect.y + rect.h - r;
            let cy = if in_top {
                (rect.y + r) as f32 - 0.5
            } else {
                (rect.y + rect.h - r) as f32 - 0.5
            };
            for x in x0..x1 {
                let in_left = x < rect.x + r;
                let in_right = x >= rect.x + rect.w - r;
                if (in_top || in_bot) && (in_left || in_right) {
                    let cx = if in_left {
                        (rect.x + r) as f32 - 0.5
                    } else {
                        (rect.x + rect.w - r) as f32 - 0.5
                    };
                    let dx = (x as f32) - cx;
                    let dy = (y as f32) - cy;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let cov = (1.0 - (dist - (rf - 0.5)).abs()).clamp(0.0, 1.0);
                    if cov > 0.0 {
                        let a = ((color.a as f32) * cov).round() as u8;
                        self.set_pixel(x, y, Color::rgba(color.r, color.g, color.b, a));
                    }
                } else if x == rect.x
                    || x == rect.x + rect.w - 1
                    || y == rect.y
                    || y == rect.y + rect.h - 1
                {
                    self.set_pixel(x, y, color);
                }
            }
        }
    }

    pub fn draw_line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: Color) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.set_pixel(x0, y0, color);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    pub fn fill_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Color) {
        let r2 = radius * radius;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= r2 {
                    self.set_pixel(cx + dx, cy + dy, color);
                }
            }
        }
    }

    pub fn draw_text(&mut self, x: i32, y: i32, scale: i32, color: Color, text: &str) {
        let style = FontStyle::new(scale_to_px(scale), scale >= 3, false);
        draw_text_styled(self, x, y, style, color, text);
    }

    pub fn draw_text_styled(
        &mut self,
        x: i32,
        y: i32,
        style: FontStyle,
        color: Color,
        text: &str,
    ) {
        draw_text_styled(self, x, y, style, color, text);
    }

    pub fn draw_text_clipped(
        &mut self,
        x: i32,
        y: i32,
        scale: i32,
        color: Color,
        text: &str,
        max_width: i32,
    ) {
        let style = FontStyle::new(scale_to_px(scale), scale >= 3, false);
        draw_text_clipped_styled(self, x, y, style, color, text, max_width);
    }

    pub fn draw_text_clipped_styled(
        &mut self,
        x: i32,
        y: i32,
        style: FontStyle,
        color: Color,
        text: &str,
        max_width: i32,
    ) {
        draw_text_clipped_styled(self, x, y, style, color, text, max_width);
    }

    /// Alpha-blends a decoded BGRA8 image scaled to `dst_rect`.
    pub fn blit_bgra_scaled(
        &mut self,
        dst_rect: Rect,
        src_w: u32,
        src_h: u32,
        src_pixels: &[u8],
    ) {
        if dst_rect.w <= 0 || dst_rect.h <= 0 || src_w == 0 || src_h == 0 {
            return;
        }
        let x0 = dst_rect.x.max(0);
        let y0 = dst_rect.y.max(0);
        let x1 = (dst_rect.x + dst_rect.w).min(self.width as i32);
        let y1 = (dst_rect.y + dst_rect.h).min(self.height as i32);
        for dy in y0..y1 {
            let sy = (((dy - dst_rect.y) as usize) * (src_h as usize)) / (dst_rect.h as usize);
            for dx in x0..x1 {
                let sx = (((dx - dst_rect.x) as usize) * (src_w as usize)) / (dst_rect.w as usize);
                let sidx = (sy * (src_w as usize) + sx) * 4;
                if sidx + 4 <= src_pixels.len() {
                    let a = src_pixels[sidx + 3];
                    if a > 0 {
                        let c = Color::rgba(
                            src_pixels[sidx + 2],
                            src_pixels[sidx + 1],
                            src_pixels[sidx],
                            a,
                        );
                        self.set_pixel(dx, dy, c);
                    }
                }
            }
        }
    }

    /// Blits an external BGRA8 image into `dst_x, dst_y` clipping to canvas bounds.
    pub fn blit_bgra(
        &mut self,
        dst_x: i32,
        dst_y: i32,
        src_w: u32,
        src_h: u32,
        src_pixels: &[u8],
    ) {
        let copy_w = (src_w as i32).min(self.width as i32 - dst_x);
        let copy_h = (src_h as i32).min(self.height as i32 - dst_y);
        if copy_w <= 0 || copy_h <= 0 || dst_x < 0 || dst_y < 0 {
            return;
        }
        let row_bytes = (copy_w as usize) * 4;
        for row in 0..(copy_h as usize) {
            let src_off = row * (src_w as usize) * 4;
            let dst_off = ((dst_y as usize + row) * (self.width as usize) + (dst_x as usize)) * 4;
            if src_off + row_bytes <= src_pixels.len() && dst_off + row_bytes <= self.pixels.len()
            {
                self.pixels[dst_off..dst_off + row_bytes]
                    .copy_from_slice(&src_pixels[src_off..src_off + row_bytes]);
            }
        }
    }
}
