use crate::canvas::CanvasView;
use crate::color::Color;

pub fn draw_back_arrow(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    canvas.draw_line(cx - 6, cy, cx + 6, cy, color);
    canvas.draw_line(cx - 6, cy + 1, cx + 6, cy + 1, color);
    canvas.draw_line(cx - 6, cy, cx - 1, cy - 5, color);
    canvas.draw_line(cx - 5, cy, cx, cy - 5, color);
    canvas.draw_line(cx - 6, cy + 1, cx - 1, cy + 6, color);
    canvas.draw_line(cx - 5, cy + 1, cx, cy + 6, color);
}

pub fn draw_forward_arrow(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    canvas.draw_line(cx - 6, cy, cx + 6, cy, color);
    canvas.draw_line(cx - 6, cy + 1, cx + 6, cy + 1, color);
    canvas.draw_line(cx + 6, cy, cx + 1, cy - 5, color);
    canvas.draw_line(cx + 5, cy, cx, cy - 5, color);
    canvas.draw_line(cx + 6, cy + 1, cx + 1, cy + 6, color);
    canvas.draw_line(cx + 5, cy + 1, cx, cy + 6, color);
}

pub fn draw_reload_icon(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    // Draw a crisp octagonal ring with a top-right gap and arrowhead
    let pts = [
        (0, -6),
        (-3, -5),
        (-5, -3),
        (-6, 0),
        (-5, 3),
        (-3, 5),
        (0, 6),
        (3, 5),
        (5, 3),
    ];
    for w in pts.windows(2) {
        canvas.draw_line(cx + w[0].0, cy + w[0].1, cx + w[1].0, cy + w[1].1, color);
    }
    // Arrowhead at top-right
    canvas.draw_line(cx + 1, cy - 6, cx + 5, cy - 4, color);
    canvas.draw_line(cx + 5, cy - 4, cx + 5, cy - 8, color);
    canvas.draw_line(cx + 5, cy - 4, cx + 1, cy - 3, color);
}

pub fn draw_close_icon(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    canvas.draw_line(cx - 4, cy - 4, cx + 4, cy + 4, color);
    canvas.draw_line(cx - 4, cy + 4, cx + 4, cy - 4, color);
}

pub fn draw_plus_icon(canvas: &mut CanvasView<'_>, cx: i32, cy: i32, color: Color) {
    canvas.draw_line(cx - 5, cy, cx + 5, cy, color);
    canvas.draw_line(cx - 5, cy + 1, cx + 5, cy + 1, color);
    canvas.draw_line(cx, cy - 5, cx, cy + 5, color);
    canvas.draw_line(cx + 1, cy - 5, cx + 1, cy + 5, color);
}
