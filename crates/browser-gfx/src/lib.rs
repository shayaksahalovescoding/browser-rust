//! 2D software graphics engine, BGRA8 canvas, native ClearType font rasterizer, image decoder, and UI vector icons.

pub mod canvas;
pub mod color;
pub mod font;
pub mod font_win32;
pub mod icons;
pub mod image;

pub use canvas::{CanvasView, OwnedCanvas, Rect};
pub use color::Color;
pub use font::{GLYPH_H, GLYPH_W};
pub use font_win32::{FontStyle, draw_text_clipped_styled, draw_text_styled, measure_text_styled};
pub use image::{DecodedImage, decode_image};

/// Measures text width and height using the crisp native Segoe UI font at pixel height `scale * 7 + 2`.
pub fn measure_text(scale: i32, text: &str) -> (i32, i32) {
    let px = scale_to_px(scale);
    measure_text_styled(FontStyle::new(px, scale >= 3, false), text)
}

pub const fn scale_to_px(scale: i32) -> i32 {
    match scale {
        ..=1 => 12,
        2 => 15,
        3 => 21,
        _ => 30,
    }
}
