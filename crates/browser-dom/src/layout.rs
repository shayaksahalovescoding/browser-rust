use browser_gfx::{Color, FontStyle, Rect, measure_text_styled};

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
    pub font: FontStyle,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlKind {
    TextInput {
        name: String,
        value: String,
        placeholder: String,
    },
    SubmitButton {
        name: String,
        label: String,
    },
}

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub node_id: NodeId,
    pub rect: Rect,
    pub style: ComputedStyle,
    pub text_runs: Vec<TextRun>,
    pub children: Vec<LayoutBox>,
    pub img_src: Option<String>,
    pub img_label: Option<String>,
    pub control: Option<ControlKind>,
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

fn make_img_box(
    doc: &Document,
    st: ComputedStyle,
    id: NodeId,
    box_x: i32,
    box_y: i32,
    avail_w: i32,
    active_href: Option<&str>,
    hits: &mut Vec<HitTarget>,
) -> LayoutBox {
    let alt = doc
        .get_attribute(id, "alt")
        .filter(|s| !s.is_empty())
        .unwrap_or("image");
    let src = doc.get_attribute(id, "src").map(str::to_string);
    let w = resolve_width(st.width, st.max_width, avail_w).unwrap_or(180);
    let h = match st.height {
        Dimension::Px(px) => px.round() as i32,
        _ => 48,
    };
    let rect = Rect::new(box_x, box_y, w.max(1), h.max(1));
    if active_href.is_some() {
        hits.push(HitTarget {
            rect,
            node_id: id,
            href: active_href.map(str::to_string),
        });
    }
    LayoutBox {
        node_id: id,
        rect,
        style: st,
        text_runs: Vec::new(),
        children: Vec::new(),
        img_src: src,
        img_label: Some(alt.to_string()),
        control: None,
    }
}

fn layout_inline_block_element(
    doc: &Document,
    styles: &[ComputedStyle],
    id: NodeId,
    avail_w: i32,
    active_href: Option<&str>,
    hits: &mut Vec<HitTarget>,
) -> LayoutBox {
    let st = styles[id].clone();
    let tag = doc.tag_name(id);
    let own_href = if tag == Some("a") {
        doc.get_attribute(id, "href")
    } else {
        None
    };
    let effective_href = own_href.or(active_href);

    if tag == Some("img") {
        return make_img_box(doc, st, id, 0, 0, avail_w, effective_href, hits);
    }

    let pad_t = st.padding[0].round() as i32;
    let pad_r = st.padding[1].round() as i32;
    let pad_b = st.padding[2].round() as i32;
    let pad_l = st.padding[3].round() as i32;
    let bw = st.border_width.round() as i32;
    let font = st.font_style();

    if tag == Some("input") {
        let ty = doc
            .get_attribute(id, "type")
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_else(|| "text".to_string());
        let name = doc.get_attribute(id, "name").unwrap_or("").to_string();
        let value = doc.get_attribute(id, "value").unwrap_or("").to_string();
        let placeholder = doc
            .get_attribute(id, "placeholder")
            .or_else(|| doc.get_attribute(id, "title"))
            .unwrap_or("")
            .to_string();

        if matches!(ty.as_str(), "submit" | "button" | "reset") {
            let label = if !value.is_empty() {
                value
            } else {
                "Submit".to_string()
            };
            let (tw, th) = measure_text_styled(font, &label);
            let w = resolve_width(st.width, st.max_width, avail_w)
                .unwrap_or(tw + pad_l + pad_r + 2 * bw)
                .min(avail_w.max(40));
            let h = match st.height {
                Dimension::Px(px) => px.round() as i32,
                _ => th + pad_t + pad_b + 2 * bw,
            };
            let rect = Rect::new(0, 0, w, h);
            let run = TextRun {
                x: ((w - tw) / 2).max(pad_l + bw),
                y: ((h - th) / 2).max(pad_t + bw),
                w: tw,
                h: th,
                text: label.clone(),
                font,
                color: st.color,
                underline: false,
                node_id: id,
                href: effective_href.map(str::to_string),
            };
            hits.push(HitTarget {
                rect,
                node_id: id,
                href: effective_href.map(str::to_string),
            });
            return LayoutBox {
                node_id: id,
                rect,
                style: st,
                text_runs: vec![run],
                children: Vec::new(),
                img_src: None,
                img_label: None,
                control: Some(ControlKind::SubmitButton { name, label }),
            };
        }

        let w = resolve_width(st.width, st.max_width, avail_w)
            .unwrap_or(280)
            .min(avail_w.max(60));
        let (_, lh) = measure_text_styled(font, "Ay");
        let h = match st.height {
            Dimension::Px(px) => px.round() as i32,
            _ => (lh + pad_t + pad_b + 2 * bw).max(34),
        };
        let rect = Rect::new(0, 0, w, h);
        hits.push(HitTarget {
            rect,
            node_id: id,
            href: None,
        });
        return LayoutBox {
            node_id: id,
            rect,
            style: st,
            text_runs: Vec::new(),
            children: Vec::new(),
            img_src: None,
            img_label: None,
            control: Some(ControlKind::TextInput {
                name,
                value,
                placeholder,
            }),
        };
    }

    if tag == Some("button") {
        let label_raw = doc.text_content(id);
        let label: String = label_raw.split_whitespace().collect::<Vec<_>>().join(" ");
        let label = if label.is_empty() {
            "Button".to_string()
        } else {
            label
        };
        let name = doc.get_attribute(id, "name").unwrap_or("").to_string();
        let (tw, th) = measure_text_styled(font, &label);
        let w = resolve_width(st.width, st.max_width, avail_w)
            .unwrap_or(tw + pad_l + pad_r + 2 * bw)
            .min(avail_w.max(40));
        let h = match st.height {
            Dimension::Px(px) => px.round() as i32,
            _ => th + pad_t + pad_b + 2 * bw,
        };
        let rect = Rect::new(0, 0, w, h);
        let run = TextRun {
            x: ((w - tw) / 2).max(pad_l + bw),
            y: ((h - th) / 2).max(pad_t + bw),
            w: tw,
            h: th,
            text: label.clone(),
            font,
            color: st.color,
            underline: false,
            node_id: id,
            href: effective_href.map(str::to_string),
        };
        hits.push(HitTarget {
            rect,
            node_id: id,
            href: effective_href.map(str::to_string),
        });
        return LayoutBox {
            node_id: id,
            rect,
            style: st,
            text_runs: vec![run],
            children: Vec::new(),
            img_src: None,
            img_label: None,
            control: Some(ControlKind::SubmitButton { name, label }),
        };
    }

    // Generic CSS inline-block container: layout then shrink-wrap if width is auto
    let mut b = layout_node(doc, styles, id, 0, 0, avail_w, effective_href, hits);
    if matches!(st.width, Dimension::Auto) {
        let mut max_r = 0i32;
        for r in &b.text_runs {
            max_r = max_r.max(r.x + r.w);
        }
        for c in &b.children {
            max_r = max_r.max(c.rect.x + c.rect.w);
        }
        if max_r > 0 {
            b.rect.w = (max_r + pad_r + bw).min(avail_w).max(20);
        }
    }
    b
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

    let max_outer_w = (avail_w - m_left - m_right).max(20);
    let explicit_w = resolve_width(st.width, st.max_width, max_outer_w);
    let box_w = explicit_w.unwrap_or(max_outer_w).min(avail_w.max(20));

    let box_x = match st.margin_auto_x {
        (true, true) => origin_x + ((avail_w - box_w) / 2).max(0),
        (true, false) => origin_x + (avail_w - box_w - m_right).max(0),
        _ if st.center_blocks && explicit_w.is_some() => origin_x + ((avail_w - box_w) / 2).max(0),
        _ => origin_x + m_left,
    };
    let box_y = origin_y + m_top;

    if tag == Some("img") {
        return make_img_box(doc, st, id, box_x, box_y, avail_w, active_href, hits);
    }

    let content_x = box_x + bw + pad_l;
    let content_y = box_y + bw + pad_t;
    let content_w = (box_w - 2 * bw - pad_l - pad_r).max(16);

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
    } else if st.display == Display::Table {
        layout_table_rows(
            doc,
            styles,
            id,
            content_x,
            &mut cur_y,
            content_w,
            active_href,
            hits,
            &mut children_boxes,
        );
    } else {
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
                    &mut children_boxes,
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
                collect_inline_tokens(doc, styles, ch, content_w, active_href, hits, &mut inline_tokens);
            }
        }
        flush_inline_tokens(
            &mut inline_tokens,
            content_x,
            &mut cur_y,
            content_w,
            st.text_align,
            &mut text_runs,
            &mut children_boxes,
            hits,
        );
    }

    let content_h = (cur_y - content_y).max(0);
    let mut box_h = content_h + pad_t + pad_b + 2 * bw;
    if let Dimension::Px(h_px) = st.height {
        box_h = box_h.max(h_px.round() as i32);
    }

    let rect = Rect::new(box_x, box_y, box_w, box_h);
    if active_href.is_some() && !children_boxes.is_empty() && text_runs.is_empty() {
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
        img_src: None,
        img_label: None,
        control: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_table_rows(
    doc: &Document,
    styles: &[ComputedStyle],
    table_id: NodeId,
    content_x: i32,
    cur_y: &mut i32,
    content_w: i32,
    active_href: Option<&str>,
    hits: &mut Vec<HitTarget>,
    out: &mut Vec<LayoutBox>,
) {
    let mut row_ids = Vec::new();
    for &ch in &doc.nodes[table_id].children {
        if styles[ch].display == Display::None {
            continue;
        }
        if styles[ch].display == Display::TableRow {
            row_ids.push(ch);
        } else {
            for &sub in &doc.nodes[ch].children {
                if styles[sub].display == Display::TableRow {
                    row_ids.push(sub);
                }
            }
        }
    }

    for row_id in row_ids {
        let cells: Vec<NodeId> = doc.nodes[row_id]
            .children
            .iter()
            .copied()
            .filter(|&c| styles[c].display == Display::TableCell)
            .collect();
        if cells.is_empty() {
            continue;
        }
        let mut cell_widths = vec![None; cells.len()];
        let mut used_w = 0i32;
        let mut auto_count = 0i32;
        for (i, &cid) in cells.iter().enumerate() {
            if let Some(w) = resolve_width(styles[cid].width, styles[cid].max_width, content_w) {
                let w = w.clamp(16, content_w);
                cell_widths[i] = Some(w);
                used_w += w;
            } else {
                auto_count += 1;
            }
        }
        let rem_w = (content_w - used_w).max(0);
        let auto_w = if auto_count > 0 {
            (rem_w / auto_count).max(40)
        } else {
            40
        };

        let mut cell_x = content_x;
        let mut row_h = 0i32;
        let mut row_boxes = Vec::with_capacity(cells.len());
        for (i, &cid) in cells.iter().enumerate() {
            let cw = cell_widths[i].unwrap_or(auto_w);
            let mut cbox = layout_node(doc, styles, cid, cell_x, *cur_y, cw, active_href, hits);
            cbox.rect.w = cw;
            row_h = row_h.max(cbox.rect.h);
            cell_x += cw;
            row_boxes.push(cbox);
        }
        for cb in &mut row_boxes {
            cb.rect.h = row_h;
        }
        out.extend(row_boxes);
        *cur_y += row_h;
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
    matches!(
        st.display,
        Display::Block | Display::Flex | Display::Table | Display::TableRow | Display::TableCell
    )
}

enum InlineToken {
    Word {
        text: String,
        font: FontStyle,
        color: Color,
        underline: bool,
        node_id: NodeId,
        href: Option<String>,
    },
    InlineBox(Box<LayoutBox>),
    Br,
}

fn collect_inline_tokens(
    doc: &Document,
    styles: &[ComputedStyle],
    id: NodeId,
    avail_w: i32,
    inherited_href: Option<&str>,
    hits: &mut Vec<HitTarget>,
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
            let font = pst.font_style();
            for word in raw.split_whitespace() {
                out.push(InlineToken::Word {
                    text: word.to_string(),
                    font,
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
            if st.display == Display::InlineBlock {
                let ibox = layout_inline_block_element(doc, styles, id, avail_w, next_href, hits);
                out.push(InlineToken::InlineBox(Box::new(ibox)));
                return;
            }
            for &ch in &doc.nodes[id].children {
                collect_inline_tokens(doc, styles, ch, avail_w, next_href, hits, out);
            }
        }
        NodeData::Document => {}
    }
}

enum LineItem {
    Run(TextRun),
    Box(Box<LayoutBox>),
}

#[allow(clippy::too_many_arguments)]
fn flush_inline_tokens(
    tokens: &mut Vec<InlineToken>,
    content_x: i32,
    cur_y: &mut i32,
    content_w: i32,
    align: TextAlign,
    out_runs: &mut Vec<TextRun>,
    out_boxes: &mut Vec<LayoutBox>,
    hits: &mut Vec<HitTarget>,
) {
    if tokens.is_empty() {
        return;
    }
    let mut lines: Vec<(Vec<LineItem>, i32, i32)> = Vec::new();
    let mut line_items: Vec<LineItem> = Vec::new();
    let mut line_w = 0i32;
    let mut line_h = 0i32;

    for tok in tokens.drain(..) {
        match tok {
            InlineToken::Br => {
                lines.push((
                    std::mem::take(&mut line_items),
                    line_w,
                    line_h.max(18),
                ));
                line_w = 0;
                line_h = 0;
            }
            InlineToken::Word {
                text,
                font,
                color,
                underline,
                node_id,
                href,
            } => {
                let (ww, wh) = measure_text_styled(font, &text);
                let space_w = measure_text_styled(font, " ").0.max(4);
                let extra_space = if line_items.is_empty() { 0 } else { space_w };
                if !line_items.is_empty() && line_w + extra_space + ww > content_w {
                    lines.push((
                        std::mem::take(&mut line_items),
                        line_w,
                        line_h.max(wh + 4),
                    ));
                    line_w = 0;
                    line_h = 0;
                }
                let x_off = if line_items.is_empty() {
                    0
                } else {
                    line_w + space_w
                };
                line_items.push(LineItem::Run(TextRun {
                    x: x_off,
                    y: 0,
                    w: ww,
                    h: wh,
                    text,
                    font,
                    color,
                    underline,
                    node_id,
                    href,
                }));
                line_w = x_off + ww;
                line_h = line_h.max(wh + 6);
            }
            InlineToken::InlineBox(mut ibox) => {
                let gap_l = ibox.style.margin[3].round() as i32;
                let gap_r = ibox.style.margin[1].round() as i32;
                let gap_t = ibox.style.margin[0].round() as i32;
                let gap_b = ibox.style.margin[2].round() as i32;
                let bw = ibox.rect.w;
                let bh = ibox.rect.h + gap_t + gap_b;
                let extra_gap = if line_items.is_empty() {
                    gap_l
                } else {
                    6.max(gap_l)
                };
                if !line_items.is_empty() && line_w + extra_gap + bw > content_w {
                    lines.push((
                        std::mem::take(&mut line_items),
                        line_w,
                        line_h.max(bh),
                    ));
                    line_w = 0;
                    line_h = 0;
                }
                let x_off = if line_items.is_empty() {
                    gap_l
                } else {
                    line_w + 6.max(gap_l)
                };
                let dx = x_off - ibox.rect.x;
                let dy = gap_t - ibox.rect.y;
                shift_layout_box(&mut ibox, dx, dy, hits);
                line_items.push(LineItem::Box(ibox));
                line_w = x_off + bw + gap_r;
                line_h = line_h.max(bh + 2);
            }
        }
    }
    if !line_items.is_empty() {
        lines.push((line_items, line_w, line_h.max(20)));
    }

    for (items, lw, lh) in lines {
        let start_x = match align {
            TextAlign::Left => content_x,
            TextAlign::Center => content_x + ((content_w - lw) / 2).max(0),
            TextAlign::Right => content_x + (content_w - lw).max(0),
        };
        for item in items {
            match item {
                LineItem::Run(mut r) => {
                    r.x += start_x;
                    r.y = *cur_y + ((lh - r.h) / 2).max(0);
                    if r.href.is_some() {
                        hits.push(HitTarget {
                            rect: Rect::new(r.x, r.y, r.w, r.h),
                            node_id: r.node_id,
                            href: r.href.clone(),
                        });
                    }
                    out_runs.push(r);
                }
                LineItem::Box(mut b) => {
                    let v_off = ((lh - b.rect.h) / 2).max(0);
                    shift_layout_box(&mut b, start_x, *cur_y + v_off, hits);
                    out_boxes.push(*b);
                }
            }
        }
        *cur_y += lh;
    }
}
