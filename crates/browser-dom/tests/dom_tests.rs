use browser_dom::{
    LayoutEngine, Stylesheet, compute_styles, paint_layout, parse_html,
};
use browser_gfx::{Color, OwnedCanvas};

#[test]
fn html_parser_css_cascade_and_layout_paint() {
    let html = r#"
        <!doctype html>
        <html>
          <head>
            <title>Test Page</title>
            <style>
              .card { background-color: #eef4ff; padding: 12px; border: 1px solid #1a73e8; }
              #hero { color: #1a73e8; }
            </style>
          </head>
          <body>
            <div class="card">
              <h1 id="hero">Hello &amp; Welcome</h1>
              <p>Visit <a href="https://example.com">Example Link</a></p>
            </div>
          </body>
        </html>
    "#;

    let doc = parse_html(html);
    assert_eq!(doc.title().as_deref(), Some("Test Page"));
    assert!(doc.get_element_by_id("hero").is_some());

    let css = doc.collect_style_blocks();
    let sheet = Stylesheet::parse(&css);
    let styles = compute_styles(&doc, &[sheet]);

    let (layout, hits, content_h) = LayoutEngine::layout(&doc, &styles, 800, 600);
    assert!(content_h >= 600);
    assert!(
        hits.iter()
            .any(|h| h.href.as_deref() == Some("https://example.com")),
        "expected clickable href hit target"
    );

    let mut canvas = OwnedCanvas::new(800, 600);
    let mut view = canvas.view_mut();
    paint_layout(&layout, &mut view, 0);

    // Expect card background color #eef4ff (r=0xee, g=0xf4, b=0xff) somewhere on canvas
    let target_bg = Color::rgb(0xEE, 0xF4, 0xFF).to_bgra();
    assert!(
        canvas.pixels.chunks_exact(4).any(|px| px == target_bg),
        "expected CSS card background to be painted"
    );
}
