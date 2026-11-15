use browser_gfx::{CanvasView, Color, Rect};

use crate::css::Display;
use crate::layout::LayoutBox;

pub fn paint_layout(layout: &LayoutBox, canvas: &mut CanvasView<'_>, scroll_y: i32) {
    canvas.clear(Color::rgb(255, 255, 255));
    paint_box(layout, canvas, scroll_y);
}

fn paint_box(b: &LayoutBox, canvas: &mut CanvasView<'_>, scroll_y: i32) {
    if b.style.display == Display::None {
        return;
    }

    let drawn_rect = Rect::new(b.rect.x, b.rect.y - scroll_y, b.rect.w, b.rect.h);
    let vp_h = canvas.height() as i32;

    if drawn_rect.y + drawn_rect.h >= 0 && drawn_rect.y < vp_h {
        if let Some(bg) = b.style.background {
            if b.style.border_radius > 0 {
                canvas.fill_rounded_rect(drawn_rect, b.style.border_radius, bg);
            } else {
                canvas.fill_rect(drawn_rect, bg);
            }
        }

        if b.style.border_width > 0.0 {
            if b.style.border_radius > 0 {
                canvas.stroke_rounded_rect(
                    drawn_rect,
                    b.style.border_radius,
                    b.style.border_color,
                );
            } else {
                canvas.stroke_rect(
                    drawn_rect,
                    b.style.border_width.round().max(1.0) as i32,
                    b.style.border_color,
                );
            }
        }

        if b.style.list_bullet {
            canvas.fill_circle(drawn_rect.x - 10, drawn_rect.y + 8, 3, b.style.color);
        }

        if let Some(lbl) = &b.img_label {
            canvas.fill_rounded_rect(drawn_rect, 4, Color::rgb(241, 243, 244));
            canvas.stroke_rounded_rect(drawn_rect, 4, Color::rgb(200, 204, 210));
            canvas.draw_text_clipped(
                drawn_rect.x + 8,
                drawn_rect.y + (drawn_rect.h / 2 - 8).max(4),
                2,
                Color::rgb(95, 99, 104),
                lbl,
                (drawn_rect.w - 16).max(20),
            );
        }

        for run in &b.text_runs {
            let ry = run.y - scroll_y;
            if ry + run.h < 0 || ry >= vp_h {
                continue;
            }
            canvas.draw_text(run.x, ry, run.scale, run.color, &run.text);
            if run.underline {
                let uy = ry + run.h + 1;
                canvas.draw_line(run.x, uy, run.x + run.w, uy, run.color);
            }
        }
    }

    for ch in &b.children {
        paint_box(ch, canvas, scroll_y);
    }
}
