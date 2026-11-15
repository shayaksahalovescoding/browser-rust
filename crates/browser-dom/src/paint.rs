use std::collections::HashMap;

use browser_gfx::{CanvasView, Color, DecodedImage, FontStyle, Rect, measure_text_styled};

use crate::css::Display;
use crate::dom::NodeId;
use crate::layout::{ControlKind, LayoutBox};

pub fn paint_layout(layout: &LayoutBox, canvas: &mut CanvasView<'_>, scroll_y: i32) {
    let empty = HashMap::new();
    paint_layout_with_assets(layout, canvas, scroll_y, &empty, None);
}

pub fn paint_layout_with_assets(
    layout: &LayoutBox,
    canvas: &mut CanvasView<'_>,
    scroll_y: i32,
    images: &HashMap<String, DecodedImage>,
    focused_input: Option<(NodeId, &str)>,
) {
    canvas.clear(Color::rgb(255, 255, 255));
    paint_box(layout, canvas, scroll_y, images, focused_input);
}

fn paint_box(
    b: &LayoutBox,
    canvas: &mut CanvasView<'_>,
    scroll_y: i32,
    images: &HashMap<String, DecodedImage>,
    focused_input: Option<(NodeId, &str)>,
) {
    if b.style.display == Display::None {
        return;
    }

    let drawn_rect = Rect::new(b.rect.x, b.rect.y - scroll_y, b.rect.w, b.rect.h);
    let vp_h = canvas.height() as i32;
    let is_focused = matches!(focused_input, Some((fid, _)) if fid == b.node_id);

    if drawn_rect.y + drawn_rect.h >= 0 && drawn_rect.y < vp_h {
        if let Some(bg) = b.style.background {
            if b.style.border_radius > 0 {
                canvas.fill_rounded_rect(drawn_rect, b.style.border_radius, bg);
            } else {
                canvas.fill_rect(drawn_rect, bg);
            }
        }

        let border_col = if is_focused {
            Color::rgb(26, 115, 232)
        } else {
            b.style.border_color
        };
        if b.style.border_width > 0.0 || is_focused {
            if b.style.border_radius > 0 {
                canvas.stroke_rounded_rect(drawn_rect, b.style.border_radius, border_col);
            } else {
                canvas.stroke_rect(
                    drawn_rect,
                    b.style.border_width.round().max(1.0) as i32,
                    border_col,
                );
            }
        }

        if b.style.list_bullet {
            canvas.fill_circle(drawn_rect.x - 10, drawn_rect.y + 9, 3, b.style.color);
        }

        if b.img_src.is_some() || b.img_label.is_some() {
            let decoded = b.img_src.as_deref().and_then(|s| images.get(s));
            if let Some(img) = decoded {
                canvas.blit_bgra_scaled(drawn_rect, img.width, img.height, &img.bgra);
            } else if let Some(lbl) = &b.img_label {
                canvas.fill_rounded_rect(drawn_rect, 6, Color::rgb(248, 249, 250));
                canvas.stroke_rounded_rect(drawn_rect, 6, Color::rgb(218, 220, 224));
                let font = FontStyle::new(12, false, false);
                canvas.draw_text_clipped_styled(
                    drawn_rect.x + 8,
                    drawn_rect.y + (drawn_rect.h / 2 - 8).max(4),
                    font,
                    Color::rgb(95, 99, 104),
                    lbl,
                    (drawn_rect.w - 16).max(16),
                );
            }
        }

        if let Some(ControlKind::TextInput {
            value,
            placeholder,
            ..
        }) = &b.control
        {
            let active_val = match focused_input {
                Some((fid, txt)) if fid == b.node_id => txt,
                _ => value.as_str(),
            };
            let font = b.style.font_style();
            let (shown, col) = if !active_val.is_empty() {
                (active_val, b.style.color)
            } else if is_focused {
                ("", b.style.color)
            } else {
                (placeholder.as_str(), Color::rgb(128, 134, 139))
            };
            let pad_l = b.style.padding[3].round().max(12.0) as i32;
            let text_x = drawn_rect.x + pad_l;
            let (_, lh) = measure_text_styled(font, "Ay");
            let text_y = drawn_rect.y + ((drawn_rect.h - lh) / 2).max(2);
            let max_w = (drawn_rect.w - pad_l * 2).max(16);
            if !shown.is_empty() {
                canvas.draw_text_clipped_styled(text_x, text_y, font, col, shown, max_w);
            }
            if is_focused {
                let (tw, _) = measure_text_styled(font, active_val);
                let cx = (text_x + tw + 1).min(drawn_rect.x + drawn_rect.w - pad_l);
                canvas.fill_rect(
                    Rect::new(cx, drawn_rect.y + 7, 2, (drawn_rect.h - 14).max(10)),
                    Color::rgb(26, 115, 232),
                );
            }
        }

        for run in &b.text_runs {
            let ry = run.y - scroll_y;
            if ry + run.h < 0 || ry >= vp_h {
                continue;
            }
            canvas.draw_text_styled(run.x, ry, run.font, run.color, &run.text);
            if run.underline {
                let uy = ry + run.h - 1;
                canvas.draw_line(run.x, uy, run.x + run.w, uy, run.color);
            }
        }
    }

    for ch in &b.children {
        paint_box(ch, canvas, scroll_y, images, focused_input);
    }
}
