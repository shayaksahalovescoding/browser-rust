use browser_gfx::{CanvasView, Color, FontStyle, Rect, measure_text_styled};

const BG_LIGHT: Color = Color::rgb(255, 255, 255);
const BG_SOFT: Color = Color::rgb(248, 249, 250);
const CARD_BG: Color = Color::rgb(255, 255, 255);
const CARD_HOVER: Color = Color::rgb(232, 240, 254);
const CARD_BORDER: Color = Color::rgb(225, 227, 231);
const TEXT_PRIMARY: Color = Color::rgb(32, 33, 36);
const TEXT_MUTED: Color = Color::rgb(95, 99, 104);
const ACCENT_BLUE: Color = Color::rgb(26, 115, 232);
const DANGER_RED: Color = Color::rgb(217, 48, 37);

const HERO_FONT: FontStyle = FontStyle::new(42, true, false);
const H1_FONT: FontStyle = FontStyle::new(24, true, false);
const TITLE_FONT: FontStyle = FontStyle::new(15, true, false);
const BODY_FONT: FontStyle = FontStyle::new(14, false, false);
const SUB_FONT: FontStyle = FontStyle::new(13, false, false);
const SMALL_FONT: FontStyle = FontStyle::new(12, false, false);
const MONO_FONT: FontStyle = FontStyle::new(13, false, true);

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
    let h = canvas.height() as i32;

    // Subtle top ambient gradient band
    canvas.fill_rect(Rect::new(0, 0, w, 190), BG_SOFT);
    canvas.draw_line(0, 190, w, 190, Color::rgb(236, 239, 244));

    // Multicolored Google-style "Chrome" + "Scratch" hero
    let letters: [(&str, Color); 6] = [
        ("C", Color::rgb(66, 133, 244)),
        ("h", Color::rgb(234, 67, 53)),
        ("r", Color::rgb(251, 188, 5)),
        ("o", Color::rgb(66, 133, 244)),
        ("m", Color::rgb(52, 168, 83)),
        ("e", Color::rgb(234, 67, 53)),
    ];
    let suffix = " Scratch";
    let mut total_hero_w = 0i32;
    for (ch, _) in letters {
        total_hero_w += measure_text_styled(HERO_FONT, ch).0;
    }
    total_hero_w += measure_text_styled(HERO_FONT, suffix).0;

    let mut hx = ((w - total_hero_w) / 2).max(24);
    for (ch, col) in letters {
        canvas.draw_text_styled(hx, 44, HERO_FONT, col, ch);
        hx += measure_text_styled(HERO_FONT, ch).0;
    }
    canvas.draw_text_styled(hx, 44, HERO_FONT, TEXT_PRIMARY, suffix);

    let sub = "Clean-Room Multi-Process Browser  •  Pure Rust + Win32 + Custom Rendering & JS Engine";
    let (sw, _) = measure_text_styled(SUB_FONT, sub);
    canvas.draw_text_clipped_styled(
        ((w - sw) / 2).max(24),
        102,
        SUB_FONT,
        TEXT_MUTED,
        sub,
        (w - 48).max(80),
    );

    // Search prompt pill (clicking opens https://www.google.com/)
    let pill_w = (w - 96).clamp(300, 580);
    let pill_x = ((w - pill_w) / 2).max(24);
    let pill_rect = Rect::new(pill_x, 140, pill_w, 42);
    let pill_hover = hover_pos.is_some_and(|(px, py)| pill_rect.contains(px, py));
    canvas.fill_rounded_rect(pill_rect, 21, Color::rgb(255, 255, 255));
    canvas.stroke_rounded_rect(
        pill_rect,
        21,
        if pill_hover {
            ACCENT_BLUE
        } else {
            CARD_BORDER
        },
    );
    canvas.fill_circle(pill_x + 22, 161, 8, Color::rgb(66, 133, 244));
    canvas.draw_text_styled(
        pill_x + 19,
        153,
        SMALL_FONT,
        Color::rgb(255, 255, 255),
        "G",
    );
    canvas.draw_text_clipped_styled(
        pill_x + 42,
        152,
        BODY_FONT,
        TEXT_MUTED,
        "Search Google or type a URL in the omnibox above...",
        pill_w - 60,
    );
    links.push((pill_rect, "https://www.google.com/".to_string()));

    // Responsive 2-column shortcut cards
    let shortcuts: [(&str, &str, &str, &str, Color); 6] = [
        (
            "G",
            "Google Search",
            "https://www.google.com/",
            "Live HTTPS search, logo & form input",
            Color::rgb(66, 133, 244),
        ),
        (
            "D",
            "Engine Showcase",
            "chrome://demo",
            "Interactive HTML5, CSS Flexbox & JS VM",
            Color::rgb(52, 168, 83),
        ),
        (
            "E",
            "Example Domain",
            "https://example.com/",
            "Clean HTTPS fetch via Win32 Schannel TLS",
            Color::rgb(103, 58, 183),
        ),
        (
            "A",
            "Chrome 157 Map",
            "chrome://architecture",
            "Reverse-engineered multi-process layout",
            Color::rgb(251, 140, 0),
        ),
        (
            "V",
            "Build & PID Info",
            "chrome://version",
            "Active renderer PID, TLS & IPC diagnostics",
            Color::rgb(0, 150, 136),
        ),
        (
            "!",
            "Process Isolation",
            "chrome://crash",
            "Abort renderer process without losing shell",
            DANGER_RED,
        ),
    ];

    let cols = if w >= 720 { 2 } else { 1 };
    let gap = 16;
    let grid_w = (w - 64).clamp(300, 760);
    let col_w = if cols == 2 {
        (grid_w - gap) / 2
    } else {
        grid_w
    };
    let start_x = ((w - grid_w) / 2).max(24);
    let start_y = 216;
    let card_h = 74;

    for (idx, (badge, title, target, desc, badge_col)) in shortcuts.iter().enumerate() {
        let col_idx = (idx % cols) as i32;
        let row_idx = (idx / cols) as i32;
        let x = start_x + col_idx * (col_w + gap);
        let y = start_y + row_idx * (card_h + gap);
        let rect = Rect::new(x, y, col_w, card_h);
        let hovered = hover_pos.is_some_and(|(px, py)| rect.contains(px, py));

        // Subtle shadow + crisp rounded card
        canvas.fill_rounded_rect(
            Rect::new(x + 1, y + 2, col_w, card_h),
            12,
            Color::rgb(238, 240, 244),
        );
        canvas.fill_rounded_rect(rect, 12, if hovered { CARD_HOVER } else { CARD_BG });
        canvas.stroke_rounded_rect(
            rect,
            12,
            if hovered { ACCENT_BLUE } else { CARD_BORDER },
        );

        let cx = x + 30;
        let cy = y + card_h / 2;
        canvas.fill_circle(cx, cy, 18, *badge_col);
        let (bw, bh) = measure_text_styled(TITLE_FONT, badge);
        canvas.draw_text_styled(
            cx - bw / 2,
            cy - bh / 2,
            TITLE_FONT,
            Color::rgb(255, 255, 255),
            badge,
        );

        let text_x = x + 60;
        let text_w = (col_w - 74).max(80);
        canvas.draw_text_clipped_styled(
            text_x,
            y + 12,
            TITLE_FONT,
            if *badge_col == DANGER_RED {
                DANGER_RED
            } else {
                TEXT_PRIMARY
            },
            title,
            text_w,
        );
        canvas.draw_text_clipped_styled(text_x, y + 32, SMALL_FONT, ACCENT_BLUE, target, text_w);
        canvas.draw_text_clipped_styled(text_x, y + 49, SMALL_FONT, TEXT_MUTED, desc, text_w);

        links.push((rect, (*target).to_string()));
    }

    let footer = format!(
        "Tab #{tab_id}  •  Isolated Renderer PID {pid}  •  Ctrl+L: Omnibox  •  F12: DevTools"
    );
    let (fw, _) = measure_text_styled(SMALL_FONT, &footer);
    canvas.draw_text_clipped_styled(
        ((w - fw) / 2).max(24),
        (h - 32).max(460),
        SMALL_FONT,
        TEXT_MUTED,
        &footer,
        (w - 48).max(80),
    );

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

    canvas.draw_text_styled(40, 32, H1_FONT, TEXT_PRIMARY, "Chrome Scratch Version");
    draw_back_home_chip(canvas, 40, 74, hover_pos, links);

    let pid_str = pid.to_string();
    let tab_str = tab_id.to_string();
    let vp_str = format!("{w} x {h}");
    let rows = [
        ("Version", "0.3.0 (Clean-Room Full Stack)"),
        ("Reference Target", "Google Chrome 157.0.8103.51 (x86_64)"),
        ("Process Model", "Multi-Process Browser + Per-Tab Renderer"),
        ("Network Stack", "URL, HTTP/1.1, GZIP/DEFLATE, Win32 Schannel TLS, Windows-1252/UTF-8"),
        ("Rendering Engine", "HTML5, CSS3 Cascade, Table/Flexbox/InlineBlock, GDI+ Images & Segoe UI"),
        ("JS Runtime", "Lexer, Pratt Parser, Closure VM & DOM Bindings"),
        ("Active Tab ID", tab_str.as_str()),
        ("Renderer PID", pid_str.as_str()),
        ("Viewport Size", vp_str.as_str()),
    ];

    let card_w = (w - 80).clamp(320, 820);
    let card_h = rows.len() as i32 * 34 + 20;
    canvas.fill_rounded_rect(Rect::new(40, 122, card_w, card_h), 10, BG_SOFT);
    canvas.stroke_rounded_rect(Rect::new(40, 122, card_w, card_h), 10, CARD_BORDER);

    let mut y = 134;
    for (k, v) in rows {
        canvas.draw_text_styled(58, y, SUB_FONT, TEXT_MUTED, k);
        canvas.draw_text_clipped_styled(230, y, BODY_FONT, TEXT_PRIMARY, v, (card_w - 206).max(80));
        y += 34;
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
    canvas.draw_text_styled(40, 32, H1_FONT, TEXT_PRIMARY, "Chrome 157 Architecture Map");
    draw_back_home_chip(canvas, 40, 74, hover_pos, links);

    let lines = [
        "Observed Chrome 157.0.8103.51 Layout  ->  Clean-Room Rust Crates:",
        "1. chrome.exe (4.19 MB bootstrap)     -> browser-shell single-binary dispatch",
        "2. --type=renderer child flags        -> browser-renderer isolated per-tab child",
        "3. Mojo + SharedMemory handles        -> browser-ipc pipe framing + Win32 shm",
        "4. Skia / DirectWrite / SkImage       -> browser-gfx BGRA8 canvas, Segoe UI & GDI+",
        "5. NetworkService utility process     -> browser-net (DNS, TLS, HTTP/1.1, GZIP)",
        "6. Blink HTML/CSS/Layout              -> browser-dom (DOM, Cascade, Table/Flex)",
        "7. V8 JavaScript Engine               -> browser-js (Lexer, Parser, DOM Bindings)",
    ];
    let card_w = (w - 80).clamp(320, 860);
    let card_h = lines.len() as i32 * 32 + 24;
    canvas.fill_rounded_rect(Rect::new(40, 122, card_w, card_h), 10, BG_SOFT);
    canvas.stroke_rounded_rect(Rect::new(40, 122, card_w, card_h), 10, CARD_BORDER);

    let mut y = 136;
    for (i, line) in lines.iter().enumerate() {
        let font = if i == 0 { TITLE_FONT } else { MONO_FONT };
        canvas.draw_text_clipped_styled(58, y, font, TEXT_PRIMARY, line, (card_w - 36).max(120));
        y += 32;
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
    canvas.draw_text_styled(40, 36, H1_FONT, TEXT_PRIMARY, "Search Query");
    draw_back_home_chip(canvas, 40, 78, hover_pos, links);
    canvas.draw_text_clipped_styled(40, 134, TITLE_FONT, ACCENT_BLUE, url, (w - 80).max(100));
    canvas.draw_text_clipped_styled(
        40,
        166,
        BODY_FONT,
        TEXT_MUTED,
        "Enter a full URL (e.g. https://www.google.com) or click chrome://demo.",
        (w - 80).max(100),
    );
    "Search".into()
}

pub fn render_network_error(canvas: &mut CanvasView<'_>, url: &str, err: &str) -> String {
    canvas.clear(BG_LIGHT);
    let w = canvas.width() as i32;
    canvas.draw_text_styled(40, 44, H1_FONT, DANGER_RED, "This site can't be reached");
    canvas.draw_text_clipped_styled(40, 92, TITLE_FONT, TEXT_PRIMARY, url, (w - 80).max(100));
    canvas.draw_text_clipped_styled(40, 124, BODY_FONT, TEXT_MUTED, err, (w - 80).max(100));
    "Page Load Error".into()
}

fn draw_back_home_chip(
    canvas: &mut CanvasView<'_>,
    x: i32,
    y: i32,
    hover_pos: Option<(i32, i32)>,
    links: &mut Vec<(Rect, String)>,
) {
    let rect = Rect::new(x, y, 176, 32);
    let hovered = hover_pos.is_some_and(|(px, py)| rect.contains(px, py));
    canvas.fill_rounded_rect(rect, 16, if hovered { CARD_HOVER } else { BG_SOFT });
    canvas.stroke_rounded_rect(rect, 16, if hovered { ACCENT_BLUE } else { CARD_BORDER });
    canvas.draw_text_styled(x + 14, y + 7, SUB_FONT, ACCENT_BLUE, "<  Back to New Tab");
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
    <p><a href="chrome://newtab">Back to New Tab</a> | <a href="https://www.google.com/">Visit Google</a></p>
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
