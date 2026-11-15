use browser_gfx::{CanvasView, Color, Rect, measure_text};

const BG_LIGHT: Color = Color::rgb(255, 255, 255);
const CARD_BG: Color = Color::rgb(245, 247, 250);
const CARD_HOVER: Color = Color::rgb(232, 240, 254);
const CARD_BORDER: Color = Color::rgb(218, 220, 224);
const TEXT_PRIMARY: Color = Color::rgb(32, 33, 36);
const TEXT_MUTED: Color = Color::rgb(95, 99, 104);
const ACCENT_BLUE: Color = Color::rgb(26, 115, 232);
const DANGER_RED: Color = Color::rgb(217, 48, 37);

pub fn render_internal_page(
    canvas: &mut CanvasView<'_>,
    tab_id: u64,
    pid: u32,
    url: &str,
    hover_pos: Option<(i32, i32)>,
    links: &mut Vec<(Rect, String)>,
) -> String {
    let normalized = url.trim();
    if normalized.eq_ignore_ascii_case("about:blank") {
        canvas.clear(BG_LIGHT);
        return "about:blank".into();
    }
    if normalized.eq_ignore_ascii_case("chrome://newtab") || normalized.is_empty() {
        return render_newtab(canvas, tab_id, pid, hover_pos, links);
    }
    if normalized.eq_ignore_ascii_case("chrome://version") {
        return render_version(canvas, tab_id, pid, hover_pos, links);
    }
    if normalized.eq_ignore_ascii_case("chrome://architecture") {
        return render_architecture(canvas, hover_pos, links);
    }
    render_external_placeholder(canvas, normalized, hover_pos, links)
}

fn render_newtab(
    canvas: &mut CanvasView<'_>,
    tab_id: u64,
    pid: u32,
    hover_pos: Option<(i32, i32)>,
    links: &mut Vec<(Rect, String)>,
) -> String {
    canvas.clear(BG_LIGHT);
    let w = canvas.width() as i32;

    let hero = "Chrome Scratch";
    let (hw, _) = measure_text(4, hero);
    let hx = ((w - hw) / 2).max(24);
    canvas.draw_text(hx, 68, 4, ACCENT_BLUE, hero);

    let sub = "Clean-Room Multi-Process Browser (Rust + Win32)";
    let (sw, _) = measure_text(2, sub);
    canvas.draw_text(((w - sw) / 2).max(24), 116, 2, TEXT_MUTED, sub);

    let cards = [
        ("chrome://version", "Build, Renderer PID & Shared Memory Info", false),
        ("chrome://architecture", "Chrome 157 Reverse-Engineered Architecture", false),
        ("about:blank", "Blank Clean Document Surface", false),
        ("chrome://crash", "Simulate Renderer Crash (Process Isolation Test)", true),
    ];

    let card_w = (w - 96).clamp(280, 620);
    let card_x = ((w - card_w) / 2).max(24);
    let mut y = 176;

    for (target, desc, is_danger) in cards {
        let rect = Rect::new(card_x, y, card_w, 64);
        let hovered = hover_pos.is_some_and(|(px, py)| rect.contains(px, py));
        let fill = if hovered { CARD_HOVER } else { CARD_BG };
        canvas.fill_rounded_rect(rect, 10, fill);
        canvas.stroke_rounded_rect(rect, 10, if hovered { ACCENT_BLUE } else { CARD_BORDER });

        let title_col = if is_danger { DANGER_RED } else { ACCENT_BLUE };
        canvas.draw_text_clipped(card_x + 18, y + 14, 2, title_col, target, card_w - 36);
        canvas.draw_text_clipped(card_x + 18, y + 38, 1, TEXT_MUTED, desc, card_w - 36);
        links.push((rect, target.to_string()));
        y += 80;
    }

    let footer = format!("Tab #{tab_id} running in isolated renderer PID {pid}");
    canvas.draw_text_clipped(24, canvas.height() as i32 - 28, 1, TEXT_MUTED, &footer, w - 48);

    "New Tab".into()
}

fn render_version(
    canvas: &mut CanvasView<'_>,
    tab_id: u64,
    pid: u32,
    hover_pos: Option<(i32, i32)>,
    links: &mut Vec<(Rect, String)>,
) -> String {
    canvas.clear(BG_LIGHT);
    let w = canvas.width() as i32;
    let h = canvas.height() as i32;

    canvas.draw_text(40, 36, 3, TEXT_PRIMARY, "Chrome Scratch Version");
    draw_back_home_chip(canvas, 40, 82, hover_pos, links);

    let rows = [
        ("Version:", "0.1.0 (Clean-Room Sub-Project 1)"),
        ("Reference Target:", "Google Chrome 157.0.8103.51 (x86_64)"),
        ("Architecture:", "Multi-Process Browser + Per-Tab Renderer"),
        ("IPC Transport:", "Length-Prefixed Pipe + Win32 File Mapping"),
        ("Pixel Format:", "BGRA8 Shared Framebuffer (StretchDIBits)"),
        ("Active Tab ID:", &tab_id.to_string()),
        ("Renderer PID:", &pid.to_string()),
        ("Viewport Size:", &format!("{w}x{h}")),
    ];

    let mut y = 136;
    for (k, v) in rows {
        canvas.draw_text(40, y, 2, TEXT_MUTED, k);
        canvas.draw_text_clipped(340, y, 2, TEXT_PRIMARY, v, (w - 360).max(80));
        y += 32;
    }
    "About Version".into()
}

fn render_architecture(
    canvas: &mut CanvasView<'_>,
    hover_pos: Option<(i32, i32)>,
    links: &mut Vec<(Rect, String)>,
) -> String {
    canvas.clear(BG_LIGHT);
    let w = canvas.width() as i32;
    canvas.draw_text(40, 36, 3, TEXT_PRIMARY, "Chrome 157 Architecture Map");
    draw_back_home_chip(canvas, 40, 82, hover_pos, links);

    let lines = [
        "Observed Chrome 157.0.8103.51 Layout -> Clean-Room Rust Crates:",
        "1. chrome.exe (4.19 MB bootstrap)    -> browser-shell single-binary dispatch",
        "2. --type=renderer child flags       -> browser-renderer isolated per-tab child",
        "3. Mojo + SharedMemory handles       -> browser-ipc pipe framing + Win32 shm",
        "4. Skia / GDI / DirectWrite surface  -> browser-gfx BGRA8 canvas & rasterizer",
        "5. Sub-Project 2 (Next)              -> browser-net (DNS, TLS, HTTP/1.1)",
        "6. Sub-Project 3 (Planned)           -> browser-html, browser-css, layout",
        "7. Sub-Project 4 (Planned)           -> browser-js bytecode VM & DOM bindings",
    ];
    let mut y = 136;
    for line in lines {
        canvas.draw_text_clipped(40, y, 2, TEXT_PRIMARY, line, (w - 64).max(120));
        y += 30;
    }
    "Chrome 157 Architecture".into()
}

fn render_external_placeholder(
    canvas: &mut CanvasView<'_>,
    url: &str,
    hover_pos: Option<(i32, i32)>,
    links: &mut Vec<(Rect, String)>,
) -> String {
    canvas.clear(BG_LIGHT);
    let w = canvas.width() as i32;
    canvas.draw_text(40, 40, 3, TEXT_PRIMARY, "Network Stack Pending (Sub-Project 2)");
    draw_back_home_chip(canvas, 40, 86, hover_pos, links);

    canvas.draw_text(40, 144, 2, TEXT_MUTED, "Requested URL:");
    canvas.draw_text_clipped(40, 174, 2, ACCENT_BLUE, url, (w - 80).max(100));
    canvas.draw_text_clipped(
        40,
        224,
        2,
        TEXT_PRIMARY,
        "Sub-Project 1 provides the multi-process shell, IPC, and renderer.",
        (w - 80).max(100),
    );
    canvas.draw_text_clipped(
        40,
        254,
        2,
        TEXT_MUTED,
        "Sub-Project 2 will add custom DNS/TLS/HTTP/1.1 fetching here.",
        (w - 80).max(100),
    );
    url.to_string()
}

fn draw_back_home_chip(
    canvas: &mut CanvasView<'_>,
    x: i32,
    y: i32,
    hover_pos: Option<(i32, i32)>,
    links: &mut Vec<(Rect, String)>,
) {
    let rect = Rect::new(x, y, 200, 30);
    let hovered = hover_pos.is_some_and(|(px, py)| rect.contains(px, py));
    canvas.fill_rounded_rect(rect, 6, if hovered { CARD_HOVER } else { CARD_BG });
    canvas.stroke_rounded_rect(rect, 6, if hovered { ACCENT_BLUE } else { CARD_BORDER });
    canvas.draw_text(x + 12, y + 7, 2, ACCENT_BLUE, "< chrome://newtab");
    links.push((rect, "chrome://newtab".into()));
}
