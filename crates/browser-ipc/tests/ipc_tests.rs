use std::io::Cursor;

use browser_ipc::{
    BrowserToRenderer, MAX_FRAME_BYTES, MouseButton, RendererToBrowser, SharedFrameBuffer,
    read_frame, write_frame,
};

#[test]
fn framing_roundtrip_and_eof() {
    let mut buf = Vec::new();
    write_frame(&mut buf, b"hello").unwrap();
    write_frame(&mut buf, b"world!").unwrap();

    let mut cur = Cursor::new(buf);
    assert_eq!(read_frame(&mut cur).unwrap(), Some(b"hello".to_vec()));
    assert_eq!(read_frame(&mut cur).unwrap(), Some(b"world!".to_vec()));
    assert_eq!(read_frame(&mut cur).unwrap(), None);
}

#[test]
fn framing_rejects_oversized_or_zero() {
    let oversized = ((MAX_FRAME_BYTES as u32) + 1).to_le_bytes();
    let mut cur = Cursor::new(oversized.to_vec());
    assert!(read_frame(&mut cur).is_err());

    let zero = 0u32.to_le_bytes();
    let mut cur_zero = Cursor::new(zero.to_vec());
    assert!(read_frame(&mut cur_zero).is_err());
}

#[test]
fn protocol_roundtrips() {
    let msgs = vec![
        BrowserToRenderer::Navigate {
            nav_id: 42,
            url: "chrome://version".into(),
        },
        BrowserToRenderer::Resize {
            width: 1280,
            height: 720,
            shm_name: "Local\\TestShm".into(),
        },
        BrowserToRenderer::MouseMove { x: 100, y: -5 },
        BrowserToRenderer::MouseDown {
            x: 320,
            y: 240,
            button: MouseButton::Left,
        },
        BrowserToRenderer::KeyPress { ch: 'G' },
        BrowserToRenderer::Scroll { delta_y: -120 },
        BrowserToRenderer::Shutdown,
    ];
    for msg in msgs {
        let bytes = msg.encode();
        let decoded = BrowserToRenderer::decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    let replies = vec![
        RendererToBrowser::Ready { pid: 9999 },
        RendererToBrowser::FrameCommitted {
            nav_id: 42,
            shm_name: "Local\\TestShm".into(),
            width: 800,
            height: 600,
            title: "Version".into(),
        },
        RendererToBrowser::LoadFailed {
            nav_id: 43,
            error: "boom".into(),
        },
        RendererToBrowser::RequestNavigate {
            url: "chrome://newtab".into(),
        },
        RendererToBrowser::DevToolsInfo {
            nav_id: 42,
            network_summary: "200 OK".into(),
            dom_dump: "<html>".into(),
            console_logs: "hello".into(),
        },
        RendererToBrowser::CursorChanged {
            cursor: 1,
            hover_url: Some("https://example.com/".into()),
        },
        RendererToBrowser::CursorChanged {
            cursor: 0,
            hover_url: None,
        },
    ];
    for r in replies {
        let bytes = r.encode();
        let decoded = RendererToBrowser::decode(&bytes).unwrap();
        assert_eq!(r, decoded);
    }
}

#[test]
fn shared_framebuffer_cross_handle_transfer() {
    let name = format!("Local\\ChromeScratch_Test_{}", std::process::id());
    let mut owner = SharedFrameBuffer::create(&name, 16, 8).unwrap();
    let mut reader = SharedFrameBuffer::open(&name, 16, 8).unwrap();

    owner.pixels_mut()[0..4].copy_from_slice(&[0x11, 0x22, 0x33, 0xFF]);
    let generation = owner.commit_generation();
    assert_eq!(generation, 1);

    assert_eq!(reader.header().generation, 1);
    assert_eq!(&reader.pixels()[0..4], &[0x11, 0x22, 0x33, 0xFF]);

    reader.pixels_mut()[4..8].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xFF]);
    assert_eq!(&owner.pixels()[4..8], &[0xAA, 0xBB, 0xCC, 0xFF]);
}
