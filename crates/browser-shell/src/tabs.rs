#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabStatus {
    Loading,
    Ready,
    Crashed { exit_code: Option<i32> },
}

#[derive(Debug, Clone)]
pub struct TabHistory {
    entries: Vec<String>,
    cursor: usize,
}

impl TabHistory {
    pub fn new(initial_url: String) -> Self {
        Self {
            entries: vec![initial_url],
            cursor: 0,
        }
    }

    pub fn current(&self) -> &str {
        &self.entries[self.cursor]
    }

    pub fn can_back(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_forward(&self) -> bool {
        self.cursor + 1 < self.entries.len()
    }

    pub fn push(&mut self, url: String) {
        if self.entries[self.cursor] == url {
            return;
        }
        self.entries.truncate(self.cursor + 1);
        self.entries.push(url);
        self.cursor += 1;
    }

    pub fn back(&mut self) -> Option<&str> {
        if self.can_back() {
            self.cursor -= 1;
            Some(&self.entries[self.cursor])
        } else {
            None
        }
    }

    pub fn forward(&mut self) -> Option<&str> {
        if self.can_forward() {
            self.cursor += 1;
            Some(&self.entries[self.cursor])
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct DevToolsSnapshot {
    pub network_summary: String,
    pub dom_dump: String,
    pub console_logs: String,
}

#[derive(Debug, Clone)]
pub struct TabState {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub history: TabHistory,
    pub status: TabStatus,
    pub renderer_pid: Option<u32>,
    pub active_nav_id: u64,
    pub devtools: Option<DevToolsSnapshot>,
}

pub struct TabManager {
    tabs: Vec<TabState>,
    active_index: usize,
    next_tab_id: u64,
    next_nav_id: u64,
}

impl TabManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            tabs: Vec::new(),
            active_index: 0,
            next_tab_id: 1,
            next_nav_id: 1,
        };
        mgr.open_tab("chrome://newtab".into());
        mgr
    }

    pub fn tabs(&self) -> &[TabState] {
        &self.tabs
    }

    pub fn active_index(&self) -> usize {
        self.active_index
    }

    pub fn active_tab(&self) -> Option<&TabState> {
        self.tabs.get(self.active_index)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut TabState> {
        self.tabs.get_mut(self.active_index)
    }

    pub fn find_by_id_mut(&mut self, id: u64) -> Option<&mut TabState> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn find_by_id(&self, id: u64) -> Option<&TabState> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn open_tab(&mut self, url: String) -> u64 {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let nav_id = self.alloc_nav_id();
        self.tabs.push(TabState {
            id,
            title: "New Tab".into(),
            url: url.clone(),
            history: TabHistory::new(url),
            status: TabStatus::Loading,
            renderer_pid: None,
            active_nav_id: nav_id,
            devtools: None,
        });
        self.active_index = self.tabs.len() - 1;
        id
    }

    pub fn close_tab(&mut self, index: usize) -> Option<u64> {
        if index >= self.tabs.len() {
            return None;
        }
        let removed = self.tabs.remove(index);
        if !self.tabs.is_empty() && self.active_index >= self.tabs.len() {
            self.active_index = self.tabs.len() - 1;
        }
        Some(removed.id)
    }

    pub fn select_tab(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.active_index = index;
            true
        } else {
            false
        }
    }

    pub fn alloc_nav_id(&mut self) -> u64 {
        let id = self.next_nav_id;
        self.next_nav_id += 1;
        id
    }
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalizes user omnibox input into a canonical URL or internal `chrome://` route.
pub fn normalize_omnibox_input(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "chrome://newtab".into();
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("chrome://")
        || lower.starts_with("about:")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
    {
        return trimmed.to_string();
    }
    if !trimmed.contains(' ') && (trimmed.contains('.') || lower.starts_with("localhost")) {
        return format!("https://{trimmed}");
    }
    format!("chrome://search?q={}", trimmed.replace(' ', "+"))
}
