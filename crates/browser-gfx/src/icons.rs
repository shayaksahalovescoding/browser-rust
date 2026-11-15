use crate::canvas::{CanvasView, Rect};
use crate::color::Color;

pub fn draw_back_arrow(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    let cx = cx as f32 + 0.5;
    let cy = cy as f32 + 0.5;
    canvas.draw_line_aa(cx - 5.5, cy, cx + 6.0, cy, 1.7, color);
    canvas.draw_line_aa(cx - 5.5, cy, cx - 0.5, cy - 5.0, 1.7, color);
    canvas.draw_line_aa(cx - 5.5, cy, cx - 0.5, cy + 5.0, 1.7, color);
}

pub fn draw_forward_arrow(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    let cx = cx as f32 + 0.5;
    let cy = cy as f32 + 0.5;
    canvas.draw_line_aa(cx - 6.0, cy, cx + 5.5, cy, 1.7, color);
    canvas.draw_line_aa(cx + 5.5, cy, cx + 0.5, cy - 5.0, 1.7, color);
    canvas.draw_line_aa(cx + 5.5, cy, cx + 0.5, cy + 5.0, 1.7, color);
}

pub fn draw_reload_icon(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    let fx = cx as f32 + 0.5;
    let fy = cy as f32 + 0.5;
    canvas.draw_arc_aa(fx, fy, 5.5, 1.65, 35.0, 325.0, color);
    canvas.draw_line_aa(fx + 4.8, fy - 3.2, fx + 4.8, fy - 7.0, 1.6, color);
    canvas.draw_line_aa(fx + 4.8, fy - 3.2, fx + 1.2, fy - 2.6, 1.6, color);
}

pub fn draw_close_icon(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    let cx = cx as f32 + 0.5;
    let cy = cy as f32 + 0.5;
    canvas.draw_line_aa(cx - 3.8, cy - 3.8, cx + 3.8, cy + 3.8, 1.5, color);
    canvas.draw_line_aa(cx - 3.8, cy + 3.8, cx + 3.8, cy - 3.8, 1.5, color);
}

pub fn draw_plus_icon(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    let cx = cx as f32 + 0.5;
    let cy = cy as f32 + 0.5;
    canvas.draw_line_aa(cx - 5.0, cy, cx + 5.0, cy, 1.65, color);
    canvas.draw_line_aa(cx, cy - 5.0, cx, cy + 5.0, 1.65, color);
}

pub fn draw_lock_icon(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    let fx = cx as f32 + 0.5;
    let fy = cy as f32 + 0.5;
    canvas.draw_arc_aa(fx, fy - 2.0, 2.8, 1.4, 180.0, 360.0, color);
    canvas.fill_rounded_rect(Rect::new(cx - 4, cy - 1, 9, 7), 2, color);
}

pub fn draw_search_icon(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    let fx = cx as f32;
    let fy = cy as f32;
    canvas.draw_arc_aa(fx - 1.0, fy - 1.0, 4.5, 1.6, 0.0, 360.0, color);
    canvas.draw_line_aa(fx + 2.3, fy + 2.3, fx + 5.8, fy + 5.8, 1.7, color);
}
