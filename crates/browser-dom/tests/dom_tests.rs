use std::collections::HashMap;

use browser_dom::{
    ControlKind, LayoutBox, LayoutEngine, Stylesheet, compute_styles, paint_layout,
    paint_layout_with_assets, parse_html,
};
use browser_gfx::{Color, DecodedImage, OwnedCanvas};

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

fn find_box_by_control<'a>(b: &'a LayoutBox, pred: &dyn Fn(&ControlKind) -> bool) -> Option<&'a LayoutBox> {
    if let Some(c) = &b.control {
        if pred(c) {
            return Some(b);
        }
    }
    for ch in &b.children {
        if let Some(found) = find_box_by_control(ch, pred) {
            return Some(found);
        }
    }
    None
}

#[test]
fn hidden_inputs_tables_entities_and_decoded_images() {
    let html = r#"
        <html><body>
          <center>
            <img src="/logo.png" width="80" height="40" alt="Brand">
            <form action="/search">
              <table>
                <tr>
                  <td width="25%"></td>
                  <td align="center">
                    <input type="hidden" name="ie" value="ISO-8859-1">
                    <input type="text" name="q" size="40" placeholder="Search">
                    <br>
                    <input type="submit" value="Go Search">
                  </td>
                  <td width="25%"><a href="/adv">Adv</a></td>
                </tr>
              </table>
            </form>
            <p>&copy; 2026 Test</p>
          </center>
        </body></html>
    "#;

    let doc = parse_html(html);
    assert!(doc.text_content(doc.root()).contains('\u{00A9}'));

    let styles = compute_styles(&doc, &[]);
    let (layout, hits, _) = LayoutEngine::layout(&doc, &styles, 1000, 600);

    // Hidden input must NOT produce a control box or hit target
    assert!(find_box_by_control(&layout, &|c| matches!(c, ControlKind::TextInput { name, .. } if name == "ie")).is_none());
    let q_box = find_box_by_control(&layout, &|c| matches!(c, ControlKind::TextInput { name, .. } if name == "q"))
        .expect("visible text input should exist");
    let submit_box = find_box_by_control(&layout, &|c| matches!(c, ControlKind::SubmitButton { label, .. } if label == "Go Search"))
        .expect("submit button should exist");

    // Input and submit button should be inside centered middle table column (between 250 and 750)
    assert!(q_box.rect.x >= 250 && q_box.rect.x + q_box.rect.w <= 760);
    assert!(submit_box.rect.y > q_box.rect.y);
    assert!(hits.iter().any(|h| h.node_id == q_box.node_id));

    // Paint with a 2x2 solid green DecodedImage for "/logo.png"
    let green = Color::rgb(12, 200, 88).to_bgra();
    let mut bgra = Vec::new();
    for _ in 0..4 {
        bgra.extend_from_slice(&green);
    }
    let mut images = HashMap::new();
    images.insert(
        "/logo.png".to_string(),
        DecodedImage {
            width: 2,
            height: 2,
            bgra,
        },
    );

    let mut canvas = OwnedCanvas::new(1000, 600);
    let mut view = canvas.view_mut();
    paint_layout_with_assets(&layout, &mut view, 0, &images, Some((q_box.node_id, "rust browser")));
    assert!(canvas.pixels.chunks_exact(4).any(|px| px == green));
}

#[test]
fn flex_shifted_links_card_links_and_textarea_controls_align() {
    let html = r#"
        <html>
          <head>
            <style>
              .bar { display: flex; justify-content: center; gap: 20px; }
              textarea.box { display: flex; width: 360px; }
            </style>
          </head>
          <body>
            <div class="bar">
              <a href="/first">First Link</a>
              <a href="/second">Second Link</a>
            </div>
            <a href="/card"><div style="padding: 10px;">Card Title</div></a>
            <textarea class="box" name="q" placeholder="Search"></textarea>
          </body>
        </html>
    "#;

    let doc = parse_html(html);
    let css = doc.collect_style_blocks();
    let sheet = Stylesheet::parse(&css);
    let styles = compute_styles(&doc, &[sheet]);
    let (layout, hits, _) = LayoutEngine::layout(&doc, &styles, 1000, 600);

    let first_hit = hits
        .iter()
        .find(|h| h.href.as_deref() == Some("/first"))
        .expect("/first link hit target missing");
    let second_hit = hits
        .iter()
        .find(|h| h.href.as_deref() == Some("/second"))
        .expect("/second link hit target missing");
    // Centered flex items in 1000px viewport must not have hitboxes left at x=0
    assert!(
        first_hit.rect.x > 100 && second_hit.rect.x > first_hit.rect.x,
        "flex link hits did not shift properly: first={:?}, second={:?}",
        first_hit.rect,
        second_hit.rect
    );

    let card_hit = hits
        .iter()
        .find(|h| h.href.as_deref() == Some("/card"))
        .expect("/card link hit target missing");
    assert!(card_hit.rect.w > 40 && card_hit.rect.h >= 14);

    let q_box = find_box_by_control(&layout, &|c| matches!(c, ControlKind::TextInput { name, .. } if name == "q"))
        .expect("textarea with display: flex must still emit TextInput control");
    assert!(q_box.rect.w >= 300);
}

#[test]
fn noscript_styles_ignored_and_inline_punctuation_glued() {
    let html = r#"
        <html>
          <head></head>
          <body>
            <noscript>
              <style>div,p,span{display:none}</style>
              <meta http-equiv="refresh" content="0;url=/enablejs">
            </noscript>
            <div id="main"><p>Click <b>Settings</b>.</p></div>
          </body>
        </html>
    "#;
    let doc = parse_html(html);
    let css = doc.collect_style_blocks();
    assert!(css.trim().is_empty(), "style inside <noscript> leaked: {css}");

    let sheet = Stylesheet::parse(&css);
    let styles = compute_styles(&doc, &[sheet]);
    let (layout, _, _) = LayoutEngine::layout(&doc, &styles, 800, 600);

    fn collect_runs(b: &LayoutBox, out: &mut Vec<(String, i32, i32)>) {
        for r in &b.text_runs {
            out.push((r.text.clone(), r.x, r.w));
        }
        for c in &b.children {
            collect_runs(c, out);
        }
    }
    let mut runs = Vec::new();
    collect_runs(&layout, &mut runs);
    let settings = runs.iter().find(|r| r.0 == "Settings").expect("Settings run missing");
    let dot = runs.iter().find(|r| r.0 == ".").expect("period run missing");
    assert_eq!(
        dot.1,
        settings.1 + settings.2,
        "period should glue directly onto Settings: settings={settings:?}, dot={dot:?}"
    );
}
