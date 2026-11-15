use std::collections::HashMap;

use browser_gfx::{Color, FontStyle};

use crate::dom::{Document, NodeData, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    InlineBlock,
    Flex,
    Table,
    TableRow,
    TableCell,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    Start,
    Center,
    End,
    SpaceBetween,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Start,
    Center,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    Auto,
    Px(f32),
    Percent(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub struct ComputedStyle {
    pub display: Display,
    pub flex_direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub gap: f32,
    pub width: Dimension,
    pub max_width: Dimension,
    pub height: Dimension,
    pub margin: [f32; 4], // top, right, bottom, left
    pub margin_auto_x: (bool, bool),
    pub padding: [f32; 4],
    pub border_width: f32,
    pub border_color: Color,
    pub border_radius: i32,
    pub background: Option<Color>,
    pub color: Color,
    pub font_scale: i32,
    pub font_px: i32,
    pub font_bold: bool,
    pub font_monospace: bool,
    pub text_align: TextAlign,
    pub center_blocks: bool,
    pub underline: bool,
    pub list_bullet: bool,
}

impl ComputedStyle {
    pub fn font_style(&self) -> FontStyle {
        FontStyle::new(self.font_px, self.font_bold, self.font_monospace)
    }
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Stretch,
            gap: 0.0,
            width: Dimension::Auto,
            max_width: Dimension::Auto,
            height: Dimension::Auto,
            margin: [0.0; 4],
            margin_auto_x: (false, false),
            padding: [0.0; 4],
            border_width: 0.0,
            border_color: Color::rgb(200, 200, 200),
            border_radius: 0,
            background: None,
            color: Color::rgb(32, 33, 36),
            font_scale: 2,
            font_px: 14,
            font_bold: false,
            font_monospace: false,
            text_align: TextAlign::Left,
            center_blocks: false,
            underline: false,
            list_bullet: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct SimpleSelector {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
}

#[derive(Debug, Clone)]
pub struct ComplexSelector {
    /// Stored right-to-left? No, stored left-to-right with combinator to previous part.
    pub parts: Vec<(Option<Combinator>, SimpleSelector)>,
}

impl ComplexSelector {
    pub fn specificity(&self) -> (u32, u32, u32) {
        let mut ids = 0u32;
        let mut classes = 0u32;
        let mut tags = 0u32;
        for (_, p) in &self.parts {
            if p.id.is_some() {
                ids += 1;
            }
            classes += p.classes.len() as u32;
            if p.tag.as_deref().is_some_and(|t| t != "*") {
                tags += 1;
            }
        }
        (ids, classes, tags)
    }
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub selectors: Vec<ComplexSelector>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

impl Stylesheet {
    pub fn parse(css: &str) -> Self {
        let stripped = strip_comments(css);
        let mut rules = Vec::new();
        parse_rule_block(&stripped, &mut rules);
        Self { rules }
    }
}

fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            if let Some(end) = s[i + 2..].find("*/") {
                i += 2 + end + 2;
                continue;
            } else {
                break;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn parse_rule_block(s: &str, out: &mut Vec<Rule>) {
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let header_start = i;
        while i < bytes.len() && bytes[i] != b'{' && bytes[i] != b'}' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'}' {
            i += 1;
            continue;
        }
        let header = s[header_start..i].trim();
        i += 1; // skip '{'
        let body_start = i;
        let mut depth = 1;
        while i < bytes.len() && depth > 0 {
            if bytes[i] == b'{' {
                depth += 1;
            } else if bytes[i] == b'}' {
                depth -= 1;
            }
            i += 1;
        }
        let body_end = if depth == 0 { i - 1 } else { i };
        let body = &s[body_start..body_end];

        if header.starts_with('@') {
            if header.to_ascii_lowercase().starts_with("@media")
                && !header.to_ascii_lowercase().contains("print")
                && !header.to_ascii_lowercase().contains("prefers-color-scheme: dark")
            {
                parse_rule_block(body, out);
            }
            continue;
        }

        let selectors: Vec<ComplexSelector> = header
            .split(',')
            .filter_map(|sel| parse_complex_selector(sel.trim()))
            .collect();
        if !selectors.is_empty() {
            let declarations = parse_declarations(body);
            out.push(Rule {
                selectors,
                declarations,
            });
        }
    }
}

pub fn parse_declarations(s: &str) -> Vec<Declaration> {
    s.split(';')
        .filter_map(|item| {
            let (prop, val) = item.split_once(':')?;
            let property = prop.trim().to_ascii_lowercase();
            let value = val.trim().trim_end_matches("!important").trim().to_string();
            if property.is_empty() || value.is_empty() {
                None
            } else {
                Some(Declaration { property, value })
            }
        })
        .collect()
}

fn parse_complex_selector(sel: &str) -> Option<ComplexSelector> {
    if sel.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    let mut pending_comb = None;
    let mut cur = String::new();
    for tok in sel.split_whitespace() {
        if tok == ">" {
            if !cur.is_empty() {
                parts.push((pending_comb.take(), parse_simple_selector(&cur)?));
                cur.clear();
            }
            pending_comb = Some(Combinator::Child);
        } else {
            if !cur.is_empty() {
                parts.push((pending_comb.take(), parse_simple_selector(&cur)?));
                cur.clear();
                pending_comb = Some(Combinator::Descendant);
            }
            cur = tok.to_string();
        }
    }
    if !cur.is_empty() {
        parts.push((pending_comb, parse_simple_selector(&cur)?));
    }
    if parts.is_empty() {
        None
    } else {
        Some(ComplexSelector { parts })
    }
}

fn parse_simple_selector(raw: &str) -> Option<SimpleSelector> {
    // Strip pseudo-classes like :hover / :visited for basic matching
    let s = raw.split(':').next().unwrap_or("").trim();
    if s.is_empty() {
        return None;
    }
    let mut tag = None;
    let mut id = None;
    let mut classes = Vec::new();

    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'#' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'#' {
                i += 1;
            }
            id = Some(s[start..i].to_string());
        } else if bytes[i] == b'.' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'#' {
                i += 1;
            }
            classes.push(s[start..i].to_string());
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'#' {
                i += 1;
            }
            tag = Some(s[start..i].to_ascii_lowercase());
        }
    }
    Some(SimpleSelector { tag, id, classes })
}

fn matches_simple(doc: &Document, id: NodeId, sel: &SimpleSelector) -> bool {
    let NodeData::Element { tag, attrs } = &doc.nodes[id].data else {
        return false;
    };
    if let Some(t) = &sel.tag {
        if t != "*" && t != tag {
            return false;
        }
    }
    if let Some(sid) = &sel.id {
        if attrs.get("id").map(|s| s.as_str()) != Some(sid.as_str()) {
            return false;
        }
    }
    if !sel.classes.is_empty() {
        let Some(cls_attr) = attrs.get("class") else {
            return false;
        };
        for c in &sel.classes {
            if !cls_attr.split_whitespace().any(|x| x == c) {
                return false;
            }
        }
    }
    true
}

fn matches_complex(doc: &Document, id: NodeId, sel: &ComplexSelector) -> bool {
    let Some((_, last)) = sel.parts.last() else {
        return false;
    };
    if !matches_simple(doc, id, last) {
        return false;
    }
    let mut cur_node = id;
    for idx in (0..sel.parts.len() - 1).rev() {
        let comb = sel.parts[idx + 1].0.unwrap_or(Combinator::Descendant);
        let target = &sel.parts[idx].1;
        match comb {
            Combinator::Child => {
                let Some(p) = doc.nodes[cur_node].parent else {
                    return false;
                };
                if !matches_simple(doc, p, target) {
                    return false;
                }
                cur_node = p;
            }
            Combinator::Descendant => {
                let mut found = None;
                let mut anc = doc.nodes[cur_node].parent;
                while let Some(a) = anc {
                    if matches_simple(doc, a, target) {
                        found = Some(a);
                        break;
                    }
                    anc = doc.nodes[a].parent;
                }
                let Some(f) = found else {
                    return false;
                };
                cur_node = f;
            }
        }
    }
    true
}

pub fn compute_styles(doc: &Document, sheets: &[Stylesheet]) -> Vec<ComputedStyle> {
    let mut styles = vec![ComputedStyle::default(); doc.nodes.len()];
    compute_node_recursive(doc, 0, &ComputedStyle::default(), sheets, &mut styles);
    styles
}

fn compute_node_recursive(
    doc: &Document,
    id: NodeId,
    parent_style: &ComputedStyle,
    sheets: &[Stylesheet],
    out: &mut [ComputedStyle],
) {
    let mut st = ComputedStyle {
        color: parent_style.color,
        font_scale: parent_style.font_scale,
        font_px: parent_style.font_px,
        font_bold: parent_style.font_bold,
        font_monospace: parent_style.font_monospace,
        text_align: parent_style.text_align,
        center_blocks: parent_style.center_blocks,
        underline: parent_style.underline,
        ..ComputedStyle::default()
    };

    if let NodeData::Element { tag, attrs } = &doc.nodes[id].data {
        apply_user_agent_defaults(tag, attrs, &mut st);

        let mut matched: Vec<((u32, u32, u32), usize, &[Declaration])> = Vec::new();
        let mut order = 0usize;
        for sheet in sheets {
            for rule in &sheet.rules {
                for sel in &rule.selectors {
                    if matches_complex(doc, id, sel) {
                        matched.push((sel.specificity(), order, &rule.declarations));
                    }
                    order += 1;
                }
            }
        }
        matched.sort_by_key(|(spec, ord, _)| (*spec, *ord));
        for (_, _, decls) in matched {
            for d in decls {
                apply_declaration(&d.property, &d.value, &mut st);
            }
        }

        if let Some(inline_css) = attrs.get("style") {
            for d in parse_declarations(inline_css) {
                apply_declaration(&d.property, &d.value, &mut st);
            }
        }
    }

    out[id] = st.clone();
    for &ch in &doc.nodes[id].children {
        compute_node_recursive(doc, ch, &st, sheets, out);
    }
}

fn apply_user_agent_defaults(
    tag: &str,
    attrs: &HashMap<String, String>,
    st: &mut ComputedStyle,
) {
    match tag {
        "head" | "style" | "script" | "title" | "meta" | "link" | "noscript" | "template"
        | "svg" | "path" | "g" => {
            st.display = Display::None;
            return;
        }
        "html" => {
            st.display = Display::Block;
            st.background = Some(Color::rgb(255, 255, 255));
        }
        "body" => {
            st.display = Display::Block;
            st.margin = [8.0, 8.0, 8.0, 8.0];
        }
        "center" => {
            st.display = Display::Block;
            st.text_align = TextAlign::Center;
            st.center_blocks = true;
        }
        "div" | "section" | "article" | "header" | "footer" | "main" | "nav" | "form"
        | "blockquote" | "pre" => {
            st.display = Display::Block;
            if tag == "pre" {
                st.background = Some(Color::rgb(245, 247, 250));
                st.padding = [10.0; 4];
                st.font_monospace = true;
            }
        }
        "code" | "kbd" | "tt" => {
            st.font_monospace = true;
        }
        "b" | "strong" => {
            st.font_bold = true;
        }
        "small" => {
            st.font_px = 12;
        }
        "table" => {
            st.display = Display::Table;
        }
        "tr" => {
            st.display = Display::TableRow;
        }
        "td" | "th" => {
            st.display = Display::TableCell;
            st.padding = [2.0, 4.0, 2.0, 4.0];
            if tag == "th" {
                st.font_bold = true;
            }
        }
        "tbody" | "thead" | "tfoot" => {
            st.display = Display::Block;
        }
        "h1" => {
            st.display = Display::Block;
            st.font_scale = 4;
            st.font_px = 30;
            st.font_bold = true;
            st.margin = [16.0, 0.0, 12.0, 0.0];
        }
        "h2" => {
            st.display = Display::Block;
            st.font_scale = 3;
            st.font_px = 22;
            st.font_bold = true;
            st.margin = [14.0, 0.0, 10.0, 0.0];
        }
        "h3" | "h4" => {
            st.display = Display::Block;
            st.font_scale = 2;
            st.font_px = 17;
            st.font_bold = true;
            st.margin = [10.0, 0.0, 8.0, 0.0];
        }
        "p" => {
            st.display = Display::Block;
            st.margin = [8.0, 0.0, 10.0, 0.0];
        }
        "ul" | "ol" => {
            st.display = Display::Block;
            st.margin = [8.0, 0.0, 8.0, 0.0];
            st.padding = [0.0, 0.0, 0.0, 24.0];
        }
        "li" => {
            st.display = Display::Block;
            st.margin = [4.0, 0.0, 4.0, 0.0];
            st.list_bullet = true;
        }
        "hr" => {
            st.display = Display::Block;
            st.height = Dimension::Px(1.0);
            st.margin = [12.0, 0.0, 12.0, 0.0];
            st.background = Some(Color::rgb(218, 220, 224));
        }
        "a" => {
            st.display = Display::Inline;
            st.color = Color::rgb(26, 115, 232);
            st.underline = true;
        }
        "img" => {
            st.display = Display::InlineBlock;
        }
        "button" => {
            st.display = Display::InlineBlock;
            st.padding = [7.0, 16.0, 7.0, 16.0];
            st.margin = [4.0, 4.0, 4.0, 4.0];
            st.border_width = 1.0;
            st.border_color = Color::rgb(218, 220, 224);
            st.border_radius = 4;
            st.background = Some(Color::rgb(248, 249, 250));
            st.color = Color::rgb(60, 64, 67);
            st.font_px = 14;
        }
        "input" => {
            let ty = attrs
                .get("type")
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_else(|| "text".to_string());
            match ty.as_str() {
                "hidden" => {
                    st.display = Display::None;
                    return;
                }
                "submit" | "button" | "reset" => {
                    st.display = Display::InlineBlock;
                    st.padding = [7.0, 16.0, 7.0, 16.0];
                    st.margin = [6.0, 4.0, 6.0, 4.0];
                    st.border_width = 1.0;
                    st.border_color = Color::rgb(218, 220, 224);
                    st.border_radius = 4;
                    st.background = Some(Color::rgb(248, 249, 250));
                    st.color = Color::rgb(60, 64, 67);
                    st.font_px = 14;
                }
                _ => {
                    st.display = Display::InlineBlock;
                    st.padding = [8.0, 14.0, 8.0, 14.0];
                    st.margin = [4.0, 4.0, 4.0, 4.0];
                    st.border_width = 1.0;
                    st.border_color = Color::rgb(218, 220, 224);
                    st.border_radius = 20;
                    st.background = Some(Color::rgb(255, 255, 255));
                    st.color = Color::rgb(32, 33, 36);
                    st.font_px = 14;
                    if let Some(sz) = attrs.get("size").and_then(|s| s.trim().parse::<f32>().ok()) {
                        let px = (sz * 7.8 + 28.0).clamp(120.0, 560.0);
                        st.width = Dimension::Px(px);
                    } else {
                        st.width = Dimension::Px(260.0);
                    }
                }
            }
        }
        _ => {}
    }

    if attrs.contains_key("hidden") {
        st.display = Display::None;
        return;
    }
    if let Some(bg) = attrs.get("bgcolor") {
        if let Some(c) = parse_color(&bg.trim().to_ascii_lowercase()) {
            st.background = Some(c);
        }
    }
    if let Some(align) = attrs.get("align") {
        match align.trim().to_ascii_lowercase().as_str() {
            "center" | "middle" => {
                st.text_align = TextAlign::Center;
                st.center_blocks = true;
            }
            "right" => st.text_align = TextAlign::Right,
            "left" => st.text_align = TextAlign::Left,
            _ => {}
        }
    }
    if matches!(tag, "img" | "table" | "td" | "th") {
        if let Some(w) = attrs.get("width") {
            let dim = parse_html_dim(w);
            if !matches!(dim, Dimension::Auto) {
                st.width = dim;
            }
        }
        if let Some(h) = attrs.get("height") {
            let dim = parse_html_dim(h);
            if !matches!(dim, Dimension::Auto) {
                st.height = dim;
            }
        }
    }
}

fn parse_html_dim(raw: &str) -> Dimension {
    let s = raw.trim();
    if let Some(pct) = s.strip_suffix('%') {
        pct.trim()
            .parse::<f32>()
            .map(Dimension::Percent)
            .unwrap_or(Dimension::Auto)
    } else if let Ok(px) = s.trim_end_matches("px").trim().parse::<f32>() {
        Dimension::Px(px)
    } else {
        Dimension::Auto
    }
}

fn apply_declaration(prop: &str, val: &str, st: &mut ComputedStyle) {
    let v = val.trim().to_ascii_lowercase();
    match prop {
        "display" => {
            st.display = match v.as_str() {
                "block" => Display::Block,
                "inline" => Display::Inline,
                "inline-block" => Display::InlineBlock,
                "flex" => Display::Flex,
                "table" => Display::Table,
                "table-row" => Display::TableRow,
                "table-cell" => Display::TableCell,
                "none" => Display::None,
                _ => st.display,
            };
        }
        "flex-direction" => {
            if v == "column" {
                st.flex_direction = FlexDirection::Column;
            } else if v == "row" {
                st.flex_direction = FlexDirection::Row;
            }
        }
        "justify-content" => {
            st.justify_content = match v.as_str() {
                "center" => JustifyContent::Center,
                "flex-end" | "end" => JustifyContent::End,
                "space-between" => JustifyContent::SpaceBetween,
                _ => JustifyContent::Start,
            };
        }
        "align-items" => {
            st.align_items = match v.as_str() {
                "center" => AlignItems::Center,
                "flex-start" | "start" => AlignItems::Start,
                _ => AlignItems::Stretch,
            };
        }
        "gap" => {
            if let Some(px) = parse_px(&v) {
                st.gap = px;
            }
        }
        "width" => st.width = parse_dim(&v),
        "max-width" => st.max_width = parse_dim(&v),
        "height" => st.height = parse_dim(&v),
        "color" => {
            if let Some(c) = parse_color(&v) {
                st.color = c;
            }
        }
        "background" | "background-color" => {
            if v == "transparent" || v == "none" {
                st.background = None;
            } else if let Some(c) = parse_color(&v) {
                st.background = Some(c);
            }
        }
        "font-size" => {
            if let Some(px) = parse_px(&v) {
                st.font_px = (px.round() as i32).clamp(10, 48);
                st.font_scale = if px >= 28.0 {
                    4
                } else if px >= 20.0 {
                    3
                } else if px >= 12.0 {
                    2
                } else {
                    1
                };
            }
        }
        "font-weight" => {
            st.font_bold = v == "bold"
                || v == "bolder"
                || v.parse::<i32>().map(|w| w >= 600).unwrap_or(false);
        }
        "font-family" => {
            st.font_monospace = v.contains("mono") || v.contains("consolas") || v.contains("courier");
        }
        "font" => {
            for tok in v.split_whitespace() {
                if tok == "bold" {
                    st.font_bold = true;
                }
                let size_tok = tok.split('/').next().unwrap_or(tok);
                if let Some(px) = parse_px(size_tok) {
                    st.font_px = (px.round() as i32).clamp(10, 48);
                }
                if tok.contains("mono") || tok.contains("consolas") || tok.contains("courier") {
                    st.font_monospace = true;
                }
            }
        }
        "text-align" => {
            st.text_align = match v.as_str() {
                "center" => TextAlign::Center,
                "right" => TextAlign::Right,
                _ => TextAlign::Left,
            };
        }
        "text-decoration" => {
            st.underline = v.contains("underline");
        }
        "border-radius" => {
            if let Some(px) = parse_px(&v) {
                st.border_radius = px.round() as i32;
            }
        }
        "border" => {
            for tok in v.split_whitespace() {
                if let Some(px) = parse_px(tok) {
                    st.border_width = px;
                } else if let Some(c) = parse_color(tok) {
                    st.border_color = c;
                }
            }
        }
        "border-color" => {
            if let Some(c) = parse_color(&v) {
                st.border_color = c;
            }
        }
        "margin" => parse_box_shorthand(&v, &mut st.margin, Some(&mut st.margin_auto_x)),
        "margin-top" => {
            if let Some(px) = parse_px(&v) {
                st.margin[0] = px;
            }
        }
        "margin-right" => {
            if v == "auto" {
                st.margin_auto_x.1 = true;
            } else if let Some(px) = parse_px(&v) {
                st.margin[1] = px;
            }
        }
        "margin-bottom" => {
            if let Some(px) = parse_px(&v) {
                st.margin[2] = px;
            }
        }
        "margin-left" => {
            if v == "auto" {
                st.margin_auto_x.0 = true;
            } else if let Some(px) = parse_px(&v) {
                st.margin[3] = px;
            }
        }
        "padding" => parse_box_shorthand(&v, &mut st.padding, None),
        "padding-top" => {
            if let Some(px) = parse_px(&v) {
                st.padding[0] = px;
            }
        }
        "padding-right" => {
            if let Some(px) = parse_px(&v) {
                st.padding[1] = px;
            }
        }
        "padding-bottom" => {
            if let Some(px) = parse_px(&v) {
                st.padding[2] = px;
            }
        }
        "padding-left" => {
            if let Some(px) = parse_px(&v) {
                st.padding[3] = px;
            }
        }
        _ => {}
    }
}

fn parse_box_shorthand(v: &str, out: &mut [f32; 4], auto_x: Option<&mut (bool, bool)>) {
    let toks: Vec<&str> = v.split_whitespace().collect();
    if toks.is_empty() {
        return;
    }
    let val = |idx: usize| -> (f32, bool) {
        let t = toks[idx];
        if t == "auto" {
            (0.0, true)
        } else {
            (parse_px(t).unwrap_or(0.0), false)
        }
    };
    match toks.len() {
        1 => {
            let (a, is_auto) = val(0);
            *out = [a, a, a, a];
            if let Some(ax) = auto_x {
                *ax = (is_auto, is_auto);
            }
        }
        2 => {
            let (tb, _) = val(0);
            let (lr, is_auto) = val(1);
            *out = [tb, lr, tb, lr];
            if let Some(ax) = auto_x {
                *ax = (is_auto, is_auto);
            }
        }
        3 => {
            let (t, _) = val(0);
            let (lr, is_auto) = val(1);
            let (b, _) = val(2);
            *out = [t, lr, b, lr];
            if let Some(ax) = auto_x {
                *ax = (is_auto, is_auto);
            }
        }
        _ => {
            let (t, _) = val(0);
            let (r, ar) = val(1);
            let (b, _) = val(2);
            let (l, al) = val(3);
            *out = [t, r, b, l];
            if let Some(ax) = auto_x {
                *ax = (al, ar);
            }
        }
    }
}

fn parse_dim(v: &str) -> Dimension {
    if v == "auto" {
        Dimension::Auto
    } else if let Some(p) = v.strip_suffix('%') {
        p.parse::<f32>()
            .map(Dimension::Percent)
            .unwrap_or(Dimension::Auto)
    } else if let Some(px) = parse_px(v) {
        Dimension::Px(px)
    } else {
        Dimension::Auto
    }
}

fn parse_px(v: &str) -> Option<f32> {
    if let Some(n) = v.strip_suffix("px") {
        n.trim().parse().ok()
    } else if let Some(n) = v.strip_suffix("pt") {
        n.trim().parse::<f32>().ok().map(|x| x * (4.0 / 3.0))
    } else if let Some(n) = v.strip_suffix("em") {
        n.trim().parse::<f32>().ok().map(|x| x * 16.0)
    } else if let Some(n) = v.strip_suffix("rem") {
        n.trim().parse::<f32>().ok().map(|x| x * 16.0)
    } else if v == "0" {
        Some(0.0)
    } else {
        None
    }
}

pub fn parse_color(v: &str) -> Option<Color> {
    if let Some(hex) = v.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::rgb(r, g, b));
        } else if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            return Some(Color::rgb(r, g, b));
        }
    }
    if let Some(inner) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let p: Vec<&str> = inner.split(',').collect();
        if p.len() == 3 {
            return Some(Color::rgb(
                p[0].trim().parse().ok()?,
                p[1].trim().parse().ok()?,
                p[2].trim().parse().ok()?,
            ));
        }
    }
    if let Some(inner) = v.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        let p: Vec<&str> = inner.split(',').collect();
        if p.len() == 4 {
            let a_f: f32 = p[3].trim().parse().ok()?;
            return Some(Color::rgba(
                p[0].trim().parse().ok()?,
                p[1].trim().parse().ok()?,
                p[2].trim().parse().ok()?,
                (a_f.clamp(0.0, 1.0) * 255.0).round() as u8,
            ));
        }
    }
    match v {
        "black" => Some(Color::rgb(0, 0, 0)),
        "white" => Some(Color::rgb(255, 255, 255)),
        "red" => Some(Color::rgb(220, 38, 38)),
        "green" => Some(Color::rgb(22, 163, 74)),
        "blue" => Some(Color::rgb(37, 99, 235)),
        "gray" | "grey" => Some(Color::rgb(107, 114, 128)),
        "orange" => Some(Color::rgb(234, 88, 12)),
        "yellow" => Some(Color::rgb(234, 179, 8)),
        "purple" => Some(Color::rgb(147, 51, 234)),
        _ => None,
    }
}
