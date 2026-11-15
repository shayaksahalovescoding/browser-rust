//! Isolated per-tab Renderer process (`--type=renderer`).

pub mod pages;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io;
use std::rc::Rc;

use browser_dom::{
    Document, HitTarget, LayoutBox, LayoutEngine, NodeData, NodeId, Stylesheet, compute_styles,
    paint_layout_with_assets, parse_html,
};
use browser_gfx::{CanvasView, DecodedImage, Rect, decode_image};
use browser_ipc::{BrowserToRenderer, MouseButton, RendererToBrowser, SharedFrameBuffer};
use browser_js::JsEngine;
use browser_net::{HttpClient, Url};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererConfig {
    pub tab_id: u64,
    pub shm_name: String,
    pub width: u32,
    pub height: u32,
}

/// Parses `--type=renderer` startup flags mirroring Chrome's single-binary dispatch.
pub fn parse_renderer_args(args: &[String]) -> Option<RendererConfig> {
    if !args.iter().any(|a| a == "--type=renderer") {
        return None;
    }
    let mut tab_id = None;
    let mut shm_name = None;
    let mut width = 1024u32;
    let mut height = 640u32;

    for arg in args {
        if let Some(v) = arg.strip_prefix("--tab-id=") {
            tab_id = v.parse().ok();
        } else if let Some(v) = arg.strip_prefix("--shm-name=") {
            shm_name = Some(v.to_string());
        } else if let Some(v) = arg.strip_prefix("--width=") {
            if let Ok(w) = v.parse() {
                width = w;
            }
        } else if let Some(v) = arg.strip_prefix("--height=") {
            if let Ok(h) = v.parse() {
                height = h;
            }
        }
    }

    Some(RendererConfig {
        tab_id: tab_id?,
        shm_name: shm_name?,
        width,
        height,
    })
}

pub struct LoadedDocument {
    pub title: String,
    pub base_url: Option<Url>,
    pub doc: Rc<RefCell<Document>>,
    pub external_sheets: Vec<Stylesheet>,
    pub js: JsEngine,
    pub layout: LayoutBox,
    pub hits: Vec<HitTarget>,
    pub images: HashMap<String, DecodedImage>,
    pub input_values: HashMap<NodeId, String>,
    pub focused_input: Option<NodeId>,
    pub content_height: i32,
    pub scroll_y: i32,
    pub network_summary: String,
}

impl LoadedDocument {
    pub fn relayout(&mut self, vw: i32, vh: i32) {
        let mut sheets = self.external_sheets.clone();
        let inline_css = self.doc.borrow().collect_style_blocks();
        if !inline_css.trim().is_empty() {
            sheets.push(Stylesheet::parse(&inline_css));
        }
        let doc_ref = self.doc.borrow();
        let styles = compute_styles(&doc_ref, &sheets);
        let (layout, hits, content_h) = LayoutEngine::layout(&doc_ref, &styles, vw, vh);
        self.layout = layout;
        self.hits = hits;
        self.content_height = content_h;
        let max_scroll = (self.content_height - vh).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max_scroll);
    }

    pub fn paint_into_canvas(&self, canvas: &mut CanvasView<'_>) {
        let focused = self
            .focused_input
            .map(|id| (id, self.input_values.get(&id).map(String::as_str).unwrap_or("")));
        paint_layout_with_assets(&self.layout, canvas, self.scroll_y, &self.images, focused);
    }

    fn paint_into(&self, shm: &mut SharedFrameBuffer) {
        let w = shm.width();
        let h = shm.height();
        let mut canvas = CanvasView::new(w, h, shm.pixels_mut());
        self.paint_into_canvas(&mut canvas);
        shm.commit_generation();
    }

    fn devtools_message(&self, nav_id: u64) -> RendererToBrowser {
        let dom_dump = self.doc.borrow().dump_tree(40).join("\n");
        let console_logs = if self.js.console_logs.is_empty() {
            "(no console output)".to_string()
        } else {
            self.js.console_logs.join("\n")
        };
        RendererToBrowser::DevToolsInfo {
            nav_id,
            network_summary: self.network_summary.clone(),
            dom_dump,
            console_logs,
        }
    }
}

pub fn run_renderer_main(cfg: RendererConfig) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    let mut shm = SharedFrameBuffer::open(&cfg.shm_name, cfg.width, cfg.height)?;
    let mut http = HttpClient::new();
    let mut current_url = String::from("chrome://newtab");
    let mut current_nav_id = 0u64;
    let mut hover_pos: Option<(i32, i32)> = None;
    let mut builtin_links: Vec<(Rect, String)> = Vec::new();
    let mut active_doc: Option<LoadedDocument> = None;

    RendererToBrowser::Ready {
        pid: std::process::id(),
    }
    .write_to(&mut writer)?;

    while let Some(msg) = BrowserToRenderer::read_from(&mut reader)? {
        match msg {
            BrowserToRenderer::Navigate { nav_id, url } => {
                if url.eq_ignore_ascii_case("chrome://crash") {
                    std::process::abort();
                }
                current_url = url.clone();
                current_nav_id = nav_id;

                if pages::is_builtin_canvas_route(&url) {
                    active_doc = None;
                    let title = render_builtin_and_commit(
                        &mut shm,
                        cfg.tab_id,
                        &url,
                        hover_pos,
                        &mut builtin_links,
                    );
                    commit_frame(&mut writer, nav_id, &shm, &title)?;
                    RendererToBrowser::DevToolsInfo {
                        nav_id,
                        network_summary: format!("Internal route: {url}"),
                        dom_dump: "(built-in chrome canvas surface)".into(),
                        console_logs: "(none)".into(),
                    }
                    .write_to(&mut writer)?;
                } else {
                    match load_document(&mut http, &url, shm.width() as i32, shm.height() as i32) {
                        Ok(loaded) => {
                            loaded.paint_into(&mut shm);
                            let title = loaded.title.clone();
                            let dt = loaded.devtools_message(nav_id);
                            active_doc = Some(loaded);
                            commit_frame(&mut writer, nav_id, &shm, &title)?;
                            dt.write_to(&mut writer)?;
                        }
                        Err(err) => {
                            active_doc = None;
                            let mut canvas =
                                CanvasView::new(shm.width(), shm.height(), shm.pixels_mut());
                            let title = pages::render_network_error(&mut canvas, &url, &err);
                            shm.commit_generation();
                            commit_frame(&mut writer, nav_id, &shm, &title)?;
                            RendererToBrowser::DevToolsInfo {
                                nav_id,
                                network_summary: format!("Error loading {url}: {err}"),
                                dom_dump: "(error page)".into(),
                                console_logs: err,
                            }
                            .write_to(&mut writer)?;
                        }
                    }
                }
            }
            BrowserToRenderer::Resize {
                width,
                height,
                shm_name,
            } => {
                shm = SharedFrameBuffer::open(&shm_name, width, height)?;
                if let Some(doc) = active_doc.as_mut() {
                    doc.relayout(width as i32, height as i32);
                    doc.paint_into(&mut shm);
                    commit_frame(&mut writer, current_nav_id, &shm, &doc.title)?;
                } else {
                    let title = render_builtin_and_commit(
                        &mut shm,
                        cfg.tab_id,
                        &current_url,
                        hover_pos,
                        &mut builtin_links,
                    );
                    commit_frame(&mut writer, current_nav_id, &shm, &title)?;
                }
            }
            BrowserToRenderer::Scroll { delta_y } => {
                if let Some(doc) = active_doc.as_mut() {
                    let vh = shm.height() as i32;
                    let max_scroll = (doc.content_height - vh).max(0);
                    let next = (doc.scroll_y - delta_y).clamp(0, max_scroll);
                    if next != doc.scroll_y {
                        doc.scroll_y = next;
                        doc.paint_into(&mut shm);
                        commit_frame(&mut writer, current_nav_id, &shm, &doc.title)?;
                    }
                }
            }
            BrowserToRenderer::MouseMove { x, y } => {
                if active_doc.is_none() {
                    let prev_hover = hover_pos.and_then(|(px, py)| find_hit(&builtin_links, px, py));
                    let next_hover = find_hit(&builtin_links, x, y);
                    hover_pos = Some((x, y));
                    if prev_hover != next_hover {
                        let title = render_builtin_and_commit(
                            &mut shm,
                            cfg.tab_id,
                            &current_url,
                            hover_pos,
                            &mut builtin_links,
                        );
                        commit_frame(&mut writer, current_nav_id, &shm, &title)?;
                    }
                }
            }
            BrowserToRenderer::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                if let Some(doc) = active_doc.as_mut() {
                    let doc_y = y + doc.scroll_y;
                    let hit = doc
                        .hits
                        .iter()
                        .rev()
                        .find(|h| h.rect.contains(x, doc_y))
                        .cloned();
                    if let Some(hit) = hit {
                        let prev_focus = doc.focused_input;
                        let is_text_input = {
                            let d = doc.doc.borrow();
                            d.tag_name(hit.node_id) == Some("input")
                                && !matches!(
                                    d.get_attribute(hit.node_id, "type")
                                        .map(|s| s.to_ascii_lowercase())
                                        .as_deref(),
                                    Some("submit" | "button" | "reset" | "hidden")
                                )
                        };
                        if is_text_input {
                            doc.focused_input = Some(hit.node_id);
                        } else {
                            doc.focused_input = None;
                        }

                        doc.js.dom_dirty = false;
                        if let Err(err) = doc.js.dispatch_click(hit.node_id) {
                            doc.js.console_logs.push(format!("JS error: {err}"));
                        }
                        if doc.js.dom_dirty {
                            doc.relayout(shm.width() as i32, shm.height() as i32);
                            doc.paint_into(&mut shm);
                            commit_frame(&mut writer, current_nav_id, &shm, &doc.title)?;
                            doc.devtools_message(current_nav_id).write_to(&mut writer)?;
                        } else if doc.focused_input != prev_focus {
                            doc.paint_into(&mut shm);
                            commit_frame(&mut writer, current_nav_id, &shm, &doc.title)?;
                        } else if !doc.js.console_logs.is_empty() {
                            doc.devtools_message(current_nav_id).write_to(&mut writer)?;
                        }

                        if let Some(href) = hit.href {
                            let resolved = if let Some(base) = &doc.base_url {
                                base.resolve(&href).map(|u| u.as_str()).unwrap_or(href)
                            } else {
                                href
                            };
                            RendererToBrowser::RequestNavigate { url: resolved }
                                .write_to(&mut writer)?;
                        } else {
                            let is_submit = {
                                let d = doc.doc.borrow();
                                (d.tag_name(hit.node_id) == Some("input")
                                    && matches!(
                                        d.get_attribute(hit.node_id, "type")
                                            .map(|s| s.to_ascii_lowercase())
                                            .as_deref(),
                                        Some("submit")
                                    ))
                                    || (d.tag_name(hit.node_id) == Some("button")
                                        && d.get_attribute(hit.node_id, "type")
                                            .map(|s| s.to_ascii_lowercase())
                                            != Some("button".to_string()))
                            };
                            if is_submit {
                                if let Some(submit_url) = build_form_submit_url(
                                    &doc.doc.borrow(),
                                    doc.base_url.as_ref(),
                                    hit.node_id,
                                    Some(hit.node_id),
                                    &doc.input_values,
                                ) {
                                    RendererToBrowser::RequestNavigate { url: submit_url }
                                        .write_to(&mut writer)?;
                                }
                            }
                        }
                    } else if doc.focused_input.take().is_some() {
                        doc.paint_into(&mut shm);
                        commit_frame(&mut writer, current_nav_id, &shm, &doc.title)?;
                    }
                } else if let Some(target_url) = find_hit(&builtin_links, x, y) {
                    RendererToBrowser::RequestNavigate {
                        url: target_url.to_string(),
                    }
                    .write_to(&mut writer)?;
                }
            }
            BrowserToRenderer::KeyPress { ch } => {
                if let Some(doc) = active_doc.as_mut() {
                    if let Some(input_id) = doc.focused_input {
                        if ch == '\r' || ch == '\n' {
                            if let Some(submit_url) = build_form_submit_url(
                                &doc.doc.borrow(),
                                doc.base_url.as_ref(),
                                input_id,
                                None,
                                &doc.input_values,
                            ) {
                                RendererToBrowser::RequestNavigate { url: submit_url }
                                    .write_to(&mut writer)?;
                            }
                        } else {
                            let val = doc.input_values.entry(input_id).or_default();
                            if ch == '\x08' {
                                val.pop();
                            } else if !ch.is_control() {
                                val.push(ch);
                            }
                            let updated = val.clone();
                            doc.doc.borrow_mut().set_attribute(input_id, "value", &updated);
                            doc.paint_into(&mut shm);
                            commit_frame(&mut writer, current_nav_id, &shm, &doc.title)?;
                        }
                    }
                }
            }
            BrowserToRenderer::MouseDown { .. } => {}
            BrowserToRenderer::Shutdown => break,
        }
    }

    Ok(())
}

pub fn load_document(
    http: &mut HttpClient,
    url: &str,
    vw: i32,
    vh: i32,
) -> Result<LoadedDocument, String> {
    let (html_src, base_url, net_summary) = if url.eq_ignore_ascii_case("chrome://demo") {
        (
            pages::DEMO_HTML.to_string(),
            None,
            "chrome://demo (built-in HTML5/CSS3/JS engine showcase)".to_string(),
        )
    } else {
        let resp = http.get(url).map_err(|e| e.to_string())?;
        let base = Url::parse(&resp.final_url).ok();
        let summary = format!(
            "HTTP {} {} | {} bytes | cache={} | {}",
            resp.status,
            resp.reason,
            resp.body.len(),
            resp.from_cache,
            resp.final_url
        );
        (resp.body_text(), base, summary)
    };

    let mut parsed = parse_html(&html_src);
    let title = parsed.title().unwrap_or_else(|| url.to_string());
    let link_hrefs = parsed.collect_stylesheet_links();
    let scripts = parsed.collect_scripts();

    let mut external_sheets = Vec::new();
    if let Some(base) = &base_url {
        for href in link_hrefs.into_iter().take(6) {
            if let Ok(css_url) = base.resolve(&href) {
                if let Ok(resp) = http.get(&css_url.as_str()) {
                    if resp.status < 400 {
                        external_sheets.push(Stylesheet::parse(&resp.body_text()));
                    }
                }
            }
        }
    }

    // Fetch & decode visible <img src="..."> assets
    let mut images = HashMap::new();
    let mut unsized_imgs = Vec::new();
    if let Some(base) = &base_url {
        let img_nodes = collect_visible_images(&parsed);
        let mut seen = HashSet::new();
        for (nid, src) in img_nodes {
            if !seen.insert(src.clone()) || seen.len() > 24 {
                continue;
            }
            let fetched = if let Ok(img_url) = base.resolve(&src) {
                http.get(&img_url.as_str())
                    .ok()
                    .filter(|r| r.status < 400)
                    .and_then(|r| decode_image(&r.body))
            } else {
                None
            };
            if let Some(decoded) = fetched {
                if parsed.get_attribute(nid, "width").is_none()
                    && parsed.get_attribute(nid, "height").is_none()
                {
                    unsized_imgs.push((nid, decoded.width, decoded.height));
                }
                images.insert(src, decoded);
            }
        }
    }
    for (nid, iw, ih) in unsized_imgs {
        let scale = (640.0 / (iw.max(1) as f32)).min(1.0);
        let w = ((iw as f32) * scale).round().max(1.0) as u32;
        let h = ((ih as f32) * scale).round().max(1.0) as u32;
        parsed.set_attribute(nid, "width", &w.to_string());
        parsed.set_attribute(nid, "height", &h.to_string());
    }

    // Seed initial input field values
    let mut input_values = HashMap::new();
    for node in &parsed.nodes {
        if let NodeData::Element { tag, attrs } = &node.data {
            if tag == "input" {
                let ty = attrs
                    .get("type")
                    .map(|s| s.to_ascii_lowercase())
                    .unwrap_or_else(|| "text".to_string());
                if !matches!(ty.as_str(), "submit" | "button" | "reset" | "hidden") {
                    input_values.insert(
                        node.id,
                        attrs.get("value").cloned().unwrap_or_default(),
                    );
                }
            }
        }
    }

    let doc = Rc::new(RefCell::new(parsed));
    let mut js = JsEngine::new(Rc::clone(&doc));
    for (src, inline) in scripts {
        let code = if let (Some(src_url), Some(base)) = (src, &base_url) {
            base.resolve(&src_url)
                .ok()
                .and_then(|u| http.get(&u.as_str()).ok())
                .filter(|r| r.status < 400)
                .map(|r| r.body_text())
                .unwrap_or_default()
        } else {
            inline
        };
        if !code.trim().is_empty() {
            if let Err(err) = js.eval(&code) {
                js.console_logs.push(format!("JS notice: {err}"));
            }
        }
    }

    let placeholder_box = LayoutBox {
        node_id: 0,
        rect: Rect::new(0, 0, vw, vh),
        style: Default::default(),
        text_runs: Vec::new(),
        children: Vec::new(),
        img_src: None,
        img_label: None,
        control: None,
    };

    let mut loaded = LoadedDocument {
        title,
        base_url,
        doc,
        external_sheets,
        js,
        layout: placeholder_box,
        hits: Vec::new(),
        images,
        input_values,
        focused_input: None,
        content_height: vh,
        scroll_y: 0,
        network_summary: net_summary,
    };
    loaded.relayout(vw, vh);
    Ok(loaded)
}

fn collect_visible_images(doc: &Document) -> Vec<(NodeId, String)> {
    fn walk(doc: &Document, id: NodeId, out: &mut Vec<(NodeId, String)>) {
        if let NodeData::Element { tag, attrs } = &doc.nodes[id].data {
            if matches!(tag.as_str(), "head" | "script" | "style" | "noscript" | "svg" | "template") {
                return;
            }
            if tag == "img" {
                if let Some(src) = attrs.get("src").filter(|s| !s.is_empty()) {
                    out.push((id, src.clone()));
                }
            }
        }
        for &ch in &doc.nodes[id].children {
            walk(doc, ch, out);
        }
    }
    let mut out = Vec::new();
    walk(doc, 0, &mut out);
    out
}

fn build_form_submit_url(
    doc: &Document,
    base_url: Option<&Url>,
    origin_node: NodeId,
    clicked_submit: Option<NodeId>,
    input_values: &HashMap<NodeId, String>,
) -> Option<String> {
    let mut cur = Some(origin_node);
    let mut form_id = None;
    while let Some(id) = cur {
        if doc.tag_name(id) == Some("form") {
            form_id = Some(id);
            break;
        }
        cur = doc.nodes[id].parent;
    }
    let form_id = form_id?;
    let action = doc.get_attribute(form_id, "action").unwrap_or("");
    let base_target = if let Some(base) = base_url {
        base.resolve(action).ok()?.as_str()
    } else if action.is_empty() {
        return None;
    } else {
        action.to_string()
    };

    fn collect_form_inputs(doc: &Document, id: NodeId, out: &mut Vec<NodeId>) {
        if doc.tag_name(id) == Some("input") {
            out.push(id);
        }
        for &ch in &doc.nodes[id].children {
            collect_form_inputs(doc, ch, out);
        }
    }
    let mut inputs = Vec::new();
    collect_form_inputs(doc, form_id, &mut inputs);

    let mut params = Vec::new();
    for iid in inputs {
        let Some(name) = doc.get_attribute(iid, "name").filter(|s| !s.is_empty()) else {
            continue;
        };
        let ty = doc
            .get_attribute(iid, "type")
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_else(|| "text".to_string());
        if matches!(ty.as_str(), "submit" | "button" | "reset") && clicked_submit != Some(iid) {
            continue;
        }
        let val = input_values
            .get(&iid)
            .map(String::as_str)
            .or_else(|| doc.get_attribute(iid, "value"))
            .unwrap_or("");
        params.push(format!("{}={}", percent_encode(name), percent_encode(val)));
    }
    if params.is_empty() {
        Some(base_target)
    } else if base_target.contains('?') {
        Some(format!("{base_target}&{}", params.join("&")))
    } else {
        Some(format!("{base_target}?{}", params.join("&")))
    }
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                let hi = (b >> 4) & 0xF;
                let lo = b & 0xF;
                out.push(char::from_digit(hi as u32, 16).unwrap().to_ascii_uppercase());
                out.push(char::from_digit(lo as u32, 16).unwrap().to_ascii_uppercase());
            }
        }
    }
    out
}

fn commit_frame<W: io::Write>(
    writer: &mut W,
    nav_id: u64,
    shm: &SharedFrameBuffer,
    title: &str,
) -> io::Result<()> {
    RendererToBrowser::FrameCommitted {
        nav_id,
        shm_name: shm.name().to_string(),
        width: shm.width(),
        height: shm.height(),
        title: title.to_string(),
    }
    .write_to(writer)
}

fn find_hit(links: &[(Rect, String)], x: i32, y: i32) -> Option<&str> {
    links
        .iter()
        .find(|(rect, _)| rect.contains(x, y))
        .map(|(_, url)| url.as_str())
}

fn render_builtin_and_commit(
    shm: &mut SharedFrameBuffer,
    tab_id: u64,
    url: &str,
    hover_pos: Option<(i32, i32)>,
    links: &mut Vec<(Rect, String)>,
) -> String {
    let w = shm.width();
    let h = shm.height();
    let mut canvas = CanvasView::new(w, h, shm.pixels_mut());
    links.clear();
    let title = pages::render_internal_page(
        &mut canvas,
        tab_id,
        std::process::id(),
        url,
        hover_pos,
        links,
    );
    shm.commit_generation();
    title
}
