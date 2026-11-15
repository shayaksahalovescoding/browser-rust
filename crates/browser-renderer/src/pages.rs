use browser_gfx::{CanvasView, Color, Rect, measure_text};

const BG_LIGHT: Color = Color::rgb(255, 255, 255);
const CARD_BG: Color = Color::rgb(245, 247, 250);
const CARD_HOVER: Color = Color::rgb(232, 240, 254);
const CARD_BORDER: Color = Color::rgb(218, 220, 224);
const TEXT_PRIMARY: Color = Color::rgb(32, 33, 36);
const TEXT_MUTED: Color = Color::rgb(95, 99, 104);
const ACCENT_BLUE: Color = Color::rgb(26, 115, 232);
const DANGER_RED: Color = Color::rgb(217, 48, 37);

pub fn is_builtin_canvas_route(url: &str) -> bool {
    let n = url.trim();
    n.is_empty()
        || n.eq_ignore_ascii_case("about:blank")
        || n.eq_ignore_ascii_case("chrome://newtab")
        || n.eq_ignore_ascii_case("chrome://version")
        || n.eq_ignore_ascii_case("chrome://architecture")
        || n.to_ascii_lowercase().starts_with("chrome://search?")
}

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
    render_search_placeholder(canvas, normalized, hover_pos, links)
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
    canvas.draw_text(hx, 44, 4, ACCENT_BLUE, hero);

    let sub = "Clean-Room Multi-Process Browser (Rust + Win32 + Custom Engine)";
    let (sw, _) = measure_text(2, sub);
    canvas.draw_text_clipped(((w - sw) / 2).max(24), 92, 2, TEXT_MUTED, sub, w - 48);

    let cards = [
        ("chrome://demo", "Interactive HTML5 + CSS Flexbox + JS VM Demo", false),
        ("https://example.com/", "Live HTTPS Fetch via Schannel TLS + HTTP/1.1", false),
        ("chrome://version", "Build, Renderer PID & Shared Memory Info", false),
        ("chrome://architecture", "Chrome 157 Reverse-Engineered Architecture", false),
        ("chrome://crash", "Simulate Renderer Crash (Process Isolation Test)", true),
    ];

    let card_w = (w - 96).clamp(280, 660);
    let card_x = ((w - card_w) / 2).max(24);
    let mut y = 140;

    for (target, desc, is_danger) in cards {
        let rect = Rect::new(card_x, y, card_w, 56);
        let hovered = hover_pos.is_some_and(|(px, py)| rect.contains(px, py));
        let fill = if hovered { CARD_HOVER } else { CARD_BG };
        canvas.fill_rounded_rect(rect, 10, fill);
        canvas.stroke_rounded_rect(rect, 10, if hovered { ACCENT_BLUE } else { CARD_BORDER });

        let title_col = if is_danger { DANGER_RED } else { ACCENT_BLUE };
        canvas.draw_text_clipped(card_x + 16, y + 10, 2, title_col, target, card_w - 32);
        canvas.draw_text_clipped(card_x + 16, y + 34, 1, TEXT_MUTED, desc, card_w - 32);
        links.push((rect, target.to_string()));
        y += 68;
    }

    let footer = format!("Tab #{tab_id} running in isolated renderer PID {pid} | F12: DevTools");
    canvas.draw_text_clipped(24, canvas.height() as i32 - 26, 1, TEXT_MUTED, &footer, w - 48);

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
        ("Version:", "0.2.0 (Clean-Room Full Stack)"),
        ("Reference Target:", "Google Chrome 157.0.8103.51 (x86_64)"),
        ("Process Model:", "Multi-Process Browser + Per-Tab Renderer"),
        ("Network Stack:", "Custom URL, HTTP/1.1, GZIP/DEFLATE, Schannel TLS"),
        ("Rendering Engine:", "Custom HTML5, CSS3 Cascade, Flexbox & 2D Painter"),
        ("JS Runtime:", "Custom Lexer, Pratt Parser, Closure VM & DOM"),
        ("Active Tab ID:", &tab_id.to_string()),
        ("Renderer PID:", &pid.to_string()),
        ("Viewport Size:", &format!("{w}x{h}")),
    ];

    let mut y = 136;
    for (k, v) in rows {
        canvas.draw_text(40, y, 2, TEXT_MUTED, k);
        canvas.draw_text_clipped(340, y, 2, TEXT_PRIMARY, v, (w - 360).max(80));
        y += 30;
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
        "5. NetworkService utility process    -> browser-net (DNS, TLS, HTTP/1.1, GZIP)",
        "6. Blink HTML/CSS/Layout             -> browser-dom (DOM, Cascade, Flexbox)",
        "7. V8 JavaScript Engine              -> browser-js (Lexer, Parser, DOM Bindings)",
    ];
    let mut y = 136;
    for line in lines {
        canvas.draw_text_clipped(40, y, 2, TEXT_PRIMARY, line, (w - 64).max(120));
        y += 30;
    }
    "Chrome 157 Architecture".into()
}

fn render_search_placeholder(
    canvas: &mut CanvasView<'_>,
    url: &str,
    hover_pos: Option<(i32, i32)>,
    links: &mut Vec<(Rect, String)>,
) -> String {
    canvas.clear(BG_LIGHT);
    let w = canvas.width() as i32;
    canvas.draw_text(40, 40, 3, TEXT_PRIMARY, "Search Query");
    draw_back_home_chip(canvas, 40, 86, hover_pos, links);
    canvas.draw_text_clipped(40, 144, 2, ACCENT_BLUE, url, (w - 80).max(100));
    canvas.draw_text_clipped(
        40,
        184,
        2,
        TEXT_MUTED,
        "Enter a full URL (e.g. https://example.com) or click chrome://demo.",
        (w - 80).max(100),
    );
    "Search".into()
}

pub fn render_network_error(canvas: &mut CanvasView<'_>, url: &str, err: &str) -> String {
    canvas.clear(BG_LIGHT);
    let w = canvas.width() as i32;
    canvas.draw_text(40, 44, 3, DANGER_RED, "This site can't be reached");
    canvas.draw_text_clipped(40, 96, 2, TEXT_PRIMARY, url, (w - 80).max(100));
    canvas.draw_text_clipped(40, 134, 2, TEXT_MUTED, err, (w - 80).max(100));
    "Page Load Error".into()
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

pub const DEMO_HTML: &str = r##"<!doctype html>
<html>
<head>
  <title>Chrome Scratch Engine Demo</title>
  <style>
    body { margin: 24px; background-color: #f8f9fa; color: #202124; }
    .container { max-width: 760px; margin: 0 auto; background-color: #ffffff; padding: 24px; border: 1px solid #dadce0; border-radius: 12px; }
    .row { display: flex; gap: 16px; margin-top: 16px; }
    .box { background-color: #e8f0fe; padding: 14px; border-radius: 8px; border: 1px solid #1a73e8; }
    #btn { background-color: #1a73e8; color: #ffffff; padding: 10px 18px; border-radius: 6px; border: 0px solid #1a73e8; }
  </style>
</head>
<body>
  <div class="container">
    <h1>Custom HTML5 + CSS3 + JS Engine</h1>
    <p>Rendered from scratch using our custom HTML tokenizer, CSS cascade, Flexbox layout, 2D software painter, and JS VM.</p>
    <div class="row">
      <div class="box"><strong>Flex Item 1</strong><br/>Box model &amp; cascade</div>
      <div class="box"><strong>Flex Item 2</strong><br/>Live JS DOM mutation</div>
    </div>
    <p id="status">Button clicks: 0</p>
    <button id="btn">Click Me (JS Event)</button>
    <p><a href="chrome://newtab">Back to New Tab</a> | <a href="https://example.com/">Visit example.com</a></p>
  </div>
  <script>
    console.log("chrome://demo script initialized");
    let count = 0;
    const status = document.getElementById("status");
    const btn = document.getElementById("btn");
    btn.addEventListener("click", () => {
      count++;
      status.textContent = "Button clicks: " + count + " (DOM updated live!)";
      status.style.color = "#1a73e8";
      console.log("clicked count =", count);
    });
  </script>
</body>
</html>"##;
