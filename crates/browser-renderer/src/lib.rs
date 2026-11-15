//! Isolated per-tab Renderer process (`--type=renderer`).

pub mod pages;

use std::io;

use browser_gfx::Rect;
use browser_ipc::{BrowserToRenderer, MouseButton, RendererToBrowser, SharedFrameBuffer};

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

pub fn run_renderer_main(cfg: RendererConfig) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    let mut shm = SharedFrameBuffer::open(&cfg.shm_name, cfg.width, cfg.height)?;
    let mut current_url = String::from("chrome://newtab");
    let mut current_nav_id = 0u64;
    let mut hover_pos: Option<(i32, i32)> = None;
    let mut links: Vec<(Rect, String)> = Vec::new();

    RendererToBrowser::Ready {
        pid: std::process::id(),
    }
    .write_to(&mut writer)?;

    while let Some(msg) = BrowserToRenderer::read_from(&mut reader)? {
        match msg {
            BrowserToRenderer::Navigate { nav_id, url } => {
                if url.eq_ignore_ascii_case("chrome://crash") {
                    // Deliberately crash only this isolated renderer process.
                    std::process::abort();
                }
                current_url = url;
                current_nav_id = nav_id;
                let title = render_and_commit(
                    &mut shm,
                    cfg.tab_id,
                    &current_url,
                    hover_pos,
                    &mut links,
                );
                RendererToBrowser::FrameCommitted {
                    nav_id,
                    shm_name: shm.name().to_string(),
                    width: shm.width(),
                    height: shm.height(),
                    title,
                }
                .write_to(&mut writer)?;
            }
            BrowserToRenderer::Resize {
                width,
                height,
                shm_name,
            } => {
                shm = SharedFrameBuffer::open(&shm_name, width, height)?;
                let title = render_and_commit(
                    &mut shm,
                    cfg.tab_id,
                    &current_url,
                    hover_pos,
                    &mut links,
                );
                RendererToBrowser::FrameCommitted {
                    nav_id: current_nav_id,
                    shm_name: shm.name().to_string(),
                    width: shm.width(),
                    height: shm.height(),
                    title,
                }
                .write_to(&mut writer)?;
            }
            BrowserToRenderer::MouseMove { x, y } => {
                let prev_hover = hover_pos.and_then(|(px, py)| find_hit(&links, px, py));
                let next_hover = find_hit(&links, x, y);
                hover_pos = Some((x, y));
                if prev_hover != next_hover {
                    let title = render_and_commit(
                        &mut shm,
                        cfg.tab_id,
                        &current_url,
                        hover_pos,
                        &mut links,
                    );
                    RendererToBrowser::FrameCommitted {
                        nav_id: current_nav_id,
                        shm_name: shm.name().to_string(),
                        width: shm.width(),
                        height: shm.height(),
                        title,
                    }
                    .write_to(&mut writer)?;
                }
            }
            BrowserToRenderer::MouseDown {
                x,
                y,
                button: MouseButton::Left,
            } => {
                if let Some(target_url) = find_hit(&links, x, y) {
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

fn find_hit(links: &[(Rect, String)], x: i32, y: i32) -> Option<&str> {
    links
        .iter()
        .find(|(rect, _)| rect.contains(x, y))
        .map(|(_, url)| url.as_str())
}

fn render_and_commit(
    shm: &mut SharedFrameBuffer,
    tab_id: u64,
    url: &str,
    hover_pos: Option<(i32, i32)>,
    links: &mut Vec<(Rect, String)>,
) -> String {
    let w = shm.width();
    let h = shm.height();
    let mut canvas = browser_gfx::CanvasView::new(w, h, shm.pixels_mut());
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
