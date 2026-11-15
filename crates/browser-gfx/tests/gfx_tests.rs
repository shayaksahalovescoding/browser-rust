use browser_gfx::{
    Color, OwnedCanvas, Rect, SvgElement, decode_data_uri, measure_text, rasterize_svg,
};

#[test]
fn canvas_clipping_and_alpha_blend() {
    let mut canvas = OwnedCanvas::new(10, 10);
    let mut view = canvas.view_mut();
    view.clear(Color::rgb(200, 100, 50));

    // Out-of-bounds fill must safely clip without panicking.
    view.fill_rect(Rect::new(-5, -5, 10, 10), Color::rgba(0, 0, 200, 128));
    // Pixel (0, 0) was blended; pixel (9, 9) stayed clear color.
    let p00 = &view.pixels()[0..4];
    assert!(p00[0] > 50); // blue increased
    let idx99 = (9 * 10 + 9) * 4;
    assert_eq!(&view.pixels()[idx99..idx99 + 4], &[50, 100, 200, 255]);
}

#[test]
fn rounded_rect_and_text_render_pixels() {
    let mut canvas = OwnedCanvas::new(120, 40);
    let mut view = canvas.view_mut();
    view.clear(Color::rgb(255, 255, 255));
    view.fill_rounded_rect(Rect::new(4, 4, 100, 30), 8, Color::rgb(30, 90, 200));

    // Corner pixel (4, 4) should remain white due to rounded corner
    let idx_corner = (4 * 120 + 4) * 4;
    assert_eq!(
        &view.pixels()[idx_corner..idx_corner + 4],
        &[255, 255, 255, 255]
    );

    // Center pixel (50, 20) should be filled blue
    let idx_center = (20 * 120 + 50) * 4;
    assert_eq!(
        &view.pixels()[idx_center..idx_center + 4],
        &[200, 90, 30, 255]
    );

    let (w, h) = measure_text(2, "Chrome");
    assert!(w > 20 && h >= 14);
    view.draw_text(10, 12, 2, Color::rgb(255, 255, 255), "Chrome");
    // Some pixel in the text area should now be white
    let mut saw_white = false;
    for y in 12..32 {
        for x in 10..60 {
            let idx = (y * 120 + x) * 4;
            if &view.pixels()[idx..idx + 4] == &[255, 255, 255, 255] {
                saw_white = true;
            }
        }
    }
    assert!(saw_white);
}

#[test]
fn decode_bmp_via_gdiplus() {
    // Minimal 1x1 24bpp BMP
    let bmp: [u8; 58] = [
        b'B', b'M', 58, 0, 0, 0, 0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1,
        0, 24, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
        0x20, 0x30, 0,
    ];
    let img = browser_gfx::decode_image(&bmp).unwrap();
    assert_eq!((img.width, img.height), (1, 1));
    assert_eq!(img.bgra, vec![0x10, 0x20, 0x30, 255]);
}

#[test]
fn svg_rasterizer_and_data_uri_decode() {
    let elements = vec![
        SvgElement::Rect {
            x: 0.0,
            y: 0.0,
            w: 24.0,
            h: 24.0,
            fill: Color::rgb(66, 133, 244),
        },
        SvgElement::Circle {
            cx: 12.0,
            cy: 12.0,
            r: 6.0,
            fill: Color::rgb(255, 255, 255),
        },
        SvgElement::Path {
            d: "M 4 20 L 12 4 L 20 20 Z".to_string(),
            fill: Color::rgb(52, 168, 83),
        },
    ];
    let svg_img = rasterize_svg((0.0, 0.0, 24.0, 24.0), 48, 48, &elements)
        .expect("svg should rasterize");
    assert_eq!((svg_img.width, svg_img.height), (48, 48));
    assert!(svg_img.bgra.chunks_exact(4).any(|px| px[3] > 0 && px[1] > 100));

    // Same 1x1 24bpp BMP (58 bytes) base64-encoded as data URI
    let data_uri = "data:image/bmp;base64,Qk06AAAAAAAAADYAAAAoAAAAAQAAAAEAAAABABgAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAECAwAA==";
    let decoded = decode_data_uri(data_uri).expect("data URI BMP should decode");
    assert_eq!((decoded.width, decoded.height), (1, 1));
    assert_eq!(decoded.bgra, vec![0x10, 0x20, 0x30, 255]);
}
