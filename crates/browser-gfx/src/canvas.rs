use crate::color::Color;
use crate::font::{GLYPH_W, glyph_bitmap};

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
        let r2 = r * r;
        for y in y0..y1 {
            let dy = if y < rect.y + r {
                (rect.y + r) - y - 1
            } else if y >= rect.y + rect.h - r {
                y - (rect.y + rect.h - r)
            } else {
                0
            };
            for x in x0..x1 {
                let dx = if x < rect.x + r {
                    (rect.x + r) - x - 1
                } else if x >= rect.x + rect.w - r {
                    x - (rect.x + rect.w - r)
                } else {
                    0
                };
                if dx == 0 || dy == 0 || dx * dx + dy * dy <= r2 {
                    self.set_pixel(x, y, color);
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
        let r_out2 = r * r;
        let r_in = (r - 1).max(0);
        let r_in2 = r_in * r_in;

        for y in y0..y1 {
            let dy = if y < rect.y + r {
                (rect.y + r) - y - 1
            } else if y >= rect.y + rect.h - r {
                y - (rect.y + rect.h - r)
            } else {
                0
            };
            for x in x0..x1 {
                let dx = if x < rect.x + r {
                    (rect.x + r) - x - 1
                } else if x >= rect.x + rect.w - r {
                    x - (rect.x + rect.w - r)
                } else {
                    0
                };
                let is_edge = if dx == 0 && dy == 0 {
                    x == rect.x
                        || x == rect.x + rect.w - 1
                        || y == rect.y
                        || y == rect.y + rect.h - 1
                } else {
                    let d2 = dx * dx + dy * dy;
                    d2 <= r_out2 && (dx == 0 || dy == 0 || d2 >= r_in2)
                };
                if is_edge {
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
        let scale = scale.max(1);
        let mut pen_x = x;
        for ch in text.chars() {
            let rows = glyph_bitmap(ch);
            for (row_idx, bits) in rows.iter().enumerate() {
                for col_idx in 0..GLYPH_W {
                    if (bits >> (7 - col_idx)) & 1 == 1 {
                        let px = pen_x + (col_idx as i32) * scale;
                        let py = y + (row_idx as i32) * scale;
                        self.fill_rect(Rect::new(px, py, scale, scale), color);
                    }
                }
            }
            pen_x += (GLYPH_W as i32) * scale;
        }
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
        let char_w = (GLYPH_W as i32) * scale.max(1);
        if max_width < char_w {
            return;
        }
        let max_chars = (max_width / char_w) as usize;
        let char_count = text.chars().count();
        if char_count <= max_chars {
            self.draw_text(x, y, scale, color, text);
        } else if max_chars >= 2 {
            let truncated: String = text.chars().take(max_chars - 1).collect();
            self.draw_text(x, y, scale, color, &(truncated + "~"));
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
