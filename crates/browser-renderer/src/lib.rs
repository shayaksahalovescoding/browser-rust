//! Isolated per-tab Renderer process (`--type=renderer`).

pub mod pages;

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use browser_dom::{
    Document, HitTarget, LayoutBox, LayoutEngine, Stylesheet, compute_styles, paint_layout,
    parse_html,
};
use browser_gfx::{CanvasView, Rect};
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

struct LoadedDocument {
    title: String,
    base_url: Option<Url>,
    doc: Rc<RefCell<Document>>,
    external_sheets: Vec<Stylesheet>,
    js: JsEngine,
    layout: LayoutBox,
    hits: Vec<HitTarget>,
    content_height: i32,
    scroll_y: i32,
    network_summary: String,
}

impl LoadedDocument {
    fn relayout(&mut self, vw: i32, vh: i32) {
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

    fn paint_into(&self, shm: &mut SharedFrameBuffer) {
        let w = shm.width();
        let h = shm.height();
        let mut canvas = CanvasView::new(w, h, shm.pixels_mut());
        paint_layout(&self.layout, &mut canvas, self.scroll_y);
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
                    if let Some(hit) = doc.hits.iter().rev().find(|h| h.rect.contains(x, doc_y)).cloned() {
                        doc.js.dom_dirty = false;
                        if let Err(err) = doc.js.dispatch_click(hit.node_id) {
                            doc.js.console_logs.push(format!("JS error: {err}"));
                        }
                        if doc.js.dom_dirty {
                            doc.relayout(shm.width() as i32, shm.height() as i32);
                            doc.paint_into(&mut shm);
                            commit_frame(&mut writer, current_nav_id, &shm, &doc.title)?;
                            doc.devtools_message(current_nav_id).write_to(&mut writer)?;
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
                        }
                    }
                } else if let Some(target_url) = find_hit(&builtin_links, x, y) {
                    RendererToBrowser::RequestNavigate {
                        url: target_url.to_string(),
                    }
                    .write_to(&mut writer)?;
                }
            }
            BrowserToRenderer::MouseDown { .. } | BrowserToRenderer::KeyPress { .. } => {}
            BrowserToRenderer::Shutdown => break,
        }
    }

    Ok(())
}

fn load_document(
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

    let parsed = parse_html(&html_src);
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
        img_label: None,
    };

    let mut loaded = LoadedDocument {
        title,
        base_url,
        doc,
        external_sheets,
        js,
        layout: placeholder_box,
        hits: Vec::new(),
        content_height: vh,
        scroll_y: 0,
        network_summary: net_summary,
    };
    loaded.relayout(vw, vh);
    Ok(loaded)
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
