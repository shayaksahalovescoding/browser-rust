use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use browser_ipc::BrowserToRenderer;
use browser_shell::process::{ProcessManager, RendererEventKind};
use browser_shell::tabs::{TabHistory, TabManager, normalize_omnibox_input};
use browser_shell::ui::{OmniboxState, UiAction, UiLayout};

#[test]
fn tab_history_and_omnibox_normalization() {
    let mut hist = TabHistory::new("chrome://newtab".into());
    assert!(!hist.can_back());
    assert!(!hist.can_forward());

    hist.push("chrome://version".into());
    hist.push("chrome://architecture".into());
    assert!(hist.can_back());
    assert!(!hist.can_forward());
    assert_eq!(hist.back(), Some("chrome://version"));
    assert!(hist.can_forward());
    assert_eq!(hist.forward(), Some("chrome://architecture"));

    // Navigating after back truncates forward history
    assert_eq!(hist.back(), Some("chrome://version"));
    hist.push("about:blank".into());
    assert!(!hist.can_forward());
    assert_eq!(hist.current(), "about:blank");

    assert_eq!(normalize_omnibox_input(""), "chrome://newtab");
    assert_eq!(
        normalize_omnibox_input("chrome://version"),
        "chrome://version"
    );
    assert_eq!(normalize_omnibox_input("about:blank"), "about:blank");
    assert_eq!(
        normalize_omnibox_input("example.com"),
        "https://example.com"
    );
    assert_eq!(
        normalize_omnibox_input("rust browser"),
        "chrome://search?q=rust+browser"
    );
}

#[test]
fn tab_manager_and_ui_hit_testing() {
    let mut mgr = TabManager::new();
    assert_eq!(mgr.tabs().len(), 1);
    let t2 = mgr.open_tab("chrome://version".into());
    assert_eq!(mgr.tabs().len(), 2);
    assert_eq!(mgr.active_tab().unwrap().id, t2);
    assert!(mgr.select_tab(0));
    assert_eq!(mgr.active_index(), 0);
    mgr.close_tab(1);
    assert_eq!(mgr.tabs().len(), 1);

    let layout = UiLayout::compute(1000, 700, 1, false, false);
    assert_eq!(
        layout.hit_test(layout.back_rect.x + 5, layout.back_rect.y + 5),
        Some(UiAction::Back)
    );
    assert_eq!(
        layout.hit_test(layout.devtools_rect.x + 5, layout.devtools_rect.y + 5),
        Some(UiAction::ToggleDevTools)
    );
    assert_eq!(
        layout.hit_test(layout.omnibox_rect.x + 10, layout.omnibox_rect.y + 10),
        Some(UiAction::FocusOmnibox)
    );
    assert_eq!(
        layout.hit_test(200, 200),
        Some(UiAction::ViewportClick { x: 200, y: 120 })
    );

    let mut box_state = OmniboxState::new("chrome://newtab");
    box_state.focus_and_select_all();
    box_state.insert_char('a');
    assert_eq!(box_state.text, "a");
    box_state.backspace();
    assert_eq!(box_state.text, "");
}

#[test]
fn multiprocess_renderer_lifecycle_and_crash_isolation() {
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_browser-shell"));
    let (tx, rx) = mpsc::channel();
    let wakeup = Arc::new(|| {});
    let mut procs = ProcessManager::new(tx, wakeup);

    procs.spawn_renderer_with_exe(&exe, 1, 640, 480).unwrap();

    // 1. Expect Ready from real child renderer process
    let ev = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(ev.tab_id, 1);
    let child_pid = match ev.kind {
        RendererEventKind::Ready { pid } => pid,
        other => panic!("expected Ready, got {other:?}"),
    };
    assert_ne!(child_pid, std::process::id());

    // 2. Navigate to chrome://version and verify rendered BGRA8 pixels in shared memory
    procs
        .get_mut(1)
        .unwrap()
        .send(&BrowserToRenderer::Navigate {
            nav_id: 1,
            url: "chrome://version".into(),
        })
        .unwrap();

    let ev2 = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    match ev2.kind {
        RendererEventKind::FrameCommitted {
            nav_id,
            width,
            height,
            title,
            ..
        } => {
            assert_eq!(nav_id, 1);
            assert_eq!((width, height), (640, 480));
            assert_eq!(title, "About Version");
        }
        other => panic!("expected FrameCommitted, got {other:?}"),
    }
    let dt1 = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(dt1.kind, RendererEventKind::DevToolsInfo { .. }));

    let pixels = procs.get(1).unwrap().pixels();
    assert_eq!(pixels.len(), 640 * 480 * 4);
    let has_non_white = pixels.chunks_exact(4).any(|p| p != [255, 255, 255, 255]);
    assert!(has_non_white, "expected rendered text/UI pixels in shm");

    // 3. Navigate to chrome://demo (HTML5 + CSS3 + Flexbox + JS engine)
    procs
        .get_mut(1)
        .unwrap()
        .send(&BrowserToRenderer::Navigate {
            nav_id: 2,
            url: "chrome://demo".into(),
        })
        .unwrap();

    let ev_demo = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    match ev_demo.kind {
        RendererEventKind::FrameCommitted { nav_id, title, .. } => {
            assert_eq!(nav_id, 2);
            assert_eq!(title, "Chrome Scratch Engine Demo");
        }
        other => panic!("expected FrameCommitted for chrome://demo, got {other:?}"),
    }
    let dt_demo = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    match dt_demo.kind {
        RendererEventKind::DevToolsInfo {
            dom_dump,
            console_logs,
            ..
        } => {
            assert!(dom_dump.contains("<h1>"));
            assert!(console_logs.contains("chrome://demo script initialized"));
        }
        other => panic!("expected DevToolsInfo for chrome://demo, got {other:?}"),
    }

    // 4. Trigger chrome://crash and verify parent stays safe and receives Crashed event
    procs
        .get_mut(1)
        .unwrap()
        .send(&BrowserToRenderer::Navigate {
            nav_id: 3,
            url: "chrome://crash".into(),
        })
        .unwrap();

    let ev3 = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(ev3.kind, RendererEventKind::Crashed));
}
