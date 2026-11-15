use browser_gfx::{Color, OwnedCanvas, Rect, measure_text};

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
