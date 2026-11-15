use crate::color::Color;
use crate::image::DecodedImage;

#[derive(Debug, Clone)]
pub enum SvgElement {
    Path {
        d: String,
        fill: Color,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: Color,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
        fill: Color,
    },
    StrokePath {
        d: String,
        stroke: Color,
        stroke_width: f32,
    },
    Polyline {
        points: Vec<(f32, f32)>,
        fill: Color,
        stroke: Color,
        stroke_width: f32,
        closed: bool,
    },
}

/// Rasterizes vector SVG primitives mapped cleanly from `viewbox` `(vx, vy, vw, vh)` into
/// a 4x supersampled anti-aliased `DecodedImage` of size `out_w x out_h`.
pub fn rasterize_svg(
    viewbox: (f32, f32, f32, f32),
    out_w: u32,
    out_h: u32,
    elements: &[SvgElement],
) -> Option<DecodedImage> {
    let w = out_w.clamp(1, 1024);
    let h = out_h.clamp(1, 1024);
    let (vx, vy, vw, vh) = viewbox;
    if vw <= 0.0 || vh <= 0.0 || elements.is_empty() {
        return None;
    }

    // 4x supersampling buffer (4*w by 4*h)
    const SS: u32 = 4;
    let sw = w * SS;
    let sh = h * SS;
    let sx = sw as f32 / vw;
    let sy = sh as f32 / vh;
    let mut hi_bgra = vec![0u8; (sw as usize) * (sh as usize) * 4];

    for el in elements {
        match el {
            SvgElement::Path { d, fill } => {
                if fill.a == 0 {
                    continue;
                }
                let subpaths = parse_svg_path_subpaths(d, vx, vy, sx, sy);
                if !subpaths.is_empty() {
                    fill_polygons_nonzero(&mut hi_bgra, sw, sh, &subpaths, *fill);
                }
            }
            SvgElement::Rect { x, y, w: rw, h: rh, fill } => {
                if fill.a == 0 || *rw <= 0.0 || *rh <= 0.0 {
                    continue;
                }
                let px = (*x - vx) * sx;
                let py = (*y - vy) * sy;
                let pw = *rw * sx;
                let ph = *rh * sy;
                let poly = vec![
                    (px, py),
                    (px + pw, py),
                    (px + pw, py + ph),
                    (px, py + ph),
                ];
                fill_polygons_nonzero(&mut hi_bgra, sw, sh, &[poly], *fill);
            }
            SvgElement::Circle { cx, cy, r, fill } => {
                if fill.a == 0 || *r <= 0.0 {
                    continue;
                }
                let pcx = (*cx - vx) * sx;
                let pcy = (*cy - vy) * sy;
                let prx = *r * sx;
                let pry = *r * sy;
                let mut poly = Vec::with_capacity(32);
                for k in 0..32 {
                    let ang = (k as f32) * std::f32::consts::TAU / 32.0;
                    poly.push((pcx + prx * ang.cos(), pcy + pry * ang.sin()));
                }
                fill_polygons_nonzero(&mut hi_bgra, sw, sh, &[poly], *fill);
            }
            SvgElement::StrokePath {
                d,
                stroke,
                stroke_width,
            } => {
                if stroke.a == 0 || *stroke_width <= 0.0 {
                    continue;
                }
                let subpaths = parse_svg_path_subpaths(d, vx, vy, sx, sy);
                let hw = (*stroke_width * 0.5 * ((sx + sy) * 0.5)).max(1.2);
                for sp in &subpaths {
                    stroke_polyline(&mut hi_bgra, sw, sh, sp, false, hw, *stroke);
                }
            }
            SvgElement::Polyline {
                points,
                fill,
                stroke,
                stroke_width,
                closed,
            } => {
                let mapped: Vec<(f32, f32)> = points
                    .iter()
                    .map(|&(px, py)| ((px - vx) * sx, (py - vy) * sy))
                    .collect();
                if fill.a > 0 && mapped.len() >= 3 {
                    fill_polygons_nonzero(&mut hi_bgra, sw, sh, &[mapped.clone()], *fill);
                }
                if stroke.a > 0 && *stroke_width > 0.0 && mapped.len() >= 2 {
                    let hw = (*stroke_width * 0.5 * ((sx + sy) * 0.5)).max(1.2);
                    stroke_polyline(&mut hi_bgra, sw, sh, &mapped, *closed, hw, *stroke);
                }
            }
        }
    }

    // Downsample 4x4 -> 1x1 box filter
    let mut bgra = vec![0u8; (w as usize) * (h as usize) * 4];
    for y in 0..h {
        for x in 0..w {
            let mut sb = 0u32;
            let mut sg = 0u32;
            let mut sr = 0u32;
            let mut sa = 0u32;
            for dy in 0..SS {
                for dx in 0..SS {
                    let i = (((y * SS + dy) * sw + (x * SS + dx)) as usize) * 4;
                    let a = hi_bgra[i + 3] as u32;
                    sb += hi_bgra[i] as u32 * a;
                    sg += hi_bgra[i + 1] as u32 * a;
                    sr += hi_bgra[i + 2] as u32 * a;
                    sa += a;
                }
            }
            let di = ((y * w + x) as usize) * 4;
            let avg_a = sa / (SS * SS);
            if avg_a > 0 {
                bgra[di] = (sb / sa).min(255) as u8;
                bgra[di + 1] = (sg / sa).min(255) as u8;
                bgra[di + 2] = (sr / sa).min(255) as u8;
                bgra[di + 3] = avg_a.min(255) as u8;
            }
        }
    }

    Some(DecodedImage {
        width: w,
        height: h,
        bgra,
    })
}

fn fill_polygons_nonzero(
    buf: &mut [u8],
    w: u32,
    h: u32,
    subpaths: &[Vec<(f32, f32)>],
    fill: Color,
) {
    let mut min_y = h as f32;
    let mut max_y = 0.0f32;
    for sp in subpaths {
        for &(_, y) in sp {
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    let y0 = (min_y.floor() as i32).max(0);
    let y1 = (max_y.ceil() as i32).min(h as i32 - 1);
    if y0 > y1 {
        return;
    }

    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for y in y0..=y1 {
        let sy = y as f32 + 0.5;
        crossings.clear();
        for sp in subpaths {
            if sp.len() < 2 {
                continue;
            }
            let n = sp.len();
            for i in 0..n {
                let (x0, yy0) = sp[i];
                let (x1, yy1) = sp[(i + 1) % n];
                if (yy0 <= sy && yy1 > sy) || (yy1 <= sy && yy0 > sy) {
                    let t = (sy - yy0) / (yy1 - yy0);
                    let x = x0 + t * (x1 - x0);
                    let dir = if yy1 > yy0 { 1 } else { -1 };
                    crossings.push((x, dir));
                }
            }
        }
        crossings.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut winding = 0i32;
        let mut idx = 0usize;
        while idx < crossings.len() {
            let start_x = crossings[idx].0;
            winding += crossings[idx].1;
            idx += 1;
            while winding != 0 && idx < crossings.len() {
                let end_x = crossings[idx].0;
                let px0 = (start_x.floor() as i32).max(0);
                let px1 = (end_x.ceil() as i32).min(w as i32 - 1);
                for x in px0..=px1 {
                    let fx = x as f32 + 0.5;
                    if fx >= start_x && fx <= end_x {
                        let off = ((y as usize) * (w as usize) + (x as usize)) * 4;
                        let dst = [buf[off], buf[off + 1], buf[off + 2], buf[off + 3]];
                        let blended = fill.blend_over(dst);
                        buf[off..off + 4].copy_from_slice(&blended);
                    }
                }
                winding += crossings[idx].1;
                idx += 1;
                if winding == 0 {
                    break;
                }
            }
        }
    }
}

fn stroke_polyline(
    buf: &mut [u8],
    w: u32,
    h: u32,
    pts: &[(f32, f32)],
    closed: bool,
    half_w: f32,
    stroke: Color,
) {
    if pts.len() < 2 {
        return;
    }
    let seg_count = if closed { pts.len() } else { pts.len() - 1 };
    let pad = half_w.ceil() as i32 + 1;
    for i in 0..seg_count {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % pts.len()];
        let min_x = (x0.min(x1).floor() as i32 - pad).max(0);
        let max_x = (x0.max(x1).ceil() as i32 + pad).min(w as i32 - 1);
        let min_y = (y0.min(y1).floor() as i32 - pad).max(0);
        let max_y = (y0.max(y1).ceil() as i32 + pad).min(h as i32 - 1);
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len_sq = dx * dx + dy * dy;
        for py in min_y..=max_y {
            let fy = py as f32 + 0.5;
            for px in min_x..=max_x {
                let fx = px as f32 + 0.5;
                let t = if len_sq > 0.0001 {
                    (((fx - x0) * dx + (fy - y0) * dy) / len_sq).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let cx = x0 + t * dx;
                let cy = y0 + t * dy;
                let dist = ((fx - cx) * (fx - cx) + (fy - cy) * (fy - cy)).sqrt();
                if dist <= half_w {
                    let off = ((py as usize) * (w as usize) + (px as usize)) * 4;
                    let dst = [buf[off], buf[off + 1], buf[off + 2], buf[off + 3]];
                    let blended = stroke.blend_over(dst);
                    buf[off..off + 4].copy_from_slice(&blended);
                }
            }
        }
    }
}

#[derive(Debug)]
enum PathTok {
    Cmd(char),
    Num(f32),
}

fn tokenize_svg_path(d: &str) -> Vec<PathTok> {
    let mut out = Vec::new();
    let bytes = d.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() || b == b',' {
            i += 1;
            continue;
        }
        if b.is_ascii_alphabetic() {
            out.push(PathTok::Cmd(b as char));
            i += 1;
            continue;
        }
        let start = i;
        if b == b'-' || b == b'+' {
            i += 1;
        }
        let mut seen_dot = false;
        while i < bytes.len() {
            let c = bytes[i];
            if c.is_ascii_digit() {
                i += 1;
            } else if c == b'.' && !seen_dot {
                seen_dot = true;
                i += 1;
            } else if (c == b'e' || c == b'E') && i + 1 < bytes.len() {
                i += 1;
                if bytes[i] == b'+' || bytes[i] == b'-' {
                    i += 1;
                }
            } else {
                break;
            }
        }
        if let Ok(num) = d[start..i].parse::<f32>() {
            out.push(PathTok::Num(num));
        } else {
            i += 1;
        }
    }
    out
}

fn parse_svg_path_subpaths(d: &str, vx: f32, vy: f32, sx: f32, sy: f32) -> Vec<Vec<(f32, f32)>> {
    let toks = tokenize_svg_path(d);
    let mut subpaths: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur_poly: Vec<(f32, f32)> = Vec::new();
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut start_x = 0.0f32;
    let mut start_y = 0.0f32;
    let mut last_ctrl: Option<(f32, f32)> = None;
    let mut last_q_ctrl: Option<(f32, f32)> = None;

    let mut ti = 0usize;
    let mut cmd = '\0';
    let to_px = |x: f32, y: f32| -> (f32, f32) { ((x - vx) * sx, (y - vy) * sy) };

    let next_num = |ti: &mut usize| -> Option<f32> {
        if let Some(PathTok::Num(v)) = toks.get(*ti) {
            *ti += 1;
            Some(*v)
        } else {
            None
        }
    };

    while ti < toks.len() {
        if let PathTok::Cmd(c) = toks[ti] {
            cmd = c;
            ti += 1;
            if cmd == 'Z' || cmd == 'z' {
                cx = start_x;
                cy = start_y;
                if !cur_poly.is_empty() {
                    subpaths.push(std::mem::take(&mut cur_poly));
                }
                last_ctrl = None;
                last_q_ctrl = None;
                continue;
            }
        }
        if cmd == '\0' {
            break;
        }
        match cmd {
            'M' | 'm' => {
                let (Some(x), Some(y)) = (next_num(&mut ti), next_num(&mut ti)) else {
                    break;
                };
                if !cur_poly.is_empty() {
                    subpaths.push(std::mem::take(&mut cur_poly));
                }
                if cmd == 'm' {
                    cx += x;
                    cy += y;
                } else {
                    cx = x;
                    cy = y;
                }
                start_x = cx;
                start_y = cy;
                cur_poly.push(to_px(cx, cy));
                cmd = if cmd == 'm' { 'l' } else { 'L' };
                last_ctrl = None;
                last_q_ctrl = None;
            }
            'L' | 'l' => {
                let (Some(x), Some(y)) = (next_num(&mut ti), next_num(&mut ti)) else {
                    break;
                };
                if cmd == 'l' {
                    cx += x;
                    cy += y;
                } else {
                    cx = x;
                    cy = y;
                }
                cur_poly.push(to_px(cx, cy));
                last_ctrl = None;
                last_q_ctrl = None;
            }
            'H' | 'h' => {
                let Some(x) = next_num(&mut ti) else { break };
                cx = if cmd == 'h' { cx + x } else { x };
                cur_poly.push(to_px(cx, cy));
                last_ctrl = None;
                last_q_ctrl = None;
            }
            'V' | 'v' => {
                let Some(y) = next_num(&mut ti) else { break };
                cy = if cmd == 'v' { cy + y } else { y };
                cur_poly.push(to_px(cx, cy));
                last_ctrl = None;
                last_q_ctrl = None;
            }
            'C' | 'c' => {
                let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x3), Some(y3)) = (
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                ) else {
                    break;
                };
                let (c1x, c1y, c2x, c2y, ex, ey) = if cmd == 'c' {
                    (cx + x1, cy + y1, cx + x2, cy + y2, cx + x3, cy + y3)
                } else {
                    (x1, y1, x2, y2, x3, y3)
                };
                flatten_cubic((cx, cy), (c1x, c1y), (c2x, c2y), (ex, ey), &to_px, &mut cur_poly);
                cx = ex;
                cy = ey;
                last_ctrl = Some((c2x, c2y));
                last_q_ctrl = None;
            }
            'S' | 's' => {
                let (Some(x2), Some(y2), Some(x3), Some(y3)) = (
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                ) else {
                    break;
                };
                let (c1x, c1y) = match last_ctrl {
                    Some((lx, ly)) => (2.0 * cx - lx, 2.0 * cy - ly),
                    None => (cx, cy),
                };
                let (c2x, c2y, ex, ey) = if cmd == 's' {
                    (cx + x2, cy + y2, cx + x3, cy + y3)
                } else {
                    (x2, y2, x3, y3)
                };
                flatten_cubic((cx, cy), (c1x, c1y), (c2x, c2y), (ex, ey), &to_px, &mut cur_poly);
                cx = ex;
                cy = ey;
                last_ctrl = Some((c2x, c2y));
                last_q_ctrl = None;
            }
            'Q' | 'q' => {
                let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                ) else {
                    break;
                };
                let (c1x, c1y, ex, ey) = if cmd == 'q' {
                    (cx + x1, cy + y1, cx + x2, cy + y2)
                } else {
                    (x1, y1, x2, y2)
                };
                flatten_quad((cx, cy), (c1x, c1y), (ex, ey), &to_px, &mut cur_poly);
                cx = ex;
                cy = ey;
                last_q_ctrl = Some((c1x, c1y));
                last_ctrl = None;
            }
            'T' | 't' => {
                let (Some(x2), Some(y2)) = (next_num(&mut ti), next_num(&mut ti)) else {
                    break;
                };
                let (c1x, c1y) = match last_q_ctrl {
                    Some((lx, ly)) => (2.0 * cx - lx, 2.0 * cy - ly),
                    None => (cx, cy),
                };
                let (ex, ey) = if cmd == 't' { (cx + x2, cy + y2) } else { (x2, y2) };
                flatten_quad((cx, cy), (c1x, c1y), (ex, ey), &to_px, &mut cur_poly);
                cx = ex;
                cy = ey;
                last_q_ctrl = Some((c1x, c1y));
                last_ctrl = None;
            }
            'A' | 'a' => {
                let (Some(rx), Some(ry), Some(_rot), Some(_large), Some(_sweep), Some(ex), Some(ey)) = (
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                    next_num(&mut ti),
                ) else {
                    break;
                };
                let (nx, ny) = if cmd == 'a' { (cx + ex, cy + ey) } else { (ex, ey) };
                // Approximate elliptical arc midpoint curve cleanly when rx/ry > 0
                let mx = (cx + nx) * 0.5;
                let my = (cy + ny) * 0.5;
                if rx > 0.0 && ry > 0.0 {
                    cur_poly.push(to_px(mx, my));
                }
                cx = nx;
                cy = ny;
                cur_poly.push(to_px(cx, cy));
                last_ctrl = None;
                last_q_ctrl = None;
            }
            _ => break,
        }
    }
    if !cur_poly.is_empty() {
        subpaths.push(cur_poly);
    }
    subpaths
}

fn flatten_cubic(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    to_px: &impl Fn(f32, f32) -> (f32, f32),
    out: &mut Vec<(f32, f32)>,
) {
    const STEPS: usize = 8;
    for s in 1..=STEPS {
        let t = s as f32 / STEPS as f32;
        let u = 1.0 - t;
        let x = u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0;
        let y = u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1;
        out.push(to_px(x, y));
    }
}

fn flatten_quad(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    to_px: &impl Fn(f32, f32) -> (f32, f32),
    out: &mut Vec<(f32, f32)>,
) {
    const STEPS: usize = 6;
    for s in 1..=STEPS {
        let t = s as f32 / STEPS as f32;
        let u = 1.0 - t;
        let x = u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0;
        let y = u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1;
        out.push(to_px(x, y));
    }
}
