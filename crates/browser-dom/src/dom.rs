use std::collections::HashMap;

pub type NodeId = usize;

#[derive(Debug, Clone)]
pub enum NodeData {
    Document,
    Element {
        tag: String,
        attrs: HashMap<String, String>,
    },
    Text(String),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub data: NodeData,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub nodes: Vec<Node>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    pub fn new() -> Self {
        Self {
            nodes: vec![Node {
                id: 0,
                parent: None,
                children: Vec::new(),
                data: NodeData::Document,
            }],
        }
    }

    pub fn root(&self) -> NodeId {
        0
    }

    pub fn create_element(&mut self, tag: &str) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            id,
            parent: None,
            children: Vec::new(),
            data: NodeData::Element {
                tag: tag.to_ascii_lowercase(),
                attrs: HashMap::new(),
            },
        });
        id
    }

    pub fn create_text(&mut self, text: &str) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            id,
            parent: None,
            children: Vec::new(),
            data: NodeData::Text(text.to_string()),
        });
        id
    }

    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        if let Some(old_parent) = self.nodes[child].parent {
            self.nodes[old_parent].children.retain(|&c| c != child);
        }
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.push(child);
    }

    pub fn set_attribute(&mut self, id: NodeId, key: &str, val: &str) {
        if let Some(Node {
            data: NodeData::Element { attrs, .. },
            ..
        }) = self.nodes.get_mut(id)
        {
            attrs.insert(key.to_ascii_lowercase(), val.to_string());
        }
    }

    pub fn get_attribute(&self, id: NodeId, key: &str) -> Option<&str> {
        if let Some(Node {
            data: NodeData::Element { attrs, .. },
            ..
        }) = self.nodes.get(id)
        {
            attrs.get(&key.to_ascii_lowercase()).map(|s| s.as_str())
        } else {
            None
        }
    }

    pub fn tag_name(&self, id: NodeId) -> Option<&str> {
        if let Some(Node {
            data: NodeData::Element { tag, .. },
            ..
        }) = self.nodes.get(id)
        {
            Some(tag.as_str())
        } else {
            None
        }
    }

    pub fn set_text_content(&mut self, id: NodeId, text: &str) {
        if let Some(node) = self.nodes.get_mut(id) {
            match &mut node.data {
                NodeData::Text(s) => *s = text.to_string(),
                NodeData::Element { .. } | NodeData::Document => {
                    node.children.clear();
                    let txt = self.create_text(text);
                    self.append_child(id, txt);
                }
            }
        }
    }

    pub fn text_content(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.collect_text(id, &mut out);
        out
    }

    fn collect_text(&self, id: NodeId, out: &mut String) {
        match &self.nodes[id].data {
            NodeData::Text(t) => out.push_str(t),
            NodeData::Element { .. } | NodeData::Document => {
                for &ch in &self.nodes[id].children {
                    self.collect_text(ch, out);
                }
            }
        }
    }

    pub fn find_first_by_tag(&self, tag: &str) -> Option<NodeId> {
        let target = tag.to_ascii_lowercase();
        self.nodes.iter().find_map(|n| match &n.data {
            NodeData::Element { tag, .. } if *tag == target => Some(n.id),
            _ => None,
        })
    }

    pub fn title(&self) -> Option<String> {
        self.find_first_by_tag("title")
            .map(|id| self.text_content(id).trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn has_ancestor_any(&self, id: NodeId, tags: &[&str]) -> bool {
        let mut cur = self.nodes.get(id).and_then(|n| n.parent);
        while let Some(pid) = cur {
            if let Some(tag) = self.tag_name(pid) {
                if tags.iter().any(|t| *t == tag) {
                    return true;
                }
            }
            cur = self.nodes.get(pid).and_then(|n| n.parent);
        }
        false
    }

    pub fn collect_style_blocks(&self) -> String {
        let mut out = String::new();
        for n in &self.nodes {
            if let NodeData::Element { tag, .. } = &n.data {
                if tag == "style" && !self.has_ancestor_any(n.id, &["noscript", "template"]) {
                    out.push_str(&self.text_content(n.id));
                    out.push('\n');
                }
            }
        }
        out
    }

    pub fn collect_stylesheet_links(&self) -> Vec<String> {
        let mut links = Vec::new();
        for n in &self.nodes {
            if let NodeData::Element { tag, attrs } = &n.data {
                if tag == "link" && !self.has_ancestor_any(n.id, &["noscript", "template"]) {
                    let is_css = attrs
                        .get("rel")
                        .is_some_and(|r| r.to_ascii_lowercase().contains("stylesheet"));
                    if is_css {
                        if let Some(href) = attrs.get("href") {
                            links.push(href.clone());
                        }
                    }
                }
            }
        }
        links
    }

    pub fn collect_scripts(&self) -> Vec<(Option<String>, String)> {
        let mut scripts = Vec::new();
        for n in &self.nodes {
            if let NodeData::Element { tag, attrs } = &n.data {
                if tag == "script" && !self.has_ancestor_any(n.id, &["noscript", "template"]) {
                    let src = attrs.get("src").cloned();
                    let inline = self.text_content(n.id);
                    scripts.push((src, inline));
                }
            }
        }
        scripts
    }

    pub fn get_element_by_id(&self, target_id: &str) -> Option<NodeId> {
        self.nodes.iter().find_map(|n| match &n.data {
            NodeData::Element { attrs, .. }
                if attrs.get("id").is_some_and(|v| v == target_id) =>
            {
                Some(n.id)
            }
            _ => None,
        })
    }

    pub fn query_selector(&self, selector: &str) -> Option<NodeId> {
        let sel = selector.trim();
        self.nodes
            .iter()
            .find(|n| matches_simple_selector(n, sel))
            .map(|n| n.id)
    }

    pub fn query_selector_all(&self, selector: &str) -> Vec<NodeId> {
        let sel = selector.trim();
        self.nodes
            .iter()
            .filter(|n| matches_simple_selector(n, sel))
            .map(|n| n.id)
            .collect()
    }

    pub fn dump_tree(&self, max_lines: usize) -> Vec<String> {
        let mut lines = Vec::new();
        self.dump_node(0, 0, max_lines, &mut lines);
        lines
    }

    fn dump_node(&self, id: NodeId, depth: usize, max_lines: usize, out: &mut Vec<String>) {
        if out.len() >= max_lines {
            return;
        }
        let indent = "  ".repeat(depth.min(8));
        match &self.nodes[id].data {
            NodeData::Document => {}
            NodeData::Element { tag, attrs } => {
                let mut desc = format!("{indent}<{tag}");
                if let Some(id_attr) = attrs.get("id") {
                    desc.push_str(&format!(" id=\"{id_attr}\""));
                }
                if let Some(cls) = attrs.get("class") {
                    desc.push_str(&format!(" class=\"{cls}\""));
                }
                desc.push('>');
                out.push(desc);
            }
            NodeData::Text(txt) => {
                let trimmed = txt.trim();
                if !trimmed.is_empty() {
                    let preview: String = trimmed.chars().take(60).collect();
                    out.push(format!("{indent}\"{preview}\""));
                }
            }
        }
        for &ch in &self.nodes[id].children {
            self.dump_node(ch, if id == 0 { 0 } else { depth + 1 }, max_lines, out);
        }
    }
}

fn matches_simple_selector(node: &Node, sel: &str) -> bool {
    let NodeData::Element { tag, attrs } = &node.data else {
        return false;
    };
    if sel == "*" {
        return true;
    }
    if let Some(id_sel) = sel.strip_prefix('#') {
        return attrs.get("id").is_some_and(|v| v == id_sel);
    }
    if let Some(cls_sel) = sel.strip_prefix('.') {
        return attrs
            .get("class")
            .is_some_and(|v| v.split_whitespace().any(|c| c == cls_sel));
    }
    tag.eq_ignore_ascii_case(sel)
}
