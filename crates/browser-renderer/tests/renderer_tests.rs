use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use browser_dom::{ControlKind, LayoutBox};
use browser_gfx::{Color, OwnedCanvas};
use browser_net::HttpClient;
use browser_renderer::load_document;

fn collect_rendered_text(b: &LayoutBox, out: &mut Vec<String>) {
    for r in &b.text_runs {
        out.push(r.text.clone());
    }
    if let Some(ControlKind::TextInput { value, .. }) = &b.control {
        if !value.is_empty() {
            out.push(value.clone());
        }
    }
    for ch in &b.children {
        collect_rendered_text(ch, out);
    }
}

fn find_control<'a>(
    b: &'a LayoutBox,
    pred: &dyn Fn(&ControlKind) -> bool,
) -> Option<&'a LayoutBox> {
    if let Some(c) = &b.control {
        if pred(c) {
            return Some(b);
        }
    }
    for ch in &b.children {
        if let Some(found) = find_control(ch, pred) {
            return Some(found);
        }
    }
    None
}

#[test]
fn renderer_loads_images_hides_hidden_inputs_and_renders_clean_controls() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    // Minimal valid 2x2 24-bpp BMP (66 bytes)
    let mut bmp = Vec::new();
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&70u32.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&2i32.to_le_bytes());
    bmp.extend_from_slice(&2i32.to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    bmp.extend_from_slice(&[0u8; 24]);
    bmp.extend_from_slice(&[244, 133, 66, 244, 133, 66, 0, 0]);
    bmp.extend_from_slice(&[53, 67, 234, 53, 67, 234, 0, 0]);

    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            if req.starts_with("GET /logo.bmp ") {
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/bmp\r\nContent-Length: {}\r\n\r\n",
                    bmp.len()
                );
                s.write_all(head.as_bytes()).unwrap();
                s.write_all(&bmp).unwrap();
            } else {
                let html = b"<html><body bgcolor=\"#fff\"><center>\
                    <img src=\"/logo.bmp\" width=\"200\" height=\"80\" alt=\"Logo\">\
                    <form action=\"/search\">\
                    <input type=\"hidden\" name=\"ie\" value=\"ISO-8859-1\">\
                    <input name=\"q\" size=\"50\">\
                    <input type=\"submit\" value=\"Search Now\">\
                    </form><p>&copy; 2026</p></center></body></html>";
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=ISO-8859-1\r\nContent-Length: {}\r\n\r\n",
                    html.len()
                );
                s.write_all(head.as_bytes()).unwrap();
                s.write_all(html).unwrap();
            }
        }
    });

    let mut http = HttpClient::new();
    let loaded = load_document(&mut http, &format!("http://{addr}/"), 1024, 640).unwrap();
    assert!(loaded.images.contains_key("/logo.bmp"));

    let mut texts = Vec::new();
    collect_rendered_text(&loaded.layout, &mut texts);
    let joined = texts.join(" ");
    assert!(!joined.contains("ISO-8859-1"), "hidden input leaked: {joined}");
    assert!(joined.contains('\u{00A9}'));
    assert!(
        find_control(&loaded.layout, &|c| matches!(c, ControlKind::TextInput { name, .. } if name == "q"))
            .is_some()
    );

    let mut canvas = OwnedCanvas::new(1024, 640);
    let mut view = canvas.view_mut();
    loaded.paint_into_canvas(&mut view);
    let google_blue = Color::rgb(66, 133, 244).to_bgra();
    assert!(canvas.pixels.chunks_exact(4).any(|px| px == google_blue));

    server.join().unwrap();
}

#[test]
#[ignore]
fn live_https_google_and_example_render_check() {
    let mut http = HttpClient::new();

    let ex = load_document(&mut http, "https://example.com/", 1280, 720).unwrap();
    assert_eq!(ex.title, "Example Domain");

    let g = load_document(&mut http, "https://www.google.com/", 1280, 720).unwrap();
    assert!(
        !g.images.is_empty(),
        "expected decoded Google logo image, got 0 images"
    );
    let mut texts = Vec::new();
    collect_rendered_text(&g.layout, &mut texts);
    let joined = texts.join(" ");
    assert!(
        !joined.contains("ISO-8859-1") && !joined.contains("AHSINF"),
        "hidden inputs leaked into rendered text: {joined}"
    );
    assert!(
        find_control(&g.layout, &|c| matches!(c, ControlKind::TextInput { name, .. } if name == "q"))
            .is_some(),
        "expected search input q"
    );

    let mut canvas = OwnedCanvas::new(1280, 720);
    let mut view = canvas.view_mut();
    g.paint_into_canvas(&mut view);
    let non_white = canvas
        .pixels
        .chunks_exact(4)
        .filter(|px| px[0] < 250 || px[1] < 250 || px[2] < 250)
        .count();
    assert!(
        non_white > 5_000,
        "expected substantial rendered pixels on google.com, got {non_white}"
    );
}
