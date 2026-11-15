//! 2D software graphics engine, BGRA8 canvas, bitmap font rasterizer, and UI vector icons.

pub mod canvas;
pub mod color;
pub mod font;
pub mod icons;

pub use canvas::{CanvasView, OwnedCanvas, Rect};
pub use color::Color;
pub use font::{GLYPH_H, GLYPH_W, measure_text};
