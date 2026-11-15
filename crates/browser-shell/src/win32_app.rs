use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use browser_gfx::OwnedCanvas;
use browser_ipc::{BrowserToRenderer, MouseButton};

use crate::process::{ProcessManager, RendererEvent, RendererEventKind};
use crate::tabs::{DevToolsSnapshot, TabManager, TabStatus, normalize_omnibox_input};
use crate::ui::{
    CHROME_TOP_H, DEVTOOLS_H, OmniboxState, UiAction, UiLayout, render_browser_frame,
    sync_omnibox_with_tab,
};

const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_PAINT: u32 = 0x000F;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_SETCURSOR: u32 = 0x0020;
const WM_KEYDOWN: u32 = 0x0100;
const WM_CHAR: u32 = 0x0102;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_APP_IPC: u32 = 0x8001;

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const CW_USEDEFAULT: i32 = -2147483648;
const SW_SHOW: i32 = 5;
const CS_HREDRAW: u32 = 0x0002;
const CS_VREDRAW: u32 = 0x0001;
const HTCLIENT: isize = 1;
const IDC_ARROW: usize = 32512;
const IDC_IBEAM: usize = 32513;
const IDC_HAND: usize = 32649;
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOZORDER: u32 = 0x0004;
const GWLP_USERDATA: i32 = -21;
const BI_RGB: u32 = 0;
const DIB_RGB_COLORS: u32 = 0;
const SRCCOPY: u32 = 0x00CC_0020;

const VK_RETURN: usize = 0x0D;
const VK_CONTROL: i32 = 0x11;
const VK_ESCAPE: usize = 0x1B;
const VK_END: usize = 0x23;
const VK_HOME: usize = 0x24;
const VK_LEFT: usize = 0x25;
const VK_UP: usize = 0x26;
const VK_RIGHT: usize = 0x27;
const VK_DOWN: usize = 0x28;
const VK_PRIOR: usize = 0x21;
const VK_NEXT: usize = 0x22;
const VK_F5: usize = 0x74;
const VK_F12: usize = 0x7B;

type WndProc = unsafe extern "system" fn(isize, u32, usize, isize) -> isize;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct RectWin {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct WndClassExW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: WndProc,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: isize,
    h_icon: isize,
    h_cursor: isize,
    hbr_background: isize,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: isize,
}

#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: Point,
    l_private: u32,
}

#[repr(C)]
struct PaintStruct {
    hdc: isize,
    f_erase: i32,
    rc_paint: RectWin,
    f_restore: i32,
    f_inc_update: i32,
    rgb_reserved: [u8; 32],
}

#[repr(C)]
struct BitmapInfoHeader {
    bi_size: u32,
    bi_width: i32,
    bi_height: i32,
    bi_planes: u16,
    bi_bit_count: u16,
    bi_compression: u32,
    bi_size_image: u32,
    bi_x_pels_per_meter: i32,
    bi_y_pels_per_meter: i32,
    bi_clr_used: u32,
    bi_clr_important: u32,
}

#[repr(C)]
struct BitmapInfo {
    bmi_header: BitmapInfoHeader,
    bmi_colors: [u32; 1],
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(lp_module_name: *const u16) -> isize;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn SetProcessDPIAware() -> i32;
    fn SetProcessDpiAwarenessContext(value: isize) -> i32;
    fn GetDpiForWindow(hwnd: isize) -> u32;
    fn SetWindowPos(
        h_wnd: isize,
        h_wnd_insert_after: isize,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        u_flags: u32,
    ) -> i32;
    fn LoadCursorW(h_instance: isize, lp_cursor_name: *const u16) -> isize;
    fn SetCursor(h_cursor: isize) -> isize;
    fn RegisterClassExW(lpwcx: *const WndClassExW) -> u16;
    fn CreateWindowExW(
        dw_ex_style: u32,
        lp_class_name: *const u16,
        lp_window_name: *const u16,
        dw_style: u32,
        x: i32,
        y: i32,
        n_width: i32,
        n_height: i32,
        h_wnd_parent: isize,
        h_menu: isize,
        h_instance: isize,
        lp_param: *mut u8,
    ) -> isize;
    fn ShowWindow(h_wnd: isize, n_cmd_show: i32) -> i32;
    fn UpdateWindow(h_wnd: isize) -> i32;
    fn GetMessageW(lp_msg: *mut Msg, h_wnd: isize, w_msg_filter_min: u32, w_msg_filter_max: u32) -> i32;
    fn TranslateMessage(lp_msg: *const Msg) -> i32;
    fn DispatchMessageW(lp_msg: *const Msg) -> isize;
    fn DefWindowProcW(h_wnd: isize, msg: u32, w_param: usize, l_param: isize) -> isize;
    fn PostQuitMessage(n_exit_code: i32);
    fn PostMessageW(h_wnd: isize, msg: u32, w_param: usize, l_param: isize) -> i32;
    fn InvalidateRect(h_wnd: isize, lp_rect: *const RectWin, b_erase: i32) -> i32;
    fn BeginPaint(h_wnd: isize, lp_paint: *mut PaintStruct) -> isize;
    fn EndPaint(h_wnd: isize, lp_paint: *const PaintStruct) -> i32;
    fn GetClientRect(h_wnd: isize, lp_rect: *mut RectWin) -> i32;
    fn GetKeyState(n_virt_key: i32) -> i16;
    fn GetWindowLongPtrW(h_wnd: isize, n_index: i32) -> isize;
    fn SetWindowLongPtrW(h_wnd: isize, n_index: i32, dw_new_long: isize) -> isize;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn StretchDIBits(
        hdc: isize,
        x_dest: i32,
        y_dest: i32,
        dest_width: i32,
        dest_height: i32,
        x_src: i32,
        y_src: i32,
        src_width: i32,
        src_height: i32,
        lp_bits: *const u8,
        lpbmi: *const BitmapInfo,
        i_usage: u32,
        rop: u32,
    ) -> i32;
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct AppState {
    hwnd: isize,
    tabs: TabManager,
    procs: ProcessManager,
    rx: Receiver<RendererEvent>,
    omnibox: OmniboxState,
    compositor: OwnedCanvas,
    hover: Option<(i32, i32)>,
    page_cursor: u8,
    hover_url: Option<String>,
    devtools_open: bool,
}

impl AppState {
    fn viewport_dims(&self) -> (u32, u32) {
        let w = self.compositor.width.max(1);
        let drawer = if self.devtools_open { DEVTOOLS_H } else { 0 };
        let h = (self.compositor.height as i32 - CHROME_TOP_H - drawer).max(80) as u32;
        (w, h)
    }

    fn toggle_devtools(&mut self) {
        self.devtools_open = !self.devtools_open;
        let (vw, vh) = self.viewport_dims();
        if let Some(active_id) = self.tabs.active_tab().map(|t| t.id) {
            let _ = self.procs.resize_tab(active_id, vw, vh);
        }
        self.invalidate();
    }

    fn spawn_and_navigate_tab(&mut self, tab_id: u64, url: String, push_history: bool) {
        let (vw, vh) = self.viewport_dims();
        let nav_id = self.tabs.alloc_nav_id();
        let is_active = self.tabs.active_tab().is_some_and(|a| a.id == tab_id);
        if is_active {
            self.page_cursor = 0;
            self.hover_url = None;
        }
        if let Some(tab) = self.tabs.find_by_id_mut(tab_id) {
            tab.url = url.clone();
            if push_history {
                tab.history.push(url.clone());
            }
            tab.status = TabStatus::Loading;
            tab.active_nav_id = nav_id;
            if is_active {
                sync_omnibox_with_tab(&mut self.omnibox, tab);
            }
        }

        let need_spawn = self
            .procs
            .get(tab_id)
            .is_none_or(|h| !h.is_alive());
        if need_spawn {
            if let Err(err) = self.procs.spawn_renderer(tab_id, vw, vh) {
                if let Some(tab) = self.tabs.find_by_id_mut(tab_id) {
                    tab.status = TabStatus::Crashed { exit_code: None };
                    tab.title = format!("Spawn Error: {err}");
                }
                return;
            }
        }
        let _ = self.procs.resize_tab(tab_id, vw, vh);
        if let Some(handle) = self.procs.get_mut(tab_id) {
            let _ = handle.send(&BrowserToRenderer::Navigate { nav_id, url });
        }
    }

    fn open_new_tab(&mut self, url: &str) {
        let id = self.tabs.open_tab(url.to_string());
        self.omnibox.focused = false;
        self.spawn_and_navigate_tab(id, url.to_string(), false);
        self.invalidate();
    }

    fn close_tab_at(&mut self, idx: usize) {
        if let Some(removed_id) = self.tabs.close_tab(idx) {
            self.procs.remove(removed_id);
            if self.tabs.tabs().is_empty() {
                unsafe { PostQuitMessage(0) };
                return;
            }
            if let Some(active) = self.tabs.active_tab().cloned() {
                let (vw, vh) = self.viewport_dims();
                let _ = self.procs.resize_tab(active.id, vw, vh);
                self.omnibox.focused = false;
                sync_omnibox_with_tab(&mut self.omnibox, &active);
            }
            self.invalidate();
        }
    }

    fn select_tab_at(&mut self, idx: usize) {
        if self.tabs.select_tab(idx) {
            self.omnibox.focused = false;
            self.page_cursor = 0;
            self.hover_url = None;
            if let Some(active) = self.tabs.active_tab().cloned() {
                sync_omnibox_with_tab(&mut self.omnibox, &active);
                let (vw, vh) = self.viewport_dims();
                let _ = self.procs.resize_tab(active.id, vw, vh);
            }
            self.invalidate();
        }
    }

    fn navigate_active(&mut self, raw_url: &str, push_history: bool) {
        if let Some(active_id) = self.tabs.active_tab().map(|t| t.id) {
            let url = normalize_omnibox_input(raw_url);
            self.omnibox.focused = false;
            self.spawn_and_navigate_tab(active_id, url, push_history);
            self.invalidate();
        }
    }

    fn reload_active(&mut self) {
        if let Some(active) = self.tabs.active_tab().cloned() {
            self.omnibox.focused = false;
            self.spawn_and_navigate_tab(active.id, active.url, false);
            self.invalidate();
        }
    }

    fn go_back(&mut self) {
        let next = self
            .tabs
            .active_tab_mut()
            .and_then(|t| t.history.back().map(|s| (t.id, s.to_string())));
        if let Some((tab_id, url)) = next {
            self.omnibox.focused = false;
            self.spawn_and_navigate_tab(tab_id, url, false);
            self.invalidate();
        }
    }

    fn go_forward(&mut self) {
        let next = self
            .tabs
            .active_tab_mut()
            .and_then(|t| t.history.forward().map(|s| (t.id, s.to_string())));
        if let Some((tab_id, url)) = next {
            self.omnibox.focused = false;
            self.spawn_and_navigate_tab(tab_id, url, false);
            self.invalidate();
        }
    }

    fn drain_ipc_events(&mut self) {
        let mut dirty = false;
        while let Ok(ev) = self.rx.try_recv() {
            dirty = true;
            match ev.kind {
                RendererEventKind::Ready { pid } => {
                    if let Some(tab) = self.tabs.find_by_id_mut(ev.tab_id) {
                        tab.renderer_pid = Some(pid);
                    }
                }
                RendererEventKind::FrameCommitted {
                    nav_id, title, ..
                } => {
                    if let Some(tab) = self.tabs.find_by_id_mut(ev.tab_id) {
                        if nav_id == tab.active_nav_id || nav_id == 0 {
                            tab.status = TabStatus::Ready;
                            tab.title = title;
                        }
                    }
                }
                RendererEventKind::LoadFailed { nav_id, error } => {
                    if let Some(tab) = self.tabs.find_by_id_mut(ev.tab_id) {
                        if nav_id == tab.active_nav_id {
                            tab.status = TabStatus::Ready;
                            tab.title = format!("Error: {error}");
                        }
                    }
                }
                RendererEventKind::RequestNavigate { url } => {
                    if self.tabs.active_tab().is_some_and(|t| t.id == ev.tab_id) {
                        self.navigate_active(&url, true);
                    } else {
                        self.spawn_and_navigate_tab(ev.tab_id, url, true);
                    }
                }
                RendererEventKind::DevToolsInfo {
                    nav_id,
                    network_summary,
                    dom_dump,
                    console_logs,
                } => {
                    if let Some(tab) = self.tabs.find_by_id_mut(ev.tab_id) {
                        if nav_id == tab.active_nav_id || nav_id == 0 {
                            tab.devtools = Some(DevToolsSnapshot {
                                network_summary,
                                dom_dump,
                                console_logs,
                            });
                        }
                    }
                }
                RendererEventKind::CursorChanged { cursor, hover_url } => {
                    if self.tabs.active_tab().is_some_and(|t| t.id == ev.tab_id) {
                        let cursor_changed = self.page_cursor != cursor;
                        let url_changed = self.hover_url != hover_url;
                        self.page_cursor = cursor;
                        self.hover_url = hover_url;
                        if cursor_changed {
                            self.apply_cursor();
                        }
                        if !url_changed {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                RendererEventKind::Crashed => {
                    let exit_code = self
                        .procs
                        .get_mut(ev.tab_id)
                        .and_then(|h| h.mark_crashed());
                    if let Some(tab) = self.tabs.find_by_id_mut(ev.tab_id) {
                        tab.status = TabStatus::Crashed { exit_code };
                    }
                }
            }
        }
        if dirty {
            self.invalidate();
        }
    }

    fn handle_resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.compositor.resize(w, h);
        let (vw, vh) = self.viewport_dims();
        if let Some(active_id) = self.tabs.active_tab().map(|t| t.id) {
            let _ = self.procs.resize_tab(active_id, vw, vh);
        }
        self.invalidate();
    }

    fn current_cursor_id(&self) -> usize {
        if let Some((hx, hy)) = self.hover {
            let crashed = self
                .tabs
                .active_tab()
                .is_some_and(|t| matches!(t.status, TabStatus::Crashed { .. }));
            let layout = UiLayout::compute(
                self.compositor.width as i32,
                self.compositor.height as i32,
                self.tabs.tabs().len(),
                crashed,
                self.devtools_open,
            );
            if layout.omnibox_rect.contains(hx, hy) {
                return IDC_IBEAM;
            }
            if hy >= CHROME_TOP_H && hy < CHROME_TOP_H + layout.viewport_h {
                return match self.page_cursor {
                    1 => IDC_HAND,
                    2 => IDC_IBEAM,
                    _ => IDC_ARROW,
                };
            }
        }
        IDC_ARROW
    }

    fn apply_cursor(&self) {
        let cid = self.current_cursor_id();
        unsafe {
            let hcur = LoadCursorW(0, cid as *const u16);
            if hcur != 0 {
                SetCursor(hcur);
            }
        }
    }

    fn handle_mouse_move(&mut self, x: i32, y: i32) {
        let prev_hover = self.hover;
        self.hover = Some((x, y));
        let (_, vh) = self.viewport_dims();
        let in_viewport = y >= CHROME_TOP_H && y < CHROME_TOP_H + vh as i32;
        if in_viewport {
            if let Some(active) = self.tabs.active_tab() {
                if !matches!(active.status, TabStatus::Crashed { .. }) {
                    if let Some(handle) = self.procs.get_mut(active.id) {
                        let _ = handle.send(&BrowserToRenderer::MouseMove {
                            x,
                            y: y - CHROME_TOP_H,
                        });
                    }
                }
            }
            self.apply_cursor();
            if prev_hover.is_some_and(|(_, py)| py < CHROME_TOP_H) {
                self.invalidate();
            }
        } else {
            self.page_cursor = 0;
            self.hover_url = None;
            self.apply_cursor();
            self.invalidate();
        }
    }

    fn handle_mouse_down(&mut self, x: i32, y: i32) {
        let crashed = self
            .tabs
            .active_tab()
            .is_some_and(|t| matches!(t.status, TabStatus::Crashed { .. }));
        let layout = UiLayout::compute(
            self.compositor.width as i32,
            self.compositor.height as i32,
            self.tabs.tabs().len(),
            crashed,
            self.devtools_open,
        );
        match layout.hit_test(x, y) {
            Some(UiAction::SelectTab(idx)) => self.select_tab_at(idx),
            Some(UiAction::CloseTab(idx)) => self.close_tab_at(idx),
            Some(UiAction::NewTab) => self.open_new_tab("chrome://newtab"),
            Some(UiAction::Back) => self.go_back(),
            Some(UiAction::Forward) => self.go_forward(),
            Some(UiAction::Reload) => self.reload_active(),
            Some(UiAction::ToggleDevTools) => self.toggle_devtools(),
            Some(UiAction::FocusOmnibox) => {
                self.omnibox.focus_and_select_all();
                self.invalidate();
            }
            Some(UiAction::ViewportClick { x: vx, y: vy }) => {
                if self.omnibox.focused {
                    self.omnibox.focused = false;
                    if let Some(active) = self.tabs.active_tab() {
                        sync_omnibox_with_tab(&mut self.omnibox, active);
                    }
                    self.invalidate();
                }
                if let Some(active_id) = self.tabs.active_tab().map(|t| t.id) {
                    if let Some(handle) = self.procs.get_mut(active_id) {
                        let _ = handle.send(&BrowserToRenderer::MouseDown {
                            x: vx,
                            y: vy,
                            button: MouseButton::Left,
                        });
                    }
                }
            }
            None => {}
        }
    }

    fn handle_key_down(&mut self, vk: usize) {
        let ctrl = unsafe { GetKeyState(VK_CONTROL) } < 0;
        if ctrl {
            match vk as u8 as char {
                'L' => {
                    self.omnibox.focus_and_select_all();
                    self.invalidate();
                    return;
                }
                'T' => {
                    self.open_new_tab("chrome://newtab");
                    return;
                }
                'W' => {
                    let idx = self.tabs.active_index();
                    self.close_tab_at(idx);
                    return;
                }
                'R' => {
                    self.reload_active();
                    return;
                }
                _ => {}
            }
        }
        if vk == VK_F5 {
            self.reload_active();
            return;
        }
        if vk == VK_F12 {
            self.toggle_devtools();
            return;
        }
        if !self.omnibox.focused {
            let scroll_delta = match vk {
                VK_DOWN => Some(-48),
                VK_UP => Some(48),
                VK_NEXT => Some(-240),
                VK_PRIOR => Some(240),
                _ => None,
            };
            if let Some(delta_y) = scroll_delta {
                self.send_scroll(delta_y);
                return;
            }
        }
        if self.omnibox.focused {
            match vk {
                VK_RETURN => {
                    let entered = self.omnibox.text.clone();
                    self.navigate_active(&entered, true);
                }
                VK_ESCAPE => {
                    self.omnibox.focused = false;
                    if let Some(active) = self.tabs.active_tab() {
                        sync_omnibox_with_tab(&mut self.omnibox, active);
                    }
                    self.invalidate();
                }
                VK_LEFT => {
                    self.omnibox.move_left();
                    self.invalidate();
                }
                VK_RIGHT => {
                    self.omnibox.move_right();
                    self.invalidate();
                }
                VK_HOME => {
                    self.omnibox.select_all = false;
                    self.omnibox.cursor = 0;
                    self.invalidate();
                }
                VK_END => {
                    self.omnibox.select_all = false;
                    self.omnibox.cursor = self.omnibox.text.chars().count();
                    self.invalidate();
                }
                _ => {}
            }
        }
    }

    fn handle_char(&mut self, code: u32) {
        if self.omnibox.focused {
            if code == 8 {
                self.omnibox.backspace();
                self.invalidate();
            } else if code >= 0x20 && code != 0x7F {
                if let Some(ch) = char::from_u32(code) {
                    self.omnibox.insert_char(ch);
                    self.invalidate();
                }
            }
        } else if code == 8 || code == 13 || (code >= 0x20 && code != 0x7F) {
            if let (Some(ch), Some(active_id)) =
                (char::from_u32(code), self.tabs.active_tab().map(|t| t.id))
            {
                if let Some(handle) = self.procs.get_mut(active_id) {
                    let _ = handle.send(&BrowserToRenderer::KeyPress { ch });
                }
            }
        }
    }

    fn send_scroll(&mut self, delta_y: i32) {
        if let Some(active_id) = self.tabs.active_tab().map(|t| t.id) {
            if let Some(handle) = self.procs.get_mut(active_id) {
                let _ = handle.send(&BrowserToRenderer::Scroll { delta_y });
            }
        }
    }

    fn paint(&mut self) {
        let active_id = self.tabs.active_tab().map(|t| t.id);
        let renderer = active_id.and_then(|id| self.procs.get(id));
        render_browser_frame(
            &mut self.compositor,
            &self.tabs,
            &self.omnibox,
            self.hover,
            self.hover_url.as_deref(),
            renderer,
            self.devtools_open,
        );
    }

    fn invalidate(&self) {
        unsafe { InvalidateRect(self.hwnd, null(), 0) };
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: isize,
    msg: u32,
    w_param: usize,
    l_param: isize,
) -> isize {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut AppState;
    if !ptr.is_null() {
        let state = unsafe { &mut *ptr };
        match msg {
            WM_ERASEBKGND => return 1,
            WM_SETCURSOR => {
                if (l_param & 0xFFFF) as isize == HTCLIENT {
                    state.apply_cursor();
                    return 1;
                }
            }
            WM_SIZE => {
                let w = (l_param & 0xFFFF) as u32;
                let h = ((l_param >> 16) & 0xFFFF) as u32;
                state.handle_resize(w, h);
                return 0;
            }
            WM_MOUSEMOVE => {
                let x = (l_param & 0xFFFF) as i16 as i32;
                let y = ((l_param >> 16) & 0xFFFF) as i16 as i32;
                state.handle_mouse_move(x, y);
                return 0;
            }
            WM_LBUTTONDOWN => {
                let x = (l_param & 0xFFFF) as i16 as i32;
                let y = ((l_param >> 16) & 0xFFFF) as i16 as i32;
                state.handle_mouse_down(x, y);
                return 0;
            }
            WM_MOUSEWHEEL => {
                let wheel_delta = ((w_param >> 16) & 0xFFFF) as i16 as i32;
                state.send_scroll((wheel_delta * 48) / 120);
                return 0;
            }
            WM_KEYDOWN => {
                state.handle_key_down(w_param);
                return 0;
            }
            WM_CHAR => {
                state.handle_char(w_param as u32);
                return 0;
            }
            WM_APP_IPC => {
                state.drain_ipc_events();
                return 0;
            }
            WM_PAINT => {
                state.paint();
                let mut ps = std::mem::MaybeUninit::<PaintStruct>::uninit();
                let hdc = unsafe { BeginPaint(hwnd, ps.as_mut_ptr()) };
                let bmi = BitmapInfo {
                    bmi_header: BitmapInfoHeader {
                        bi_size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                        bi_width: state.compositor.width as i32,
                        bi_height: -(state.compositor.height as i32), // top-down DIB
                        bi_planes: 1,
                        bi_bit_count: 32,
                        bi_compression: BI_RGB,
                        bi_size_image: 0,
                        bi_x_pels_per_meter: 0,
                        bi_y_pels_per_meter: 0,
                        bi_clr_used: 0,
                        bi_clr_important: 0,
                    },
                    bmi_colors: [0],
                };
                unsafe {
                    StretchDIBits(
                        hdc,
                        0,
                        0,
                        state.compositor.width as i32,
                        state.compositor.height as i32,
                        0,
                        0,
                        state.compositor.width as i32,
                        state.compositor.height as i32,
                        state.compositor.pixels.as_ptr(),
                        &bmi,
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    );
                    EndPaint(hwnd, ps.as_ptr());
                }
                return 0;
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                return 0;
            }
            _ => {}
        }
    } else if msg == WM_DESTROY {
        unsafe { PostQuitMessage(0) };
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, w_param, l_param) }
}

pub fn run_browser_window() -> std::io::Result<()> {
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } == 0 {
        unsafe { SetProcessDPIAware() };
    }

    let h_instance = unsafe { GetModuleHandleW(null()) };
    let class_name = to_wide_null("ChromeScratchWindowClass");
    let title = to_wide_null("Chrome Scratch");

    let wc = WndClassExW {
        cb_size: std::mem::size_of::<WndClassExW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfn_wnd_proc: wnd_proc,
        cb_cls_extra: 0,
        cb_wnd_extra: 0,
        h_instance,
        h_icon: 0,
        h_cursor: unsafe { LoadCursorW(0, IDC_ARROW as *const u16) },
        hbr_background: 0,
        lpsz_menu_name: null(),
        lpsz_class_name: class_name.as_ptr(),
        h_icon_sm: 0,
    };
    if unsafe { RegisterClassExW(&wc) } == 0 {
        return Err(std::io::Error::last_os_error());
    }

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1200,
            780,
            0,
            0,
            h_instance,
            null_mut(),
        )
    };
    if hwnd == 0 {
        return Err(std::io::Error::last_os_error());
    }

    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi > 96 {
        let scaled_w = ((1200u64 * dpi as u64) / 96).min(1920) as i32;
        let scaled_h = ((780u64 * dpi as u64) / 96).min(1040) as i32;
        unsafe {
            SetWindowPos(hwnd, 0, 0, 0, scaled_w, scaled_h, SWP_NOMOVE | SWP_NOZORDER);
        }
    }

    let mut rc = RectWin {
        left: 0,
        top: 0,
        right: 1200,
        bottom: 780,
    };
    unsafe { GetClientRect(hwnd, &mut rc) };
    let cw = (rc.right - rc.left).max(1) as u32;
    let ch = (rc.bottom - rc.top).max(1) as u32;

    let (tx, rx) = mpsc::channel();
    let wakeup: Arc<dyn Fn() + Send + Sync> = Arc::new(move || unsafe {
        PostMessageW(hwnd, WM_APP_IPC, 0, 0);
    });

    let tabs = TabManager::new();
    let initial_url = tabs
        .active_tab()
        .map(|t| t.url.clone())
        .unwrap_or_else(|| "chrome://newtab".into());
    let initial_id = tabs.active_tab().map(|t| t.id).unwrap_or(1);

    let mut state = Box::new(AppState {
        hwnd,
        tabs,
        procs: ProcessManager::new(tx, wakeup),
        rx,
        omnibox: OmniboxState::new(&initial_url),
        compositor: OwnedCanvas::new(cw, ch),
        hover: None,
        page_cursor: 0,
        hover_url: None,
        devtools_open: false,
    });
    state.spawn_and_navigate_tab(initial_id, initial_url, false);

    let state_ptr = Box::into_raw(state);
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
    }

    let mut msg = std::mem::MaybeUninit::<Msg>::uninit();
    loop {
        let ret = unsafe { GetMessageW(msg.as_mut_ptr(), 0, 0, 0) };
        if ret <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(msg.as_ptr());
            DispatchMessageW(msg.as_ptr());
        }
    }

    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        drop(Box::from_raw(state_ptr));
    }
    Ok(())
}
