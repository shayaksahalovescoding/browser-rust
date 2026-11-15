use browser_gfx::{Color, GLYPH_H, Rect, measure_text};

use crate::css::{
    AlignItems, ComputedStyle, Dimension, Display, FlexDirection, JustifyContent, TextAlign,
};
use crate::dom::{Document, NodeData, NodeId};

#[derive(Debug, Clone)]
pub struct TextRun {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub text: String,
    pub scale: i32,
    pub color: Color,
    pub underline: bool,
    pub node_id: NodeId,
    pub href: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HitTarget {
    pub rect: Rect,
    pub node_id: NodeId,
    pub href: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub node_id: NodeId,
    pub rect: Rect,
    pub style: ComputedStyle,
    pub text_runs: Vec<TextRun>,
    pub children: Vec<LayoutBox>,
    pub img_label: Option<String>,
}

pub struct LayoutEngine;

impl LayoutEngine {
    pub fn layout(
        doc: &Document,
        styles: &[ComputedStyle],
        viewport_w: i32,
        viewport_h: i32,
    ) -> (LayoutBox, Vec<HitTarget>, i32) {
        let mut hits = Vec::new();
        let root_box = layout_node(doc, styles, 0, 0, 0, viewport_w.max(100), None, &mut hits);
        let content_h = root_box.rect.h.max(viewport_h);
        (root_box, hits, content_h)
    }
}

fn resolve_width(dim: Dimension, max_dim: Dimension, avail_w: i32) -> Option<i32> {
    let mut w = match dim {
        Dimension::Px(px) => Some(px.round() as i32),
        Dimension::Percent(p) => Some(((avail_w as f32) * (p / 100.0)).round() as i32),
        Dimension::Auto => None,
    };
    let max_w = match max_dim {
        Dimension::Px(px) => Some(px.round() as i32),
        Dimension::Percent(p) => Some(((avail_w as f32) * (p / 100.0)).round() as i32),
        Dimension::Auto => None,
    };
    if let Some(mw) = max_w {
        w = Some(w.unwrap_or(avail_w).min(mw));
    }
    w
}

fn layout_node(
    doc: &Document,
    styles: &[ComputedStyle],
    id: NodeId,
    origin_x: i32,
    origin_y: i32,
    avail_w: i32,
    inherited_href: Option<&str>,
    hits: &mut Vec<HitTarget>,
) -> LayoutBox {
    let st = styles[id].clone();
    let tag = doc.tag_name(id);
    let own_href = if tag == Some("a") {
        doc.get_attribute(id, "href")
    } else {
        None
    };
    let active_href = own_href.or(inherited_href);

    let m_top = st.margin[0].round() as i32;
    let m_right = st.margin[1].round() as i32;
    let m_bottom = st.margin[2].round() as i32;
    let m_left = st.margin[3].round() as i32;
    let pad_t = st.padding[0].round() as i32;
    let pad_r = st.padding[1].round() as i32;
    let pad_b = st.padding[2].round() as i32;
    let pad_l = st.padding[3].round() as i32;
    let bw = st.border_width.round() as i32;

    let max_outer_w = (avail_w - m_left - m_right).max(40);
    let box_w = resolve_width(st.width, st.max_width, max_outer_w)
        .unwrap_or(max_outer_w)
        .min(avail_w.max(40));

    let box_x = match st.margin_auto_x {
        (true, true) => origin_x + ((avail_w - box_w) / 2).max(0),
        (true, false) => origin_x + (avail_w - box_w - m_right).max(0),
        _ => origin_x + m_left,
    };
    let box_y = origin_y + m_top;

    // Handle <img> / <input> / <hr> leaf elements cleanly
    if tag == Some("img") {
        let alt = doc
            .get_attribute(id, "alt")
            .filter(|s| !s.is_empty())
            .unwrap_or("image");
        let w = resolve_width(st.width, st.max_width, avail_w).unwrap_or(180);
        let h = match st.height {
            Dimension::Px(px) => px.round() as i32,
            _ => 48,
        };
        let rect = Rect::new(box_x, box_y, w, h);
        if active_href.is_some() {
            hits.push(HitTarget {
                rect,
                node_id: id,
                href: active_href.map(str::to_string),
            });
        }
        return LayoutBox {
            node_id: id,
            rect,
            style: st,
            text_runs: Vec::new(),
            children: Vec::new(),
            img_label: Some(format!("[img: {alt}]")),
        };
    }

    let content_x = box_x + bw + pad_l;
    let content_y = box_y + bw + pad_t;
    let content_w = (box_w - 2 * bw - pad_l - pad_r).max(20);

    let mut children_boxes = Vec::new();
    let mut text_runs = Vec::new();
    let mut cur_y = content_y;

    if st.display == Display::Flex {
        layout_flex_children(
            doc,
            styles,
            id,
            &st,
            content_x,
            content_y,
            content_w,
            active_href,
            hits,
            &mut children_boxes,
            &mut cur_y,
        );
    } else {
        // Mixed block + inline children flow
        let mut inline_tokens: Vec<InlineToken> = Vec::new();
        let children = &doc.nodes[id].children;
        for &ch in children {
            let ch_st = &styles[ch];
            if ch_st.display == Display::None {
                continue;
            }
            if is_block_like(doc, ch, ch_st) {
                flush_inline_tokens(
                    &mut inline_tokens,
                    content_x,
                    &mut cur_y,
                    content_w,
                    st.text_align,
                    &mut text_runs,
                    hits,
                );
                let cbox = layout_node(
                    doc,
                    styles,
                    ch,
                    content_x,
                    cur_y,
                    content_w,
                    active_href,
                    hits,
                );
                let cm_bottom = ch_st.margin[2].round() as i32;
                cur_y = cbox.rect.y + cbox.rect.h + cm_bottom;
                children_boxes.push(cbox);
            } else {
                collect_inline_tokens(doc, styles, ch, active_href, &mut inline_tokens);
            }
        }
        flush_inline_tokens(
            &mut inline_tokens,
            content_x,
            &mut cur_y,
            content_w,
            st.text_align,
            &mut text_runs,
            hits,
        );
    }

    let content_h = (cur_y - content_y).max(0);
    let mut box_h = content_h + pad_t + pad_b + 2 * bw;
    if let Dimension::Px(h_px) = st.height {
        box_h = box_h.max(h_px.round() as i32);
    }

    let rect = Rect::new(box_x, box_y, box_w, box_h);
    if matches!(tag, Some("button") | Some("input")) || active_href.is_some() {
        hits.push(HitTarget {
            rect,
            node_id: id,
            href: active_href.map(str::to_string),
        });
    }
    let _ = m_bottom;

    LayoutBox {
        node_id: id,
        rect,
        style: st,
        text_runs,
        children: children_boxes,
        img_label: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_flex_children(
    doc: &Document,
    styles: &[ComputedStyle],
    id: NodeId,
    st: &ComputedStyle,
    content_x: i32,
    content_y: i32,
    content_w: i32,
    active_href: Option<&str>,
    hits: &mut Vec<HitTarget>,
    out: &mut Vec<LayoutBox>,
    cur_y: &mut i32,
) {
    let gap = st.gap.round() as i32;
    let visible_children: Vec<NodeId> = doc.nodes[id]
        .children
        .iter()
        .copied()
        .filter(|&c| {
            styles[c].display != Display::None
                && !matches!(&doc.nodes[c].data, NodeData::Text(t) if t.trim().is_empty())
        })
        .collect();

    if visible_children.is_empty() {
        return;
    }

    if st.flex_direction == FlexDirection::Column {
        for (i, ch) in visible_children.iter().enumerate() {
            let cbox = layout_node(
                doc,
                styles,
                *ch,
                content_x,
                *cur_y,
                content_w,
                active_href,
                hits,
            );
            *cur_y = cbox.rect.y + cbox.rect.h + styles[*ch].margin[2].round() as i32;
            if i + 1 < visible_children.len() {
                *cur_y += gap;
            }
            out.push(cbox);
        }
        return;
    }

    // Flex Row
    let n = visible_children.len() as i32;
    let total_gap = gap * (n - 1).max(0);
    let slot_w = ((content_w - total_gap) / n.max(1)).max(40);
    let mut temp_boxes = Vec::with_capacity(visible_children.len());
    let mut total_used_w = total_gap;
    let mut max_h = 0;

    for &ch in &visible_children {
        let ch_st = &styles[ch];
        let explicit_w = resolve_width(ch_st.width, ch_st.max_width, content_w);
        let child_avail = explicit_w.unwrap_or(slot_w);
        let cbox = layout_node(
            doc,
            styles,
            ch,
            0,
            content_y,
            child_avail,
            active_href,
            hits,
        );
        total_used_w += cbox.rect.w;
        max_h = max_h.max(cbox.rect.h);
        temp_boxes.push(cbox);
    }

    let extra = (content_w - total_used_w).max(0);
    let (mut x, extra_gap) = match st.justify_content {
        JustifyContent::Start => (content_x, 0),
        JustifyContent::Center => (content_x + extra / 2, 0),
        JustifyContent::End => (content_x + extra, 0),
        JustifyContent::SpaceBetween if temp_boxes.len() > 1 => {
            (content_x, extra / (temp_boxes.len() as i32 - 1))
        }
        JustifyContent::SpaceBetween => (content_x, 0),
    };

    for mut cbox in temp_boxes {
        let dx = x - cbox.rect.x;
        let dy = match st.align_items {
            AlignItems::Center => (max_h - cbox.rect.h) / 2,
            AlignItems::Start | AlignItems::Stretch => 0,
        };
        shift_layout_box(&mut cbox, dx, dy, hits);
        x += cbox.rect.w + gap + extra_gap;
        out.push(cbox);
    }
    *cur_y = content_y + max_h;
}

fn shift_layout_box(b: &mut LayoutBox, dx: i32, dy: i32, hits: &mut [HitTarget]) {
    if dx == 0 && dy == 0 {
        return;
    }
    let old_rect = b.rect;
    b.rect.x += dx;
    b.rect.y += dy;
    for r in &mut b.text_runs {
        r.x += dx;
        r.y += dy;
    }
    for h in hits.iter_mut() {
        if h.node_id == b.node_id && h.rect == old_rect {
            h.rect.x += dx;
            h.rect.y += dy;
        }
    }
    for ch in &mut b.children {
        shift_layout_box(ch, dx, dy, hits);
    }
}

fn is_block_like(doc: &Document, id: NodeId, st: &ComputedStyle) -> bool {
    if matches!(&doc.nodes[id].data, NodeData::Text(_)) {
        return false;
    }
    matches!(st.display, Display::Block | Display::Flex)
}

enum InlineToken {
    Word {
        text: String,
        scale: i32,
        color: Color,
        underline: bool,
        node_id: NodeId,
        href: Option<String>,
    },
    Br,
}

fn collect_inline_tokens(
    doc: &Document,
    styles: &[ComputedStyle],
    id: NodeId,
    inherited_href: Option<&str>,
    out: &mut Vec<InlineToken>,
) {
    let st = &styles[id];
    if st.display == Display::None {
        return;
    }
    match &doc.nodes[id].data {
        NodeData::Text(raw) => {
            let parent_id = doc.nodes[id].parent.unwrap_or(0);
            let pst = &styles[parent_id];
            for word in raw.split_whitespace() {
                out.push(InlineToken::Word {
                    text: word.to_string(),
                    scale: pst.font_scale,
                    color: pst.color,
                    underline: pst.underline,
                    node_id: parent_id,
                    href: inherited_href.map(str::to_string),
                });
            }
        }
        NodeData::Element { tag, .. } => {
            if tag == "br" {
                out.push(InlineToken::Br);
                return;
            }
            let own_href = if tag == "a" {
                doc.get_attribute(id, "href")
            } else {
                None
            };
            let next_href = own_href.or(inherited_href);
            if tag == "input" {
                let val = doc
                    .get_attribute(id, "value")
                    .or_else(|| doc.get_attribute(id, "placeholder"))
                    .unwrap_or("[input]");
                out.push(InlineToken::Word {
                    text: format!("[{val}]"),
                    scale: st.font_scale,
                    color: st.color,
                    underline: false,
                    node_id: id,
                    href: next_href.map(str::to_string),
                });
                return;
            }
            for &ch in &doc.nodes[id].children {
                collect_inline_tokens(doc, styles, ch, next_href, out);
            }
        }
        NodeData::Document => {}
    }
}

fn flush_inline_tokens(
    tokens: &mut Vec<InlineToken>,
    content_x: i32,
    cur_y: &mut i32,
    content_w: i32,
    align: TextAlign,
    out_runs: &mut Vec<TextRun>,
    hits: &mut Vec<HitTarget>,
) {
    if tokens.is_empty() {
        return;
    }
    let mut lines: Vec<(Vec<TextRun>, i32, i32)> = Vec::new();
    let mut line_runs: Vec<TextRun> = Vec::new();
    let mut line_w = 0i32;
    let mut line_h = 0i32;

    for tok in tokens.drain(..) {
        match tok {
            InlineToken::Br => {
                lines.push((
                    std::mem::take(&mut line_runs),
                    line_w,
                    line_h.max(16),
                ));
                line_w = 0;
                line_h = 0;
            }
            InlineToken::Word {
                text,
                scale,
                color,
                underline,
                node_id,
                href,
            } => {
                let (ww, wh) = measure_text(scale, &text);
                let space_w = (8 * scale) / 2;
                let extra_space = if line_runs.is_empty() { 0 } else { space_w };
                if !line_runs.is_empty() && line_w + extra_space + ww > content_w {
                    lines.push((
                        std::mem::take(&mut line_runs),
                        line_w,
                        line_h.max(wh),
                    ));
                    line_w = 0;
                    line_h = 0;
                }
                let x_off = if line_runs.is_empty() {
                    0
                } else {
                    line_w + space_w
                };
                line_runs.push(TextRun {
                    x: x_off,
                    y: 0,
                    w: ww,
                    h: (GLYPH_H as i32) * scale,
                    text,
                    scale,
                    color,
                    underline,
                    node_id,
                    href,
                });
                line_w = x_off + ww;
                line_h = line_h.max(wh + 6);
            }
        }
    }
    if !line_runs.is_empty() {
        lines.push((line_runs, line_w, line_h.max(18)));
    }

    for (mut runs, lw, lh) in lines {
        let start_x = match align {
            TextAlign::Left => content_x,
            TextAlign::Center => content_x + ((content_w - lw) / 2).max(0),
            TextAlign::Right => content_x + (content_w - lw).max(0),
        };
        for r in &mut runs {
            r.x += start_x;
            r.y = *cur_y;
            if r.href.is_some() {
                hits.push(HitTarget {
                    rect: Rect::new(r.x, r.y, r.w, r.h),
                    node_id: r.node_id,
                    href: r.href.clone(),
                });
            }
        }
        out_runs.extend(runs);
        *cur_y += lh;
    }
}
