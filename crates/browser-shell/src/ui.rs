use browser_gfx::{
    CanvasView, Color, GLYPH_W, OwnedCanvas, Rect, icons, measure_text,
};

use crate::process::TabRendererHandle;
use crate::tabs::{TabManager, TabState, TabStatus};

pub const TAB_STRIP_H: i32 = 38;
pub const TOOLBAR_H: i32 = 42;
pub const CHROME_TOP_H: i32 = TAB_STRIP_H + TOOLBAR_H;

const STRIP_BG: Color = Color::rgb(222, 225, 230);
const TOOLBAR_BG: Color = Color::rgb(255, 255, 255);
const TAB_INACTIVE: Color = Color::rgb(222, 225, 230);
const TAB_HOVER: Color = Color::rgb(235, 238, 242);
const TAB_ACTIVE: Color = Color::rgb(255, 255, 255);
const OMNIBOX_BG: Color = Color::rgb(241, 243, 244);
const OMNIBOX_FOCUS_BORDER: Color = Color::rgb(26, 115, 232);
const BORDER_SUBTLE: Color = Color::rgb(218, 220, 224);
const TEXT_DARK: Color = Color::rgb(32, 33, 36);
const TEXT_MUTED: Color = Color::rgb(95, 99, 104);
const ICON_DISABLED: Color = Color::rgb(180, 184, 190);
const BTN_HOVER: Color = Color::rgb(236, 239, 243);
const CRASH_BG: Color = Color::rgb(248, 249, 250);
const ACCENT_BLUE: Color = Color::rgb(26, 115, 232);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiAction {
    SelectTab(usize),
    CloseTab(usize),
    NewTab,
    Back,
    Forward,
    Reload,
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
    pub crash_reload_rect: Option<Rect>,
}

impl UiLayout {
    pub fn compute(width: i32, height: i32, tab_count: usize, crashed: bool) -> Self {
        let max_tabs_w = (width - 56).max(120);
        let tab_w = if tab_count > 0 {
            (max_tabs_w / tab_count as i32).clamp(90, 220)
        } else {
            180
        };

        let mut tab_rects = Vec::with_capacity(tab_count);
        let mut close_rects = Vec::with_capacity(tab_count);
        let mut x = 8;
        for _ in 0..tab_count {
            let tr = Rect::new(x, 6, tab_w - 4, TAB_STRIP_H - 6);
            let cr = Rect::new(tr.x + tr.w - 24, tr.y + 8, 16, 16);
            tab_rects.push(tr);
            close_rects.push(cr);
            x += tab_w;
        }

        let new_tab_rect = Rect::new(x + 4, 8, 26, 26);
        let back_rect = Rect::new(8, TAB_STRIP_H + 6, 30, 30);
        let forward_rect = Rect::new(42, TAB_STRIP_H + 6, 30, 30);
        let reload_rect = Rect::new(76, TAB_STRIP_H + 6, 30, 30);
        let omnibox_x = 116;
        let omnibox_w = (width - omnibox_x - 16).max(80);
        let omnibox_rect = Rect::new(omnibox_x, TAB_STRIP_H + 6, omnibox_w, 30);

        let crash_reload_rect = if crashed {
            let vp_h = (height - CHROME_TOP_H).max(100);
            Some(Rect::new(
                (width / 2 - 70).max(40),
                CHROME_TOP_H + vp_h / 2 + 30,
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
            crash_reload_rect,
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
        if self.omnibox_rect.contains(x, y) {
            return Some(UiAction::FocusOmnibox);
        }
        if let Some(cr) = self.crash_reload_rect {
            if cr.contains(x, y) {
                return Some(UiAction::Reload);
            }
        }
        if y >= CHROME_TOP_H {
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
) {
    let w = compositor.width as i32;
    let h = compositor.height as i32;
    let active_tab = tabs.active_tab();
    let is_crashed = matches!(
        active_tab.map(|t| &t.status),
        Some(TabStatus::Crashed { .. })
    );
    let layout = UiLayout::compute(w, h, tabs.tabs().len(), is_crashed);

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
        view.fill_rounded_rect(Rect::new(tr.x, tr.y, tr.w, tr.h + 6), 8, bg);

        let title = if matches!(tab.status, TabStatus::Crashed { .. }) {
            "Aw, Snap!"
        } else {
            tab.title.as_str()
        };
        view.draw_text_clipped(tr.x + 10, tr.y + 9, 2, TEXT_DARK, title, tr.w - 40);

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

    // Omnibox
    view.fill_rounded_rect(layout.omnibox_rect, 15, OMNIBOX_BG);
    if omnibox.focused {
        view.stroke_rounded_rect(layout.omnibox_rect, 15, OMNIBOX_FOCUS_BORDER);
    }
    let text_x = layout.omnibox_rect.x + 14;
    let text_y = layout.omnibox_rect.y + 7;
    let max_text_w = layout.omnibox_rect.w - 28;

    if omnibox.focused && omnibox.select_all && !omnibox.text.is_empty() {
        let (tw, _) = measure_text(2, &omnibox.text);
        let sel_w = tw.min(max_text_w);
        view.fill_rect(
            Rect::new(text_x - 2, text_y - 2, sel_w + 4, 20),
            Color::rgb(180, 213, 254),
        );
    }
    view.draw_text_clipped(text_x, text_y, 2, TEXT_DARK, &omnibox.text, max_text_w);

    if omnibox.focused && !omnibox.select_all {
        let caret_px = text_x + (omnibox.cursor as i32) * (GLYPH_W as i32) * 2;
        if caret_px < layout.omnibox_rect.x + layout.omnibox_rect.w - 10 {
            view.fill_rect(Rect::new(caret_px, text_y - 1, 2, 18), TEXT_DARK);
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
                    view.blit_bgra(0, CHROME_TOP_H, r.width(), r.height(), r.pixels());
                } else {
                    view.fill_rect(
                        Rect::new(0, CHROME_TOP_H, w, (h - CHROME_TOP_H).max(0)),
                        Color::rgb(255, 255, 255),
                    );
                }
            }
        }
    }
}

fn draw_toolbar_button(
    view: &mut CanvasView<'_>,
    rect: Rect,
    hover: Option<(i32, i32)>,
    enabled: bool,
) {
    if enabled && hover.is_some_and(|(hx, hy)| rect.contains(hx, hy)) {
        view.fill_circle(rect.x + rect.w / 2, rect.y + rect.h / 2, rect.w / 2, BTN_HOVER);
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
    let (tw, _) = measure_text(3, title);
    view.draw_text(
        ((w - tw) / 2).max(24),
        CHROME_TOP_H + vp_h / 2 - 56,
        3,
        TEXT_DARK,
        title,
    );

    let msg = match exit_code {
        Some(code) => format!("Renderer process terminated unexpectedly (exit code {code})."),
        None => "Renderer process terminated unexpectedly. The browser shell stayed safe.".into(),
    };
    let (mw, _) = measure_text(2, &msg);
    view.draw_text_clipped(
        ((w - mw) / 2).max(24),
        CHROME_TOP_H + vp_h / 2 - 12,
        2,
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
        view.fill_rounded_rect(btn, 6, col);
        let label = "Reload";
        let (lw, _) = measure_text(2, label);
        view.draw_text(
            btn.x + (btn.w - lw) / 2,
            btn.y + 10,
            2,
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
