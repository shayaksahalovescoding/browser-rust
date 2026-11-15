use std::collections::HashMap;

use crate::dom::{Document, NodeData, NodeId};

const VOID_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
    "source", "track", "wbr",
];

pub fn parse_html(input: &str) -> Document {
    let mut doc = Document::new();
    let mut stack: Vec<NodeId> = vec![doc.root()];
    let bytes = input.as_bytes();
    let mut pos = 0usize;

    while pos < bytes.len() {
        if bytes[pos] == b'<' {
            if input[pos..].starts_with("<!--") {
                if let Some(end) = input[pos + 4..].find("-->") {
                    pos += 4 + end + 3;
                } else {
                    break;
                }
                continue;
            }
            if input[pos..].to_ascii_lowercase().starts_with("<!doctype") {
                if let Some(end) = input[pos..].find('>') {
                    pos += end + 1;
                } else {
                    break;
                }
                continue;
            }
            if input[pos..].starts_with("</") {
                pos += 2;
                let tag = consume_ident(input, &mut pos).to_ascii_lowercase();
                skip_until_gt(input, &mut pos);
                pop_to_tag(&mut stack, &doc, &tag);
                continue;
            }

            // Start tag
            pos += 1;
            let tag = consume_ident(input, &mut pos).to_ascii_lowercase();
            if tag.is_empty() {
                continue;
            }
            let (attrs, self_closing) = parse_attrs(input, &mut pos);

            // Implicit closures matching HTML5 parser behavior
            if tag == "p" {
                close_if_open(&mut stack, &doc, "p");
            } else if tag == "li" {
                close_if_open(&mut stack, &doc, "li");
            }

            let elem = doc.create_element(&tag);
            for (k, v) in attrs {
                doc.set_attribute(elem, &k, &v);
            }
            let parent = *stack.last().unwrap_or(&0);
            doc.append_child(parent, elem);

            if !self_closing && !VOID_TAGS.contains(&tag.as_str()) {
                if tag == "script" || tag == "style" {
                    let close_pat = format!("</{tag}");
                    let rest = &input[pos..];
                    let end_rel = rest
                        .to_ascii_lowercase()
                        .find(&close_pat)
                        .unwrap_or(rest.len());
                    let raw = &rest[..end_rel];
                    if !raw.is_empty() {
                        let txt = doc.create_text(raw);
                        doc.append_child(elem, txt);
                    }
                    pos += end_rel;
                    if pos < bytes.len() {
                        skip_until_gt(input, &mut pos);
                    }
                } else {
                    stack.push(elem);
                }
            }
        } else {
            let next_lt = input[pos..].find('<').map(|i| pos + i).unwrap_or(bytes.len());
            let raw_text = &input[pos..next_lt];
            pos = next_lt;
            let decoded = decode_entities(raw_text);
            if !decoded.is_empty() {
                let parent = *stack.last().unwrap_or(&0);
                let txt = doc.create_text(&decoded);
                doc.append_child(parent, txt);
            }
        }
    }

    doc
}

fn consume_ident(s: &str, pos: &mut usize) -> String {
    let start = *pos;
    let bytes = s.as_bytes();
    while *pos < bytes.len() {
        let b = bytes[*pos];
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b':' {
            *pos += 1;
        } else {
            break;
        }
    }
    s[start..*pos].to_string()
}

fn skip_ws(s: &str, pos: &mut usize) {
    let bytes = s.as_bytes();
    while *pos < bytes.len() && bytes[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
}

fn skip_until_gt(s: &str, pos: &mut usize) {
    let bytes = s.as_bytes();
    while *pos < bytes.len() {
        let b = bytes[*pos];
        *pos += 1;
        if b == b'>' {
            break;
        }
    }
}

fn parse_attrs(s: &str, pos: &mut usize) -> (HashMap<String, String>, bool) {
    let mut attrs = HashMap::new();
    let mut self_closing = false;
    let bytes = s.as_bytes();

    loop {
        skip_ws(s, pos);
        if *pos >= bytes.len() {
            break;
        }
        if bytes[*pos] == b'>' {
            *pos += 1;
            break;
        }
        if bytes[*pos] == b'/' {
            self_closing = true;
            *pos += 1;
            continue;
        }
        let name = consume_ident(s, pos).to_ascii_lowercase();
        if name.is_empty() {
            *pos += 1;
            continue;
        }
        skip_ws(s, pos);
        let val = if *pos < bytes.len() && bytes[*pos] == b'=' {
            *pos += 1;
            skip_ws(s, pos);
            if *pos < bytes.len() && (bytes[*pos] == b'"' || bytes[*pos] == b'\'') {
                let quote = bytes[*pos];
                *pos += 1;
                let start = *pos;
                while *pos < bytes.len() && bytes[*pos] != quote {
                    *pos += 1;
                }
                let raw = &s[start..*pos];
                if *pos < bytes.len() {
                    *pos += 1;
                }
                decode_entities(raw)
            } else {
                let start = *pos;
                while *pos < bytes.len()
                    && !bytes[*pos].is_ascii_whitespace()
                    && bytes[*pos] != b'>'
                {
                    *pos += 1;
                }
                decode_entities(&s[start..*pos])
            }
        } else {
            String::new()
        };
        attrs.insert(name, val);
    }
    (attrs, self_closing)
}

fn close_if_open(stack: &mut Vec<NodeId>, doc: &Document, tag: &str) {
    if let Some(pos) = stack
        .iter()
        .rposition(|&id| matches!(&doc.nodes[id].data, NodeData::Element { tag: t, .. } if t == tag))
    {
        if pos > 0 {
            stack.truncate(pos);
        }
    }
}

fn pop_to_tag(stack: &mut Vec<NodeId>, doc: &Document, tag: &str) {
    if let Some(pos) = stack
        .iter()
        .rposition(|&id| matches!(&doc.nodes[id].data, NodeData::Element { tag: t, .. } if t == tag))
    {
        if pos > 0 {
            stack.truncate(pos);
        }
    }
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(semi_rel) = s[i..].find(';') {
                if semi_rel <= 12 {
                    let ent = &s[i + 1..i + semi_rel];
                    let rep = match ent {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "quot" => Some('"'),
                        "apos" => Some('\''),
                        "nbsp" => Some(' '),
                        "mdash" | "ndash" => Some('-'),
                        "copy" => Some('c'),
                        _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                            u32::from_str_radix(&ent[2..], 16)
                                .ok()
                                .and_then(char::from_u32)
                        }
                        _ if ent.starts_with('#') => {
                            ent[1..].parse::<u32>().ok().and_then(char::from_u32)
                        }
                        _ => None,
                    };
                    if let Some(ch) = rep {
                        out.push(ch);
                        i += semi_rel + 1;
                        continue;
                    }
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}
