use browser_gfx::{
    CanvasView, Color, FontStyle, OwnedCanvas, Rect, icons, measure_text_styled,
};

use crate::process::TabRendererHandle;
use crate::tabs::{TabManager, TabState, TabStatus};

pub const TAB_STRIP_H: i32 = 40;
pub const TOOLBAR_H: i32 = 44;
pub const CHROME_TOP_H: i32 = TAB_STRIP_H + TOOLBAR_H;
pub const DEVTOOLS_H: i32 = 200;

const STRIP_BG: Color = Color::rgb(223, 229, 239);
const TOOLBAR_BG: Color = Color::rgb(255, 255, 255);
const TAB_INACTIVE: Color = Color::rgb(223, 229, 239);
const TAB_HOVER: Color = Color::rgb(236, 241, 248);
const TAB_ACTIVE: Color = Color::rgb(255, 255, 255);
const TAB_DIVIDER: Color = Color::rgb(196, 205, 218);
const OMNIBOX_BG: Color = Color::rgb(237, 242, 250);
const OMNIBOX_FOCUS_BG: Color = Color::rgb(255, 255, 255);
const OMNIBOX_FOCUS_BORDER: Color = Color::rgb(26, 115, 232);
const BORDER_SUBTLE: Color = Color::rgb(222, 226, 230);
const TEXT_DARK: Color = Color::rgb(32, 33, 36);
const TEXT_MUTED: Color = Color::rgb(95, 99, 104);
const ICON_DISABLED: Color = Color::rgb(189, 193, 198);
const BTN_HOVER: Color = Color::rgb(235, 240, 248);
const CRASH_BG: Color = Color::rgb(248, 249, 250);
const ACCENT_BLUE: Color = Color::rgb(26, 115, 232);
const SECURE_GREEN: Color = Color::rgb(24, 128, 56);

const TAB_FONT: FontStyle = FontStyle::new(13, false, false);
const TAB_FONT_ACTIVE: FontStyle = FontStyle::new(13, true, false);
const OMNIBOX_FONT: FontStyle = FontStyle::new(14, false, false);
const CHIP_FONT: FontStyle = FontStyle::new(12, true, false);
const H1_FONT: FontStyle = FontStyle::new(26, true, false);
const BODY_FONT: FontStyle = FontStyle::new(14, false, false);
const MONO_FONT: FontStyle = FontStyle::new(12, false, true);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    SelectTab(usize),
    CloseTab(usize),
    NewTab,
    Back,
    Forward,
    Reload,
    ToggleDevTools,
    FocusOmnibox,
    ViewportClick { x: i32, y: i32 },
}

pub struct OmniboxState {
    pub text: String,
    pub cursor: usize,
    pub focused: bool,
    pub select_all: bool,
}

impl OmniboxState {
    pub fn new(initial: &str) -> Self {
        let len = initial.chars().count();
        Self {
            text: initial.to_string(),
            cursor: len,
            focused: false,
            select_all: false,
        }
    }

    pub fn set_text(&mut self, s: &str) {
        self.text = s.to_string();
        self.cursor = self.text.chars().count();
        self.select_all = false;
    }

    pub fn focus_and_select_all(&mut self) {
        self.focused = true;
        self.select_all = true;
        self.cursor = self.text.chars().count();
    }

    pub fn insert_char(&mut self, ch: char) {
        if self.select_all {
            self.text.clear();
            self.cursor = 0;
            self.select_all = false;
        }
        let byte_idx = char_to_byte_idx(&self.text, self.cursor);
        self.text.insert(byte_idx, ch);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.select_all {
            self.text.clear();
            self.cursor = 0;
            self.select_all = false;
            return;
        }
        if self.cursor > 0 {
            let start = char_to_byte_idx(&self.text, self.cursor - 1);
            let end = char_to_byte_idx(&self.text, self.cursor);
            self.text.replace_range(start..end, "");
            self.cursor -= 1;
        }
    }

    pub fn move_left(&mut self) {
        self.select_all = false;
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.select_all = false;
        let max = self.text.chars().count();
        if self.cursor < max {
            self.cursor += 1;
        }
    }
}

fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

pub struct UiLayout {
    pub width: i32,
    pub height: i32,
    pub tab_rects: Vec<Rect>,
    pub close_rects: Vec<Rect>,
    pub new_tab_rect: Rect,
    pub back_rect: Rect,
    pub forward_rect: Rect,
    pub reload_rect: Rect,
    pub omnibox_rect: Rect,
    pub devtools_rect: Rect,
    pub crash_reload_rect: Option<Rect>,
    pub viewport_h: i32,
}

impl UiLayout {
    pub fn compute(
        width: i32,
        height: i32,
        tab_count: usize,
        crashed: bool,
        devtools_open: bool,
    ) -> Self {
        let max_tabs_w = (width - 64).max(120);
        let tab_w = if tab_count > 0 {
            (max_tabs_w / tab_count as i32).clamp(100, 232)
        } else {
            190
        };

        let mut tab_rects = Vec::with_capacity(tab_count);
        let mut close_rects = Vec::with_capacity(tab_count);
        let mut x = 8;
        for _ in 0..tab_count {
            let tr = Rect::new(x, 6, tab_w - 4, TAB_STRIP_H - 6);
            let cr = Rect::new(tr.x + tr.w - 24, tr.y + 9, 16, 16);
            tab_rects.push(tr);
            close_rects.push(cr);
            x += tab_w;
        }

        let new_tab_rect = Rect::new(x + 4, 8, 26, 26);
        let btn_y = TAB_STRIP_H + 7;
        let back_rect = Rect::new(8, btn_y, 30, 30);
        let forward_rect = Rect::new(40, btn_y, 30, 30);
        let reload_rect = Rect::new(72, btn_y, 30, 30);
        let devtools_w = 88;
        let devtools_rect = Rect::new((width - devtools_w - 10).max(200), btn_y, devtools_w, 30);
        let omnibox_x = 112;
        let omnibox_w = (devtools_rect.x - omnibox_x - 10).max(80);
        let omnibox_rect = Rect::new(omnibox_x, TAB_STRIP_H + 6, omnibox_w, 32);

        let drawer_h = if devtools_open { DEVTOOLS_H } else { 0 };
        let viewport_h = (height - CHROME_TOP_H - drawer_h).max(80);

        let crash_reload_rect = if crashed {
            Some(Rect::new(
                (width / 2 - 70).max(40),
                CHROME_TOP_H + viewport_h / 2 + 34,
                140,
                36,
            ))
        } else {
            None
        };

        Self {
            width,
            height,
            tab_rects,
            close_rects,
            new_tab_rect,
            back_rect,
            forward_rect,
            reload_rect,
            omnibox_rect,
            devtools_rect,
            crash_reload_rect,
            viewport_h,
        }
    }

    pub fn hit_test(&self, x: i32, y: i32) -> Option<UiAction> {
        for (i, cr) in self.close_rects.iter().enumerate() {
            if cr.contains(x, y) {
                return Some(UiAction::CloseTab(i));
            }
        }
        for (i, tr) in self.tab_rects.iter().enumerate() {
            if tr.contains(x, y) {
                return Some(UiAction::SelectTab(i));
            }
        }
        if self.new_tab_rect.contains(x, y) {
            return Some(UiAction::NewTab);
        }
        if self.back_rect.contains(x, y) {
            return Some(UiAction::Back);
        }
        if self.forward_rect.contains(x, y) {
            return Some(UiAction::Forward);
        }
        if self.reload_rect.contains(x, y) {
            return Some(UiAction::Reload);
        }
        if self.devtools_rect.contains(x, y) {
            return Some(UiAction::ToggleDevTools);
        }
        if self.omnibox_rect.contains(x, y) {
            return Some(UiAction::FocusOmnibox);
        }
        if let Some(cr) = self.crash_reload_rect {
            if cr.contains(x, y) {
                return Some(UiAction::Reload);
            }
        }
        if y >= CHROME_TOP_H && y < CHROME_TOP_H + self.viewport_h {
            return Some(UiAction::ViewportClick {
                x,
                y: y - CHROME_TOP_H,
            });
        }
        None
    }
}

pub fn render_browser_frame(
    compositor: &mut OwnedCanvas,
    tabs: &TabManager,
    omnibox: &OmniboxState,
    hover: Option<(i32, i32)>,
    renderer: Option<&TabRendererHandle>,
    devtools_open: bool,
) {
    let w = compositor.width as i32;
    let h = compositor.height as i32;
    let active_tab = tabs.active_tab();
    let is_crashed = matches!(
        active_tab.map(|t| &t.status),
        Some(TabStatus::Crashed { .. })
    );
    let layout = UiLayout::compute(w, h, tabs.tabs().len(), is_crashed, devtools_open);

    let mut view = compositor.view_mut();

    // Tab strip background
    view.fill_rect(Rect::new(0, 0, w, TAB_STRIP_H), STRIP_BG);

    // Draw tabs
    for (idx, tab) in tabs.tabs().iter().enumerate() {
        let tr = layout.tab_rects[idx];
        let cr = layout.close_rects[idx];
        let is_active = idx == tabs.active_index();
        let is_hovered = hover.is_some_and(|(hx, hy)| tr.contains(hx, hy));

        let bg = if is_active {
            TAB_ACTIVE
        } else if is_hovered {
            TAB_HOVER
        } else {
            TAB_INACTIVE
        };
        if is_active || is_hovered {
            view.fill_rounded_rect(Rect::new(tr.x, tr.y, tr.w, tr.h + 10), 10, bg);
        } else if idx + 1 < tabs.tabs().len() && idx + 1 != tabs.active_index() {
            view.fill_rect(Rect::new(tr.x + tr.w + 1, tr.y + 8, 1, 16), TAB_DIVIDER);
        }

        // Favicon pill dot
        let fav_col = if matches!(tab.status, TabStatus::Crashed { .. }) {
            Color::rgb(217, 48, 37)
        } else if matches!(tab.status, TabStatus::Loading) {
            Color::rgb(251, 188, 5)
        } else if tab.url.starts_with("https://") {
            ACCENT_BLUE
        } else {
            Color::rgb(95, 99, 104)
        };
        view.fill_circle(tr.x + 15, tr.y + 17, 5, fav_col);

        let title = if matches!(tab.status, TabStatus::Crashed { .. }) {
            "Aw, Snap!"
        } else {
            tab.title.as_str()
        };
        let tfont = if is_active { TAB_FONT_ACTIVE } else { TAB_FONT };
        view.draw_text_clipped_styled(
            tr.x + 26,
            tr.y + 8,
            tfont,
            TEXT_DARK,
            title,
            (tr.w - 54).max(24),
        );

        let close_hover = hover.is_some_and(|(hx, hy)| cr.contains(hx, hy));
        if close_hover {
            view.fill_circle(cr.x + 8, cr.y + 8, 8, BTN_HOVER);
        }
        icons::draw_close_icon(&mut view, cr.x + 8, cr.y + 8, TEXT_MUTED);
    }

    // New Tab (+) button
    if hover.is_some_and(|(hx, hy)| layout.new_tab_rect.contains(hx, hy)) {
        view.fill_circle(
            layout.new_tab_rect.x + 13,
            layout.new_tab_rect.y + 13,
            13,
            TAB_HOVER,
        );
    }
    icons::draw_plus_icon(
        &mut view,
        layout.new_tab_rect.x + 13,
        layout.new_tab_rect.y + 13,
        TEXT_DARK,
    );

    // Toolbar background + bottom hairline
    view.fill_rect(Rect::new(0, TAB_STRIP_H, w, TOOLBAR_H), TOOLBAR_BG);
    view.fill_rect(Rect::new(0, CHROME_TOP_H - 1, w, 1), BORDER_SUBTLE);

    // Nav buttons
    let can_back = active_tab.is_some_and(|t| t.history.can_back());
    let can_fwd = active_tab.is_some_and(|t| t.history.can_forward());

    draw_toolbar_button(&mut view, layout.back_rect, hover, can_back);
    icons::draw_back_arrow(
        &mut view,
        layout.back_rect.x + 15,
        layout.back_rect.y + 15,
        if can_back { TEXT_DARK } else { ICON_DISABLED },
    );

    draw_toolbar_button(&mut view, layout.forward_rect, hover, can_fwd);
    icons::draw_forward_arrow(
        &mut view,
        layout.forward_rect.x + 15,
        layout.forward_rect.y + 15,
        if can_fwd { TEXT_DARK } else { ICON_DISABLED },
    );

    draw_toolbar_button(&mut view, layout.reload_rect, hover, true);
    icons::draw_reload_icon(
        &mut view,
        layout.reload_rect.x + 15,
        layout.reload_rect.y + 15,
        TEXT_DARK,
    );

    // DevTools chip
    let dt_bg = if devtools_open {
        Color::rgb(211, 227, 253)
    } else if hover.is_some_and(|(hx, hy)| layout.devtools_rect.contains(hx, hy)) {
        BTN_HOVER
    } else {
        OMNIBOX_BG
    };
    view.fill_rounded_rect(layout.devtools_rect, 15, dt_bg);
    let (dw, dh) = measure_text_styled(CHIP_FONT, "DevTools");
    view.draw_text_styled(
        layout.devtools_rect.x + (layout.devtools_rect.w - dw) / 2,
        layout.devtools_rect.y + (layout.devtools_rect.h - dh) / 2,
        CHIP_FONT,
        if devtools_open { ACCENT_BLUE } else { TEXT_DARK },
        "DevTools",
    );

    // Pill Omnibox
    let ob_bg = if omnibox.focused {
        OMNIBOX_FOCUS_BG
    } else {
        OMNIBOX_BG
    };
    view.fill_rounded_rect(layout.omnibox_rect, 16, ob_bg);
    if omnibox.focused {
        view.stroke_rounded_rect(layout.omnibox_rect, 16, OMNIBOX_FOCUS_BORDER);
    }

    // Security / Route indicator badge inside left side of omnibox
    let active_url = active_tab.map(|t| t.url.as_str()).unwrap_or("");
    let badge_col = if active_url.starts_with("https://") {
        SECURE_GREEN
    } else if active_url.starts_with("chrome://") {
        ACCENT_BLUE
    } else {
        TEXT_MUTED
    };
    let bx = layout.omnibox_rect.x + 15;
    let by = layout.omnibox_rect.y + 16;
    view.fill_circle(bx, by, 7, Color::rgb(255, 255, 255));
    view.fill_circle(bx, by, 5, badge_col);

    let text_x = layout.omnibox_rect.x + 30;
    let (_, oh) = measure_text_styled(OMNIBOX_FONT, "Ay");
    let text_y = layout.omnibox_rect.y + (layout.omnibox_rect.h - oh) / 2;
    let max_text_w = (layout.omnibox_rect.w - 44).max(20);

    if omnibox.focused && omnibox.select_all && !omnibox.text.is_empty() {
        let (tw, _) = measure_text_styled(OMNIBOX_FONT, &omnibox.text);
        let sel_w = tw.min(max_text_w);
        view.fill_rect(
            Rect::new(text_x - 2, text_y, sel_w + 4, oh),
            Color::rgb(180, 213, 254),
        );
    }
    view.draw_text_clipped_styled(
        text_x,
        text_y,
        OMNIBOX_FONT,
        TEXT_DARK,
        &omnibox.text,
        max_text_w,
    );

    if omnibox.focused && !omnibox.select_all {
        let prefix_end = char_to_byte_idx(&omnibox.text, omnibox.cursor);
        let (caret_off, _) = measure_text_styled(OMNIBOX_FONT, &omnibox.text[..prefix_end]);
        let caret_px = text_x + caret_off;
        if caret_px < layout.omnibox_rect.x + layout.omnibox_rect.w - 10 {
            view.fill_rect(Rect::new(caret_px, text_y, 2, oh), ACCENT_BLUE);
        }
    }

    // Viewport area
    if let Some(tab) = active_tab {
        match &tab.status {
            TabStatus::Crashed { exit_code } => {
                render_crash_screen(&mut view, w, h, *exit_code, layout.crash_reload_rect, hover);
            }
            TabStatus::Loading | TabStatus::Ready => {
                if let Some(r) = renderer {
                    let blit_h = r.height().min(layout.viewport_h as u32);
                    view.blit_bgra(0, CHROME_TOP_H, r.width(), blit_h, r.pixels());
                } else {
                    view.fill_rect(
                        Rect::new(0, CHROME_TOP_H, w, layout.viewport_h),
                        Color::rgb(255, 255, 255),
                    );
                }
            }
        }
        if devtools_open {
            render_devtools_drawer(&mut view, w, CHROME_TOP_H + layout.viewport_h, h, tab);
        }
    }
}

fn render_devtools_drawer(
    view: &mut CanvasView<'_>,
    w: i32,
    top_y: i32,
    h: i32,
    tab: &TabState,
) {
    let drawer_h = (h - top_y).max(0);
    if drawer_h <= 0 {
        return;
    }
    view.fill_rect(Rect::new(0, top_y, w, drawer_h), Color::rgb(32, 33, 36));
    view.fill_rect(Rect::new(0, top_y, w, 2), ACCENT_BLUE);
    view.draw_text_styled(
        14,
        top_y + 8,
        CHIP_FONT,
        Color::rgb(138, 180, 248),
        "DevTools  •  Elements & Console (F12 to close)",
    );

    let half_w = (w / 2).max(120);
    view.fill_rect(
        Rect::new(half_w, top_y + 28, 1, drawer_h - 28),
        Color::rgb(60, 64, 67),
    );

    if let Some(dt) = &tab.devtools {
        view.draw_text_clipped_styled(
            14,
            top_y + 30,
            MONO_FONT,
            Color::rgb(129, 201, 149),
            &dt.network_summary,
            half_w - 24,
        );
        let mut y = top_y + 50;
        for line in dt.dom_dump.lines().take(9) {
            view.draw_text_clipped_styled(
                14,
                y,
                MONO_FONT,
                Color::rgb(232, 234, 237),
                line,
                half_w - 24,
            );
            y += 16;
        }
        view.draw_text_styled(
            half_w + 14,
            top_y + 30,
            CHIP_FONT,
            Color::rgb(253, 214, 99),
            "Console:",
        );
        let mut cy = top_y + 50;
        for line in dt.console_logs.lines().take(9) {
            view.draw_text_clipped_styled(
                half_w + 14,
                cy,
                MONO_FONT,
                Color::rgb(232, 234, 237),
                line,
                (w - half_w - 26).max(60),
            );
            cy += 16;
        }
    } else {
        view.draw_text_styled(
            14,
            top_y + 34,
            MONO_FONT,
            Color::rgb(154, 160, 166),
            "Waiting for document telemetry...",
        );
    }
}

fn draw_toolbar_button(
    view: &mut CanvasView<'_>,
    rect: Rect,
    hover: Option<(i32, i32)>,
    enabled: bool,
) {
    if enabled && hover.is_some_and(|(hx, hy)| rect.contains(hx, hy)) {
        view.fill_circle(
            rect.x + rect.w / 2,
            rect.y + rect.h / 2,
            rect.w / 2,
            BTN_HOVER,
        );
    }
}

fn render_crash_screen(
    view: &mut CanvasView<'_>,
    w: i32,
    h: i32,
    exit_code: Option<i32>,
    reload_rect: Option<Rect>,
    hover: Option<(i32, i32)>,
) {
    let vp_h = (h - CHROME_TOP_H).max(100);
    view.fill_rect(Rect::new(0, CHROME_TOP_H, w, vp_h), CRASH_BG);

    let title = "Aw, Snap!";
    let (tw, _) = measure_text_styled(H1_FONT, title);
    view.draw_text_styled(
        ((w - tw) / 2).max(24),
        CHROME_TOP_H + vp_h / 2 - 58,
        H1_FONT,
        TEXT_DARK,
        title,
    );

    let msg = match exit_code {
        Some(code) => format!("Renderer process terminated unexpectedly (exit code {code})."),
        None => "Renderer process terminated unexpectedly. The browser shell stayed safe.".into(),
    };
    let (mw, _) = measure_text_styled(BODY_FONT, &msg);
    view.draw_text_clipped_styled(
        ((w - mw) / 2).max(24),
        CHROME_TOP_H + vp_h / 2 - 14,
        BODY_FONT,
        TEXT_MUTED,
        &msg,
        (w - 48).max(100),
    );

    if let Some(btn) = reload_rect {
        let hovered = hover.is_some_and(|(hx, hy)| btn.contains(hx, hy));
        let col = if hovered {
            Color::rgb(23, 98, 204)
        } else {
            ACCENT_BLUE
        };
        view.fill_rounded_rect(btn, 18, col);
        let label = "Reload";
        let (lw, lh) = measure_text_styled(CHIP_FONT, label);
        view.draw_text_styled(
            btn.x + (btn.w - lw) / 2,
            btn.y + (btn.h - lh) / 2,
            CHIP_FONT,
            Color::rgb(255, 255, 255),
            label,
        );
    }
}

pub fn sync_omnibox_with_tab(omnibox: &mut OmniboxState, tab: &TabState) {
    if !omnibox.focused {
        omnibox.set_text(&tab.url);
    }
}
