use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect, Margin},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap, Table, Row, Cell, TableState, Gauge},
    text::{Line, Span},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, HashMap},
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime},
};
use unicode_width::UnicodeWidthStr;
use chrono::{DateTime, Local};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[derive(Serialize, Deserialize, Clone)]
struct ColorConfig {
    bg: String,
    text: String,
    dim: String,
    teal: String,
    green: String,
    grey: String,
    accent: String,
    hint_bg: String,
    error: String,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            bg: "#141617".to_string(),
            text: "#8C9696".to_string(),
            dim: "#464B4B".to_string(),
            teal: "#236E6E".to_string(),
            green: "#466E55".to_string(),
            grey: "#646E6E".to_string(),
            accent: "#50A091".to_string(),
            hint_bg: "#1E2D2D".to_string(),
            error: "#A04646".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Config {
    settings: SettingsConfig,
    paths: PathsConfig,
    colors: ColorConfig,
    binds: HashMap<String, String>,
    open_with: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SettingsConfig {
    pane_split_percent: u16,
    notification_duration_secs: u64,
    search_debounce_ms: u64,
}

#[derive(Serialize, Deserialize, Clone)]
struct PathsConfig {
    scripts_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        let mut binds = HashMap::new();
        binds.insert("quit".to_string(), "q".to_string());
        binds.insert("open_ide".to_string(), "ctrl+w".to_string());
        binds.insert("preview_popup".to_string(), "ctrl+o".to_string());
        binds.insert("create_file".to_string(), "ctrl+n".to_string());
        binds.insert("create_folder".to_string(), "ctrl+z".to_string());
        binds.insert("rename".to_string(), "ctrl+r".to_string());
        binds.insert("delete".to_string(), "delete".to_string());
        binds.insert("search".to_string(), "ctrl+f".to_string());
        binds.insert("quick_search".to_string(), "/".to_string());
        binds.insert("zoxide".to_string(), "ctrl+j".to_string());
        binds.insert("shell".to_string(), "ctrl+t".to_string());
        binds.insert("archive".to_string(), "ctrl+b".to_string());
        binds.insert("help".to_string(), "?".to_string());
        binds.insert("open_scripts".to_string(), "f1".to_string());
        binds.insert("open_with".to_string(), "f2".to_string());
        binds.insert("open_term".to_string(), "f3".to_string());
        binds.insert("add_bookmark".to_string(), "f6".to_string());
        binds.insert("drives".to_string(), "f5".to_string());
        binds.insert("bookmarks".to_string(), "ctrl+d".to_string());
        binds.insert("toggle_hidden".to_string(), ".".to_string());
        binds.insert("sync_panes".to_string(), "ctrl+p".to_string());
        
        if cfg!(target_os = "macos") {
             binds.insert("copy".to_string(), "super+c".to_string());
             binds.insert("cut".to_string(), "super+x".to_string());
             binds.insert("paste".to_string(), "super+v".to_string());
             binds.insert("copy_path".to_string(), "super+e".to_string());
        } else {
             binds.insert("copy".to_string(), "ctrl+c".to_string());
             binds.insert("cut".to_string(), "ctrl+x".to_string());
             binds.insert("paste".to_string(), "ctrl+v".to_string());
             binds.insert("copy_path".to_string(), "ctrl+e".to_string());
        }
        binds.insert("info".to_string(), "i".to_string());
        binds.insert("select_all".to_string(), "ctrl+a".to_string());
        binds.insert("sort".to_string(), "ctrl+s".to_string());
        binds.insert("toggle_selection".to_string(), "space".to_string());
        binds.insert("switch_pane".to_string(), "tab".to_string());
        binds.insert("refresh".to_string(), "ctrl+l".to_string());

        let mut open_with = HashMap::new();
        open_with.insert("󰀻 Editor (Micro)".to_string(), "micro".to_string());
        open_with.insert("󰀻 Editor (VS Code)".to_string(), "code".to_string());
        open_with.insert("󰀻 Image Viewer (Preview)".to_string(), "open -a Preview".to_string());
        open_with.insert("󰀻 Browser (Zen)".to_string(), "zen-browser".to_string());
        open_with.insert("󰀻 Browser (Safari)".to_string(), "open -a Safari".to_string());
        open_with.insert("󰀻 Browser (Chrome)".to_string(), "open -a 'Google Chrome'".to_string());
        open_with.insert("󰀻 Media (IINA)".to_string(), "open -a IINA".to_string());
        open_with.insert("󰀻 Terminal (Kitty)".to_string(), "open -a kitty".to_string());

        Self {
            settings: SettingsConfig {
                pane_split_percent: 50,
                notification_duration_secs: 3,
                search_debounce_ms: 300,
            },
            paths: PathsConfig {
                scripts_dir: "~/kaf".to_string(),
            },
            colors: ColorConfig::default(),
            binds,
            open_with,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SortMode {
    Name,
    Extension,
    Size,
    Date,
}

impl SortMode {
    fn to_str(&self) -> &str {
        match self {
            SortMode::Name => "󰗚 Name",
            SortMode::Extension => "󰉓 Ext",
            SortMode::Size => "󰈄 Size",
            SortMode::Date => "󰃭 Date",
        }
    }
}

#[derive(PartialEq)]
enum InputMode {
    None,
    CreateFile,
    CreateFolder,
    DeleteConfirm,
    CustomScripts,
    FileInfo,
    Help,
    OpenWith,
    MicroMenu,
    QuickSearch,
    Rename,
    PreviewPopup,
    IdeSaveConfirm,
    IdePromptFileName,
    Bookmarks,
    Drives,
}

#[derive(PartialEq)]
enum IdeFocus {
    ProjectTree,
    Editor,
}

struct History {
    past: Vec<PathBuf>,
    future: Vec<PathBuf>,
}

impl History {
    fn new() -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
        }
    }

    fn push(&mut self, path: PathBuf) {
        if let Some(last) = self.past.last() {
            if last == &path {
                return;
            }
        }
        self.past.push(path);
        self.future.clear();
    }
}

struct TaskProgress {
    message: String,
    percentage: u16,
    active: bool,
}

struct Pane {
    current_dir: PathBuf,
    items: Vec<PathBuf>,
    state: TableState,
    history: History,
    sort_mode: SortMode,
    offset: usize,
    selected_items: HashSet<PathBuf>,
    anchor_index: Option<usize>,
    show_hidden: bool,
}

fn get_visible_slice(text: &str, start_width: usize, max_width: usize) -> String {
    let mut current_width = 0;
    let mut result = String::new();
    for c in text.chars() {
        let w = UnicodeWidthStr::width(c.to_string().as_str());
        if current_width >= start_width && current_width + w <= start_width + max_width {
            result.push(c);
        } else if current_width + w > start_width + max_width {
            break;
        }
        current_width += w;
    }
    result
}

impl Pane {
    fn new(path: PathBuf) -> Self {
        let mut pane = Self {
            current_dir: path,
            items: Vec::new(),
            state: TableState::default(),
            history: History::new(),
            sort_mode: SortMode::Name,
            offset: 0,
            selected_items: HashSet::new(),
            anchor_index: None,
            show_hidden: false,
        };
        pane.refresh();
        pane
    }

    fn refresh(&mut self) {
        let ignored_files = [".ignore", "wget-hsts", ".bash_history", ".python_history"];
        if let Ok(entries) = fs::read_dir(&self.current_dir) {
            let mut paths: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !self.show_hidden && name.starts_with('.') {
                        return false;
                    }
                    !ignored_files.contains(&name) && !name.starts_with(".fuse_hidden")
                })
                .collect();

            paths.sort_by(|a, b| {
                let a_is_dir = a.is_dir();
                let b_is_dir = b.is_dir();
                if a_is_dir != b_is_dir {
                    return b_is_dir.cmp(&a_is_dir);
                }
                match self.sort_mode {
                    SortMode::Name => a.file_name().cmp(&b.file_name()),
                    SortMode::Extension => a
                        .extension()
                        .unwrap_or_default()
                        .cmp(b.extension().unwrap_or_default()),
                    SortMode::Size => {
                        let a_s = if a.is_file() { fs::metadata(a).map(|m| m.len()).unwrap_or(0) } else { 0 };
                        let b_s = if b.is_file() { fs::metadata(b).map(|m| m.len()).unwrap_or(0) } else { 0 };
                        a_s.cmp(&b_s)
                    }
                    SortMode::Date => {
                        let a_t = fs::metadata(a).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
                        let b_t = fs::metadata(b).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
                        b_t.cmp(&a_t)
                    }
                }
            });
            self.items = paths;
        }
        
        if self.items.is_empty() {
            self.state.select(None);
        } else {
            let selected = self.state.selected().map(|i| i.min(self.items.len().saturating_sub(1))).unwrap_or(0);
            self.state.select(Some(selected));
        }
    }

    fn toggle_selection(&mut self) {
        if let Some(i) = self.state.selected() {
            if let Some(path) = self.items.get(i) {
                if self.selected_items.contains(path) {
                    self.selected_items.remove(path);
                } else {
                    self.selected_items.insert(path.clone());
                }
            }
        }
    }

    fn update_shift_selection(&mut self, current_idx: usize) {
        if let Some(anchor) = self.anchor_index {
            self.selected_items.clear();
            let (start, end) = if anchor < current_idx { (anchor, current_idx) } else { (current_idx, anchor) };
            for i in start..=end {
                if let Some(item) = self.items.get(i) {
                    self.selected_items.insert(item.clone());
                }
            }
        }
    }

    fn select_all_toggle(&mut self) {
        if self.selected_items.is_empty() {
            for item in &self.items {
                self.selected_items.insert(item.clone());
            }
        } else {
            self.selected_items.clear();
        }
    }

    fn get_targets(&self) -> Vec<PathBuf> {
        if self.selected_items.is_empty() {
            if let Some(i) = self.state.selected() {
                if let Some(p) = self.items.get(i) {
                    return vec![p.clone()];
                }
            }
            return vec![];
        }
        self.selected_items.iter().cloned().collect()
    }
}

struct App {
    left_pane: Pane,
    right_pane: Pane,
    active_left: bool,
    search_results: Arc<Mutex<Vec<PathBuf>>>,
    search_state: ListState,
    search_query: String,
    is_searching: bool,
    is_loading: Arc<Mutex<bool>>,
    zoxide_mode: bool,
    is_shell_mode: bool,
    shell_input: String,
    shell_suggestion: String,
    shell_output: Arc<Mutex<Vec<String>>>,
    shell_scroll: u16,
    shell_history: Vec<String>,
    shell_history_index: usize,
    shell_cmd_valid: bool,
    clipboard_items: Vec<PathBuf>,
    is_cut: bool,
    notification: String,
    notification_time: Option<Instant>,
    input_mode: InputMode,
    text_input: String,
    input_cursor_pos: usize,
    custom_scripts: Vec<PathBuf>,
    scripts_state: ListState,
    pane_split_percent: u16,
    preview_content: String,
    preview_scroll: u16,
    last_preview_path: Option<PathBuf>,
    last_preview_rect: Rect,
    file_info_data: Vec<String>,
    open_with_apps: Vec<(String, String)>,
    open_with_state: ListState,
    task_progress: Arc<Mutex<TaskProgress>>,
    micro_project_tree: Vec<PathBuf>,
    micro_project_state: ListState,
    micro_editor_text: Vec<String>,
    micro_undo_stack: Vec<Vec<String>>,
    micro_cursor_y: usize,
    micro_cursor_x: usize,
    micro_scroll_y: usize,
    micro_scroll_x: usize,
    micro_select_start: Option<(usize, usize)>,
    micro_suggestions: Vec<String>,
    micro_suggestion_idx: usize,
    micro_show_autocomplete: bool,
    micro_rects: Vec<Rect>,
    micro_config: HashMap<String, String>,
    micro_bindings: HashMap<String, String>,
    micro_current_file: Option<PathBuf>,
    micro_is_dirty: bool,
    search_debounce_task: Option<Instant>,
    ide_focus: IdeFocus,
    config: Config,
    quick_search_matches: Vec<usize>,
    quick_search_idx: usize,
    help_scroll: u16,
    bookmarks: Vec<(String, PathBuf)>,
    bookmarks_state: ListState,
    drives: Vec<(String, String)>,
    drives_state: ListState,
}

impl App {
    fn new() -> App {
        let current = env::current_dir().unwrap_or_else(|_| {
            env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
        });
        
        let config = Self::load_config();
        
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let history_path = home.join(".ka_shell_history");
        let shell_hist = fs::read_to_string(history_path)
            .map(|content| content.lines().map(String::from).collect())
            .unwrap_or_else(|_| Vec::new());

        let mut app = App {
            left_pane: Pane::new(current.clone()),
            right_pane: Pane::new(current),
            active_left: true,
            search_results: Arc::new(Mutex::new(Vec::new())),
            search_state: ListState::default(),
            search_query: String::new(),
            is_searching: false,
            is_loading: Arc::new(Mutex::new(false)),
            zoxide_mode: false,
            is_shell_mode: false,
            shell_input: String::new(),
            shell_suggestion: String::new(),
            shell_output: Arc::new(Mutex::new(Vec::new())),
            shell_scroll: 0,
            shell_history_index: shell_hist.len(),
            shell_history: shell_hist,
            shell_cmd_valid: true,
            clipboard_items: Vec::new(),
            is_cut: false,
            notification: String::new(),
            notification_time: None,
            input_mode: InputMode::None,
            text_input: String::new(),
            input_cursor_pos: 0,
            custom_scripts: Vec::new(),
            scripts_state: ListState::default(),
            pane_split_percent: config.settings.pane_split_percent,
            preview_content: String::new(),
            preview_scroll: 0,
            last_preview_path: None,
            last_preview_rect: Rect::default(),
            file_info_data: Vec::new(),
            open_with_apps: Vec::new(),
            open_with_state: ListState::default(),
            task_progress: Arc::new(Mutex::new(TaskProgress { message: String::new(), percentage: 0, active: false })),
            micro_project_tree: Vec::new(),
            micro_project_state: ListState::default(),
            micro_editor_text: vec![String::new()],
            micro_undo_stack: Vec::new(),
            micro_cursor_y: 0,
            micro_cursor_x: 0,
            micro_scroll_y: 0,
            micro_scroll_x: 0,
            micro_select_start: Option::None,
            micro_suggestions: Vec::new(),
            micro_suggestion_idx: 0,
            micro_show_autocomplete: false,
            micro_rects: Vec::new(),
            micro_config: HashMap::new(),
            micro_bindings: HashMap::new(),
            micro_current_file: None,
            micro_is_dirty: false,
            search_debounce_task: None,
            ide_focus: IdeFocus::Editor,
            config,
            quick_search_matches: Vec::new(),
            quick_search_idx: 0,
            help_scroll: 0,
            bookmarks: Self::load_bookmarks(),
            bookmarks_state: ListState::default(),
            drives: Vec::new(),
            drives_state: ListState::default(),
        };
        app.load_micro_settings();
        if !app.bookmarks.is_empty() { app.bookmarks_state.select(Some(0)); }
        app
    }

    fn resolve_path(path: &str) -> PathBuf {
        if path.starts_with("~/") {
            if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
                return home.join(&path[2..]);
            }
        }
        PathBuf::from(path)
    }

    fn load_config() -> Config {
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let config_dir = home.join(".config/ka");
        let config_file = config_dir.join("config.toml");

        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        if let Ok(content) = fs::read_to_string(&config_file) {
            if let Ok(config) = toml::from_str(&content) {
                return config;
            }
        }

        let default_config = Config::default();
        if let Ok(toml_str) = toml::to_string_pretty(&default_config) {
            let _ = fs::write(config_file, toml_str);
        }
        default_config
    }

    fn load_bookmarks() -> Vec<(String, PathBuf)> {
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let file = home.join(".config/ka/bookmarks.json");
        if let Ok(content) = fs::read_to_string(&file) {
            if let Ok(b) = serde_json::from_str::<Vec<(String, PathBuf)>>(&content) {
                return b.into_iter().map(|(name, path)| {
                    let path = if path.is_dir() {
                        path
                    } else {
                        path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
                    };
                    (name, path)
                }).collect();
            }
        }
        Vec::new()
    }

    fn save_bookmarks(&mut self) {
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let file = home.join(".config/ka/bookmarks.json");
        if let Ok(s) = serde_json::to_string_pretty(&self.bookmarks) {
            let _ = fs::write(file, s);
        }
        self.bookmarks = Self::load_bookmarks();
    }

    fn load_micro_settings(&mut self) {
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let config_dir = home.join(".config/micro");
        
        if let Ok(settings) = fs::read_to_string(config_dir.join("settings.json")) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&settings) {
                if let Some(obj) = v.as_object() {
                    for (k, val) in obj {
                        self.micro_config.insert(k.clone(), val.to_string());
                    }
                }
            }
        }
        
        if let Ok(binds) = fs::read_to_string(config_dir.join("bindings.json")) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&binds) {
                if let Some(obj) = v.as_object() {
                    for (k, val) in obj {
                        self.micro_bindings.insert(k.clone(), val.to_string());
                    }
                }
            }
        }
    }

    fn save_history(&self) {
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let history_path = home.join(".ka_shell_history");
        let content = self.shell_history.join("\n");
        let _ = fs::write(history_path, content);
    }

    fn notify(&mut self, msg: &str) {
        self.notification = format!("[*] {}", msg);
        self.notification_time = Some(Instant::now());
    }

    fn check_notification(&mut self) {
        if let Some(time) = self.notification_time {
            if time.elapsed() >= Duration::from_secs(self.config.settings.notification_duration_secs) {
                self.notification.clear();
                self.notification_time = None;
            }
        }
    }

    fn active_pane(&mut self) -> &mut Pane {
        if self.active_left {
            &mut self.left_pane
        } else {
            &mut self.right_pane
        }
    }

    fn clear_kitty_preview(&self) {
        let _ = StdCommand::new("kitty")
            .args(["+kitten", "icat", "--clear", "--stdin", "no", "--silent", "--transfer-mode", "file"])
            .spawn();
    }

    fn update_preview(&mut self, rect: Rect) {
        let path = {
            let pane = self.active_pane();
            pane.state.selected().and_then(|i| pane.items.get(i).cloned())
        };

        if path == self.last_preview_path && rect == self.last_preview_rect { return; }
        
        self.last_preview_path = path.clone();
        self.last_preview_rect = rect;
        self.clear_kitty_preview();
        self.preview_content.clear();
        self.preview_scroll = 0;

        if let Some(p) = path {
            if p.is_dir() {
                let output = StdCommand::new("lsd")
                    .args(["--tree", "--depth", "2", "--color", "never", "--icon", "never"])
                    .arg(&p)
                    .output();
                
                if let Ok(out) = output {
                    self.preview_content = String::from_utf8_lossy(&out.stdout).to_string();
                }
            } else {
                let mime_output = StdCommand::new("file")
                    .args(["--mime-type", "-b"])
                    .arg(&p)
                    .output();

                let mime = if let Ok(out) = mime_output {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                } else {
                    "application/octet-stream".to_string()
                };

                if mime.starts_with("image/") {
                    let _ = StdCommand::new("kitty")
                        .args([
                            "+kitten", "icat", "--silent", "--stdin", "no", "--transfer-mode", "file",
                            "--place", &format!("{}x{}@{}x{}", rect.width.saturating_sub(2), rect.height.saturating_sub(2), rect.x + 1, rect.y + 1)
                        ])
                        .arg(&p)
                        .spawn();
                } else if mime.starts_with("text/") || mime == "application/json" || mime == "application/x-toml" || mime == "application/xml" {
                    let output = StdCommand::new("bat")
                        .args(["--style", "plain", "--color", "never", "--line-range", ":1000", "--terminal-width", &rect.width.saturating_sub(4).to_string()])
                        .arg(&p)
                        .output()
                        .or_else(|_| StdCommand::new("cat").arg(&p).output());
                    
                    if let Ok(out) = output {
                        self.preview_content = String::from_utf8_lossy(&out.stdout)
                            .chars()
                            .filter(|c| !c.is_control() || c.is_whitespace())
                            .collect();
                    }
                } else {
                    self.preview_content = format!("Binary file: {}\nMime: {}", p.display(), mime);
                }
            }
        }
    }

    fn navigate_to(&mut self, path: PathBuf) {
        let display_path = path.display().to_string();
        let pane = self.active_pane();
        let old_path = pane.current_dir.clone();
        pane.history.push(old_path);
        pane.current_dir = path;
        let _ = env::set_current_dir(&pane.current_dir);
        pane.selected_items.clear();
        pane.anchor_index = None;
        pane.refresh();
        pane.state.select(Some(0));
        self.notify(&format!("Navigated to {}", display_path));
    }

    fn go_back(&mut self) {
        let pane = self.active_pane();
        if let Some(prev) = pane.history.past.pop() {
            pane.history.future.push(pane.current_dir.clone());
            pane.current_dir = prev;
            let _ = env::set_current_dir(&pane.current_dir);
            pane.selected_items.clear();
            pane.anchor_index = None;
            pane.refresh();
            self.notify("History back");
        }
    }

    fn go_forward(&mut self) {
        let pane = self.active_pane();
        if let Some(next) = pane.history.future.pop() {
            pane.history.past.push(pane.current_dir.clone());
            pane.current_dir = next;
            let _ = env::set_current_dir(&pane.current_dir);
            pane.selected_items.clear();
            pane.anchor_index = None;
            pane.refresh();
            self.notify("History forward");
        }
    }

    fn sync_panes(&mut self) {
        let current_path = self.active_pane().current_dir.clone();
        if self.active_left {
            self.right_pane.current_dir = current_path;
            self.right_pane.refresh();
        } else {
            self.left_pane.current_dir = current_path;
            self.left_pane.refresh();
        }
        self.notify("Panes synced");
    }

    fn spawn_search(&mut self) {
        let query = self.search_query.clone();
        if query.len() < 2 && !self.zoxide_mode { return; }

        let dir = self.active_pane().current_dir.clone();
        let results_arc = Arc::clone(&self.search_results);
        let loading_arc = Arc::clone(&self.is_loading);
        let is_zoxide = self.zoxide_mode;

        *loading_arc.lock().unwrap() = true;

        thread::spawn(move || {
            let mut final_results = Vec::new();
            if is_zoxide {
                let output = StdCommand::new("zoxide")
                    .arg("query")
                    .arg("-l")
                    .arg(if query.is_empty() { "." } else { &query })
                    .output();

                if let Ok(out) = output {
                    let result_str = String::from_utf8_lossy(&out.stdout);
                    for line in result_str.lines() {
                        if !line.is_empty() {
                            let path = PathBuf::from(line);
                            if path.is_dir() {
                                final_results.push(path.clone());
                                let files_in_dir = StdCommand::new("fd")
                                    .args(["--type", "f", "--max-results", "10", "."])
                                    .arg(&path)
                                    .output();
                                if let Ok(f_out) = files_in_dir {
                                    let f_str = String::from_utf8_lossy(&f_out.stdout);
                                    for f_line in f_str.lines() {
                                        final_results.push(PathBuf::from(f_line));
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                let output = StdCommand::new("fd")
                    .args(["--type", "f", "--type", "d", "--hidden", "--exclude", ".git", "--exclude", "node_modules", "--exclude", "target", "--max-results", "100"])
                    .arg(&query)
                    .arg(&dir)
                    .output();
                if let Ok(out) = output {
                    let result_str = String::from_utf8_lossy(&out.stdout);
                    final_results = result_str.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect();
                }
            }

            let mut res = results_arc.lock().unwrap();
            *res = final_results;
            *loading_arc.lock().unwrap() = false;
        });
        self.search_state.select(Some(0));
    }

    fn execute_shell_command(&mut self) {
        let trimmed = self.shell_input.trim().to_string();
        if trimmed.is_empty() { return; }
        
        if trimmed == "clear" {
            self.shell_output.lock().unwrap().clear();
            self.shell_input.clear();
            self.shell_suggestion.clear();
            self.input_cursor_pos = 0;
            self.shell_scroll = 0;
            self.notify("Shell output cleared");
            return;
        }

        if trimmed == "cd" || trimmed.starts_with("cd ") {
            let path_str = if trimmed == "cd" { "~" } else { trimmed[3..].trim() };
            let current_dir = self.active_pane().current_dir.clone();
            let new_path = if path_str == "~" {
                env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
            } else {
                current_dir.join(path_str)
            };

            if new_path.exists() && new_path.is_dir() {
                self.navigate_to(new_path);
                self.shell_output.lock().unwrap().push(format!("$ {}", trimmed));
                self.shell_input.clear();
                self.shell_suggestion.clear();
                self.input_cursor_pos = 0;
                return;
            }
        }

        let cmd_str = trimmed.clone();
        self.shell_history.push(cmd_str.clone());
        self.shell_history_index = self.shell_history.len();

        let dir = self.active_pane().current_dir.clone();
        let output_arc = Arc::clone(&self.shell_output);
        
        thread::spawn(move || {
            let shell = env::var("SHELL").unwrap_or_else(|_| String::from("sh"));
            let output = StdCommand::new(shell)
                .arg("-c")
                .arg(&cmd_str)
                .current_dir(dir)
                .output();
            
            let mut out_guard = output_arc.lock().unwrap();
            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if !stdout.is_empty() { out_guard.push(stdout.trim().to_string()); }
                    if !stderr.is_empty() { out_guard.push(stderr.trim().to_string()); }
                }
                Err(e) => {
                    out_guard.push(format!("Error: {}", e));
                }
            }
        });

        self.shell_output.lock().unwrap().push(format!("$ {}", trimmed));
        self.shell_input.clear();
        self.shell_suggestion.clear();
        self.input_cursor_pos = 0;
        self.notify("Executing...");
    }

    fn open_external_term(&mut self) {
        let dir = self.active_pane().current_dir.clone();
        let term = if cfg!(target_os = "macos") {
            vec!["open", "-a", "Terminal"]
        } else {
            vec!["kitty", "alacritty", "foot", "gnome-terminal", "xterm"]
        };
        
        if cfg!(target_os = "macos") {
            let _ = StdCommand::new("open").arg("-a").arg("Terminal").arg(&dir).spawn();
            self.notify("Opened Terminal");
            return;
        }

        for t in term {
            if StdCommand::new(t).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok() {
                let _ = StdCommand::new(t).current_dir(&dir).spawn();
                self.notify(&format!("Opened {}", t));
                return;
            }
        }
        self.notify("No terminal emulator found");
    }

    fn shell_update_suggestion(&mut self) {
        if self.shell_input.is_empty() {
            self.shell_suggestion.clear();
            self.shell_cmd_valid = true;
            return;
        }

        let first_word = self.shell_input.split_whitespace().next().unwrap_or("");
        
        self.shell_cmd_valid = StdCommand::new("which")
            .arg(first_word)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let shell = env::var("SHELL").unwrap_or_else(|_| String::from("/bin/sh"));
        let dir = self.active_pane().current_dir.clone();
        
        let cmd = if shell.contains("fish") {
            format!("complete -C'{}'", self.shell_input)
        } else if shell.contains("zsh") {
            format!("zsh -c 'autoload -U compinit; compinit; compadd -D -m \"{}*\"'", self.shell_input)
        } else {
            format!("compgen -c '{}'", self.shell_input)
        };

        if let Ok(out) = StdCommand::new(&shell)
            .arg("-c")
            .arg(cmd)
            .current_dir(dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output() {
            let results = String::from_utf8_lossy(&out.stdout);
            if let Some(first_line) = results.lines().next() {
                let suggested = first_line.split('\t').next().unwrap_or("").to_string();
                if suggested.starts_with(&self.shell_input) && suggested.len() > self.shell_input.len() {
                    self.shell_suggestion = suggested[self.shell_input.len()..].to_string();
                } else {
                    self.shell_suggestion.clear();
                }
            }
        }
    }

    fn shell_history_up(&mut self) {
        if self.shell_history_index > 0 {
            self.shell_history_index -= 1;
            self.shell_input = self.shell_history[self.shell_history_index].clone();
            self.input_cursor_pos = self.shell_input.chars().count();
            self.shell_suggestion.clear();
        }
    }

    fn shell_history_down(&mut self) {
        if self.shell_history_index < self.shell_history.len().saturating_sub(1) {
            self.shell_history_index += 1;
            self.shell_input = self.shell_history[self.shell_history_index].clone();
            self.input_cursor_pos = self.shell_input.chars().count();
        } else {
            self.shell_history_index = self.shell_history.len();
            self.shell_input.clear();
            self.input_cursor_pos = 0;
        }
        self.shell_suggestion.clear();
    }

    #[cfg(target_os = "macos")]
    fn copy_item(&mut self, cut: bool) {
        let targets = self.active_pane().get_targets();
        if targets.is_empty() { return; }

        self.is_cut = cut;
        self.clipboard_items = targets.clone();

        let mut paths_str = String::new();
        for p in &targets {
            let abs_path = p.canonicalize().unwrap_or(p.to_path_buf());
            paths_str.push_str(&format!("file://{}\n", abs_path.display()));
        }

        let _ = StdCommand::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child.stdin.as_mut().unwrap().write_all(paths_str.as_bytes())?;
                Ok(())
            });

        self.notify(&format!("{} {} items", if cut { "Cut" } else { "Copied" }, targets.len()));
        self.active_pane().selected_items.clear();
        self.active_pane().anchor_index = None;
    }

    #[cfg(not(target_os = "macos"))]
    fn copy_item(&mut self, cut: bool) {
        let targets = self.active_pane().get_targets();
        if targets.is_empty() { return; }

        self.is_cut = cut;
        self.clipboard_items = targets.clone();

        let mut paths_str = String::new();
        for p in &targets {
            let abs_path = p.canonicalize().unwrap_or(p.to_path_buf());
            paths_str.push_str(&format!("file://{}\n", abs_path.display()));
        }

        let cmd = if StdCommand::new("wl-copy").arg("--version").output().is_ok() { "wl-copy" } else { "xclip" };
        let args = if cmd == "xclip" { vec!["-selection", "clipboard", "-t", "text/uri-list"] } else { vec!["-t", "text/uri-list"] };

        let _ = StdCommand::new(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child.stdin.as_mut().unwrap().write_all(paths_str.as_bytes())?;
                Ok(())
            });

        self.notify(&format!("{} {} items", if cut { "Cut" } else { "Copied" }, targets.len()));
        self.active_pane().selected_items.clear();
        self.active_pane().anchor_index = None;
    }

    #[cfg(target_os = "macos")]
    fn paste_item(&mut self) {
        let dest_dir = self.active_pane().current_dir.clone();
        if self.clipboard_items.is_empty() {
            if let Ok(out) = StdCommand::new("pbpaste").output() {
                let content = String::from_utf8_lossy(&out.stdout);
                for line in content.lines() {
                    let p = PathBuf::from(line.trim().trim_start_matches("file://"));
                    if p.exists() {
                        self.clipboard_items.push(p);
                        self.is_cut = false;
                    }
                }
            }
        }
        if self.clipboard_items.is_empty() { return; }

        let is_cut = self.is_cut;
        let items = self.clipboard_items.clone();
        let progress = Arc::clone(&self.task_progress);
        {
            let mut p = progress.lock().unwrap();
            p.active = true;
            p.percentage = 0;
            p.message = if is_cut { "󰪹 Moving..." } else { "󰆏 Copying..." }.to_string();
        }
        thread::spawn(move || {
            let total = items.len();
            for (i, src) in items.iter().enumerate() {
                let dest = Self::get_unique_path(&dest_dir, src.file_name().unwrap_or_default());
                let _ = if is_cut { fs::rename(src, &dest).map(|_| ()) } else if src.is_dir() { Self::copy_dir_recursive_impl(src, &dest) } else { fs::copy(src, &dest).map(|_| ()) };
                {
                    let mut p = progress.lock().unwrap();
                    p.percentage = ((i + 1) as f32 / total as f32 * 100.0) as u16;
                }
            }
            thread::sleep(Duration::from_millis(300));
            let mut p = progress.lock().unwrap();
            p.active = false;
        });
        if is_cut { self.clipboard_items.clear(); }
        self.notify("Paste operation started");
    }

    #[cfg(not(target_os = "macos"))]
    fn paste_item(&mut self) {
        let dest_dir = self.active_pane().current_dir.clone();
        if self.clipboard_items.is_empty() {
            let cmd = if StdCommand::new("wl-paste").arg("--version").output().is_ok() { "wl-paste" } else { "xclip" };
            let args = if cmd == "xclip" { vec!["-selection", "clipboard", "-o"] } else { vec![] };

            if let Ok(out) = StdCommand::new(cmd).args(args).output() {
                let content = String::from_utf8_lossy(&out.stdout);
                for line in content.lines() {
                    let p = PathBuf::from(line.trim().trim_start_matches("file://"));
                    if p.exists() {
                        self.clipboard_items.push(p);
                        self.is_cut = false;
                    }
                }
            }
        }
        if self.clipboard_items.is_empty() { return; }

        let is_cut = self.is_cut;
        let items = self.clipboard_items.clone();
        let progress = Arc::clone(&self.task_progress);
        {
            let mut p = progress.lock().unwrap();
            p.active = true;
            p.percentage = 0;
            p.message = if is_cut { "󰪹 Moving..." } else { "󰆏 Copying..." }.to_string();
        }
        thread::spawn(move || {
            let total = items.len();
            for (i, src) in items.iter().enumerate() {
                let dest = Self::get_unique_path(&dest_dir, src.file_name().unwrap_or_default());
                let _ = if is_cut { fs::rename(src, &dest).map(|_| ()) } else if src.is_dir() { Self::copy_dir_recursive_impl(src, &dest) } else { fs::copy(src, &dest).map(|_| ()) };
                {
                    let mut p = progress.lock().unwrap();
                    p.percentage = ((i + 1) as f32 / total as f32 * 100.0) as u16;
                }
            }
            thread::sleep(Duration::from_millis(300));
            let mut p = progress.lock().unwrap();
            p.active = false;
        });
        if is_cut { self.clipboard_items.clear(); }
        self.notify("Paste operation started");
    }

    fn get_unique_path(dir: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
        let mut path = dir.join(file_name);
        if !path.exists() { return path; }
        let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let extension = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let mut counter = 1;
        while path.exists() {
            let new_name = format!("{}_{}{}", stem, counter, extension);
            path = dir.join(new_name);
            counter += 1;
        }
        path
    }

    fn copy_dir_recursive_impl(src: &PathBuf, dst: &PathBuf) -> io::Result<()> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let dest_path = dst.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                Self::copy_dir_recursive_impl(&entry.path(), &dest_path)?;
            } else {
                fs::copy(entry.path(), dest_path)?;
            }
        }
        Ok(())
    }

    fn delete_item(&mut self) {
        let targets = self.active_pane().get_targets();
        if targets.is_empty() { return; }

        #[cfg(target_os = "macos")]
        {
            let mut script = String::from("tell application \"Finder\"\n");
            for p in &targets {
                let abs_path = p.canonicalize().unwrap_or_else(|_| p.clone());
                script.push_str(&format!("delete POSIX file \"{}\"\n", abs_path.display()));
            }
            script.push_str("end tell");

            let status = StdCommand::new("osascript").args(["-e", &script]).status();
            if status.is_err() || !status.unwrap().success() {
                self.notify("Error: failed to move to trash");
                return;
            }
        }

        #[cfg(target_os = "linux")]
        {
            for p in &targets {
                let abs_path = p.canonicalize().unwrap_or_else(|_| p.clone());
                let status = StdCommand::new("gio")
                    .args(["trash", &abs_path.to_string_lossy()])
                    .status();
                if status.is_err() || !status.unwrap().success() {
                    self.notify("Error: failed to move to trash");
                    return;
                }
            }
        }

        self.active_pane().selected_items.clear();
        self.active_pane().anchor_index = None;
        self.left_pane.refresh();
        self.right_pane.refresh();
        self.notify("Moved to trash");
        self.input_mode = InputMode::None;
    }

    fn create_at_active(&mut self) {
        let dir = self.active_pane().current_dir.clone();
        let input_text = self.text_input.clone(); 
        let path = dir.join(&input_text);
        
        let res = match self.input_mode {
            InputMode::CreateFile => fs::File::create(&path).map(|_| ()),
            InputMode::CreateFolder => fs::create_dir(&path),
            InputMode::Rename => {
                let pane = self.active_pane();
                if let Some(i) = pane.state.selected() {
                    if let Some(old_p) = pane.items.get(i) {
                        let new_p = old_p.parent().unwrap_or(&PathBuf::from(".")).join(&input_text);
                        fs::rename(old_p, new_p)
                    } else { Ok(()) }
                } else { Ok(()) }
            }
            _ => Ok(()),
        };
        if res.is_ok() {
            self.left_pane.refresh(); self.right_pane.refresh();
            self.notify(&format!("Operation successful: {}", input_text));
        } else {
            self.notify("Operation failed");
        }
        self.text_input.clear(); self.input_mode = InputMode::None;
        self.input_cursor_pos = 0;
    }

    fn open_custom_scripts(&mut self) {
        let kaf_path = Self::resolve_path(&self.config.paths.scripts_dir);
        if let Ok(entries) = fs::read_dir(kaf_path) {
            self.custom_scripts = entries.flatten().map(|e| e.path()).collect();
            if !self.custom_scripts.is_empty() {
                self.scripts_state.select(Some(0));
                self.input_mode = InputMode::CustomScripts;
                self.notify("Scripts menu opened");
            }
        } else {
            self.notify(&format!("Scripts folder {} not found", self.config.paths.scripts_dir));
        }
    }

    fn execute_custom_script(&mut self) {
        if let Some(script) = self.scripts_state.selected().and_then(|i| self.custom_scripts.get(i).cloned()) {
            let dir = self.active_pane().current_dir.clone();
            let interpreter = match script.extension().and_then(|s| s.to_str()) {
                Some("py") => "python3", 
                Some("js") => "node", 
                Some("rb") => "ruby",
                Some("pl") => "perl",
                _ => "sh",
            };
            
            let output_arc = Arc::clone(&self.shell_output);
            let script_path = script.clone();
            let interp = interpreter.to_string();
            
            thread::spawn(move || {
                let output = StdCommand::new(interp)
                    .arg(&script_path)
                    .current_dir(dir)
                    .output();
                
                let mut out_guard = output_arc.lock().unwrap();
                match output {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        if !stdout.is_empty() { out_guard.push(stdout.trim().to_string()); }
                        if !stderr.is_empty() { out_guard.push(stderr.trim().to_string()); }
                    }
                    Err(e) => {
                        out_guard.push(format!("Error executing script: {}", e));
                    }
                }
            });
            
            self.shell_output.lock().unwrap().push(format!("Running script: {}", script.display()));
            self.is_shell_mode = true;
            self.notify("Script started...");
        }
        self.input_mode = InputMode::None;
    }

    fn load_open_with_apps(&mut self) {
        let mut apps = Vec::new();
        for (label, cmd) in &self.config.open_with {
            apps.push((label.clone(), cmd.clone()));
        }
        self.open_with_apps = apps;
    }

    fn execute_open_with(&mut self) {
        let app = self.open_with_state.selected().and_then(|i| self.open_with_apps.get(i).map(|(_, cmd)| cmd.clone()));
        let path = self.active_pane().state.selected().and_then(|i| self.active_pane().items.get(i).cloned());
        
        if let (Some(app_cmd), Some(file_path)) = (app, path) {
            let parts: Vec<&str> = app_cmd.split_whitespace().collect();
            if parts.is_empty() { return; }
            let mut cmd = StdCommand::new(parts[0]);
            if parts.len() > 1 {
                cmd.args(&parts[1..]);
            }
            cmd.arg(&file_path);
            
            let _ = cmd.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            self.notify(&format!("Opened with {}", app_cmd));
        }
        self.input_mode = InputMode::None;
    }

    fn copy_path_to_clipboard(&mut self) {
        let path = {
            let pane = self.active_pane();
            pane.state.selected().and_then(|i| pane.items.get(i).cloned())
        };
        if let Some(p) = path {
            let path_str = p.canonicalize().unwrap_or(p).to_string_lossy().to_string();
            
            let res = StdCommand::new("pbcopy")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .and_then(|mut child| {
                    use std::io::Write;
                    child.stdin.as_mut().unwrap().write_all(path_str.as_bytes())?;
                    Ok(())
                });
            
            if res.is_ok() {
                self.notify("Path copied to clipboard");
            } else {
                self.notify("Clipboard tool not found");
            }
        } else {
            self.notify("Nothing selected");
        }
    }

    fn get_dir_size_recursive(path: &Path) -> u64 {
        let mut size = 0;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_dir() {
                        size += Self::get_dir_size_recursive(&entry.path());
                    } else {
                        size += meta.len();
                    }
                }
            }
        }
        size
    }

    fn show_file_info(&mut self) {
        let path = {
            let pane = self.active_pane();
            pane.state.selected().and_then(|i| pane.items.get(i).cloned())
        };

        if let Some(p) = path {
            if let Ok(meta) = fs::metadata(&p) {
                let mut info = Vec::new();
                info.push(format!("󰈚 Name: {}", p.file_name().unwrap_or_default().to_string_lossy()));
                info.push(format!("󰉖 Path: {}", p.display()));
                info.push(format!("󰋗 Type: {}", if p.is_dir() { "Dir" } else { "File" }));
                
                let size = if p.is_dir() {
                    Self::get_dir_size_recursive(&p)
                } else {
                    meta.len()
                };
                info.push(format!("󰈄 Size: {}", format_size(size)));
                
                #[cfg(unix)]
                {
                    let mode = meta.permissions().mode();
                    info.push(format!("󰞀 Perms: {:o}", mode & 0o777));
                    
                    info.push(format!("󰭦 UID/GID: {} / {}", meta.uid(), meta.gid()));
                    info.push(format!("󰥣 Inodes: {}", meta.ino()));

                    info.push(format!("󰌹 Links: {}", meta.nlink()));
                    info.push(format!("󰒓 BlockSize: {}", meta.blksize()));
                    info.push(format!("󰓅 Device: {}", meta.dev()));
                }

                if let Ok(mod_time) = meta.modified() {
                    info.push(format!("󰃭 Modified: {}", format_time(mod_time)));
                }
                if let Ok(acc_time) = meta.accessed() {
                    info.push(format!("󰔟 Accessed: {}", format_time(acc_time)));
                }
                if let Ok(cre_time) = meta.created() {
                    info.push(format!("󰈔 Created: {}", format_time(cre_time)));
                }
                
                self.file_info_data = info;
                self.input_mode = InputMode::FileInfo;
                self.notify("File info displayed");
            }
        }
    }

    fn open_micro_menu(&mut self) {
        self.input_mode = InputMode::MicroMenu;
        self.ide_focus = IdeFocus::Editor;
        let current_dir = self.active_pane().current_dir.clone();
        
        let path_to_open = {
            let pane = self.active_pane();
            pane.state.selected().and_then(|i| {
                let p = pane.items.get(i).cloned()?;
                if p.is_file() { Some(p) } else { None }
            })
        };

        self.micro_load_project_tree(current_dir);
        
        if let Some(path) = path_to_open {
            self.micro_open_file(path);
        } else {
            self.micro_editor_text = vec![String::new()];
            self.micro_cursor_y = 0;
            self.micro_cursor_x = 0;
            self.micro_scroll_y = 0;
            self.micro_scroll_x = 0;
            self.micro_current_file = None;
        }
        self.micro_is_dirty = false;
        self.micro_undo_stack.clear();
        self.notify("IDE mode active");
    }

    fn micro_load_project_tree(&mut self, dir: PathBuf) {
        if let Ok(entries) = fs::read_dir(&dir) {
            self.micro_project_tree = entries.flatten()
                .map(|e| e.path())
                .collect();
            if let Some(ref current) = self.micro_current_file {
                if let Some(idx) = self.micro_project_tree.iter().position(|p| p == current) {
                    self.micro_project_state.select(Some(idx));
                }
            } else {
                self.micro_project_state.select(Some(0));
            }
        }
    }

    fn micro_open_file(&mut self, path: PathBuf) {
        if path.is_file() {
            if let Ok(bytes) = fs::read(&path) {
                let content = String::from_utf8_lossy(&bytes);
                self.micro_editor_text = content.lines()
                    .map(|s| s.chars().filter(|c| !c.is_control() || c.is_whitespace()).collect())
                    .collect();
                    
                if self.micro_editor_text.is_empty() { self.micro_editor_text.push(String::new()); }
                self.micro_cursor_y = 0;
                self.micro_cursor_x = 0;
                self.micro_scroll_y = 0;
                self.micro_scroll_x = 0;
                self.micro_select_start = None;
                self.micro_current_file = Some(path.clone());
                self.micro_is_dirty = false;
                
                if let Some(idx) = self.micro_project_tree.iter().position(|p| p == &path) {
                    self.micro_project_state.select(Some(idx));
                }
                
                self.notify(&format!("Opened: {}", path.file_name().unwrap_or_default().to_string_lossy()));
            }
        }
    }

    fn micro_save_file(&mut self) {
        if let Some(path) = &self.micro_current_file {
            let content = self.micro_editor_text.join("\n");
            if fs::write(path, content).is_ok() {
                self.micro_is_dirty = false;
                self.notify("File saved");
            } else {
                self.notify("Error saving file");
            }
        } else {
            self.input_mode = InputMode::IdePromptFileName;
            self.text_input.clear();
            self.input_cursor_pos = 0;
        }
    }

    fn micro_record_undo(&mut self) {
        self.micro_is_dirty = true;
        if self.micro_undo_stack.len() > 100 {
            self.micro_undo_stack.remove(0);
        }
        self.micro_undo_stack.push(self.micro_editor_text.clone());
    }

    fn micro_undo(&mut self) {
        if let Some(prev_state) = self.micro_undo_stack.pop() {
            self.micro_editor_text = prev_state;
            self.micro_cursor_y = self.micro_cursor_y.min(self.micro_editor_text.len().saturating_sub(1));
            let line_width = self.micro_editor_text[self.micro_cursor_y].chars().count();
            self.micro_cursor_x = self.micro_cursor_x.min(line_width);
            self.notify("Undo performed");
        }
    }

    fn micro_apply_suggestion(&mut self) {
        if self.micro_show_autocomplete && !self.micro_suggestions.is_empty() {
            self.micro_record_undo();
            let selected = self.micro_suggestions[self.micro_suggestion_idx].clone();
            let line = &mut self.micro_editor_text[self.micro_cursor_y];
            
            let word_start = line.chars().take(self.micro_cursor_x)
                .collect::<String>()
                .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                .map(|i| i + 1).unwrap_or(0);
                
            let mut chars: Vec<char> = line.chars().collect();
            chars.splice(word_start..self.micro_cursor_x, selected.chars());
            *line = chars.into_iter().collect();
            
            self.micro_cursor_x = word_start + selected.chars().count();
            self.micro_show_autocomplete = false;
            self.micro_check_scrolling();
        }
    }

    fn micro_handle_input(&mut self, c: char) {
        self.micro_record_undo();
        if self.micro_select_start.is_some() {
            self.micro_delete_selection();
        }
        
        let filtered_c = if c.is_control() && !c.is_whitespace() { ' ' } else { c };
        let line = &mut self.micro_editor_text[self.micro_cursor_y];
        
        let mut chars: Vec<char> = line.chars().collect();
        
        let pair = match filtered_c {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            '"' => Some('"'),
            '\'' => Some('\''),
            _ => None,
        };

        if let Some(closing) = pair {
            chars.insert(self.micro_cursor_x, filtered_c);
            chars.insert(self.micro_cursor_x + 1, closing);
            self.micro_cursor_x += 1;
        } else {
            chars.insert(self.micro_cursor_x, filtered_c);
            self.micro_cursor_x += 1;
        }
        
        *line = chars.into_iter().collect();
        self.micro_update_autocomplete();
        self.micro_check_scrolling();
    }

    fn micro_delete_selection(&mut self) {
        if let Some((start_y, start_x)) = self.micro_select_start {
            self.micro_record_undo();
            let (mut sy, mut sx) = (start_y, start_x);
            let (mut ey, mut ex) = (self.micro_cursor_y, self.micro_cursor_x);
            if (sy, sx) > (ey, ex) {
                std::mem::swap(&mut sy, &mut ey);
                std::mem::swap(&mut sx, &mut ex);
            }

            if sy == ey {
                let mut chars: Vec<char> = self.micro_editor_text[sy].chars().collect();
                if sx < chars.len() && ex <= chars.len() {
                    chars.drain(sx..ex);
                    self.micro_editor_text[sy] = chars.into_iter().collect();
                }
            } else {
                let suffix: String = self.micro_editor_text[ey].chars().skip(ex).collect();
                let mut start_chars: Vec<char> = self.micro_editor_text[sy].chars().collect();
                start_chars.truncate(sx);
                let mut new_line = start_chars.into_iter().collect::<String>();
                new_line.push_str(&suffix);
                self.micro_editor_text[sy] = new_line;
                
                for _ in sy + 1..=ey {
                    if sy + 1 < self.micro_editor_text.len() {
                        self.micro_editor_text.remove(sy + 1);
                    }
                }
            }
            self.micro_cursor_y = sy;
            self.micro_cursor_x = sx;
            self.micro_select_start = None;
            self.micro_check_scrolling();
        }
    }

    fn micro_get_selection(&self) -> Option<String> {
        let (start_y, start_x) = self.micro_select_start?;
        let (mut sy, mut sx) = (start_y, start_x);
        let (mut ey, mut ex) = (self.micro_cursor_y, self.micro_cursor_x);

        if (sy, sx) > (ey, ex) {
            std::mem::swap(&mut sy, &mut ey);
            std::mem::swap(&mut sx, &mut ex);
        }

        let mut result = String::new();
        for i in sy..=ey {
            if let Some(line) = self.micro_editor_text.get(i) {
                let chars: Vec<char> = line.chars().collect();
                let start = if i == sy { sx.min(chars.len()) } else { 0 };
                let end = if i == ey { ex.min(chars.len()) } else { chars.len() };

                if start <= end {
                    result.push_str(&chars[start..end].iter().collect::<String>());
                }
                if i < ey {
                    result.push('\n');
                }
            }
        }
        Some(result)
    }

    fn micro_copy(&mut self) {
        let text = self.micro_get_selection().unwrap_or_else(|| self.micro_editor_text[self.micro_cursor_y].clone());
        let mut osa = StdCommand::new("osascript")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(mut stdin) = osa.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(format!("set the clipboard to \"{}\"", text.replace("\"", "\\\"")).as_bytes());
        }
        self.notify("Text copied to buffer");
    }

    fn micro_paste(&mut self) {
        let text = {
            let output = StdCommand::new("pbpaste").output().unwrap();
            String::from_utf8_lossy(&output.stdout).to_string()
        };

        let text_clean: String = text.chars().filter(|c| !c.is_control() || c.is_whitespace()).collect();
        
        self.micro_record_undo();
        if self.micro_select_start.is_some() { self.micro_delete_selection(); }
        
        let lines: Vec<&str> = text_clean.split('\n').collect();
        if lines.len() == 1 {
            let mut chars: Vec<char> = self.micro_editor_text[self.micro_cursor_y].chars().collect();
            chars.splice(self.micro_cursor_x..self.micro_cursor_x, lines[0].chars());
            self.micro_editor_text[self.micro_cursor_y] = chars.into_iter().collect();
            self.micro_cursor_x += lines[0].chars().count();
        } else {
            let line_chars: Vec<char> = self.micro_editor_text[self.micro_cursor_y].chars().collect();
            let prefix: String = line_chars[..self.micro_cursor_x].iter().collect();
            let suffix: String = line_chars[self.micro_cursor_x..].iter().collect();
            
            self.micro_editor_text[self.micro_cursor_y] = format!("{}{}", prefix, lines[0]);
            
            let mut y = self.micro_cursor_y + 1;
            for i in 1..lines.len() - 1 {
                self.micro_editor_text.insert(y, lines[i].to_string());
                y += 1;
            }
            
            let last_line_content = lines.last().unwrap();
            let last_len = last_line_content.chars().count();
            self.micro_editor_text.insert(y, format!("{}{}", last_line_content, suffix));
            
            self.micro_cursor_y = y;
            self.micro_cursor_x = last_len;
        }
        self.micro_check_scrolling();
        self.notify("Text pasted");
    }

    fn micro_update_autocomplete(&mut self) {
        let line = &self.micro_editor_text[self.micro_cursor_y];
        let prefix: String = line.chars().take(self.micro_cursor_x).collect();
        let word = prefix.split(|c: char| !c.is_alphanumeric() && c != '_')
            .last().unwrap_or("").to_string();
        
        if word.is_empty() {
            self.micro_show_autocomplete = false;
            return;
        }

        let current_ext = self.micro_current_file.as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .unwrap_or("").to_lowercase();

        let rust_kw = vec!["fn", "let", "mut", "match", "if", "else", "loop", "pub", "struct", "enum", "println!", "use", "mod", "crate", "Self", "impl", "trait", "return", "while", "for", "in"];
        let py_kw = vec!["def", "import", "from", "as", "if", "elif", "else", "while", "for", "in", "return", "class", "with", "try", "except", "lambda", "None", "True", "False"];
        let js_kw = vec!["function", "const", "let", "var", "if", "else", "for", "while", "return", "class", "export", "import", "from", "async", "await", "try", "catch", "null", "undefined"];
        let cpp_kw = vec!["int", "char", "float", "double", "if", "else", "while", "for", "return", "class", "public", "private", "protected", "using", "namespace", "include", "void", "constexpr", "static"];
        let go_kw = vec!["func", "package", "import", "var", "type", "struct", "interface", "map", "chan", "go", "select", "case", "default", "if", "for", "range", "return", "defer"];
        let sql_kw = vec!["SELECT", "FROM", "WHERE", "INSERT", "UPDATE", "DELETE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER", "ON", "GROUP", "BY", "ORDER", "LIMIT", "CREATE", "TABLE"];
        let html_kw = vec!["div", "span", "html", "head", "body", "script", "link", "meta", "style", "title", "h1", "h2", "p", "a", "img", "ul", "li", "table", "tr", "td"];
        let java_kw = vec!["public", "private", "protected", "class", "interface", "extends", "implements", "new", "import", "package", "void", "int", "double", "float", "boolean", "if", "else", "while", "for", "return", "static", "final"];
        let rb_kw = vec!["def", "end", "class", "module", "attr_reader", "attr_writer", "attr_accessor", "require", "if", "else", "elsif", "unless", "while", "until", "for", "in", "do", "begin", "rescue", "ensure", "nil", "true", "False"];

        let keywords = match current_ext.as_str() {
            "rs" => rust_kw,
            "py" => py_kw,
            "js" | "ts" | "jsx" | "tsx" => js_kw,
            "cpp" | "c" | "h" | "hpp" => cpp_kw,
            "go" => go_kw,
            "sql" => sql_kw,
            "html" => html_kw,
            "java" => java_kw,
            "rb" => rb_kw,
            _ => vec![],
        };

        self.micro_suggestions = keywords.into_iter()
            .filter(|k| k.to_lowercase().starts_with(&word.to_lowercase()) && k.to_lowercase() != word.to_lowercase())
            .map(|s| s.to_string())
            .collect();
        
        self.micro_show_autocomplete = !self.micro_suggestions.is_empty();
        self.micro_suggestion_idx = 0;
    }

    fn micro_check_scrolling(&mut self) {
        if self.micro_rects.is_empty() { return; }
        let editor_h = self.micro_rects[1].height.saturating_sub(4) as usize;
        let editor_w = self.micro_rects[1].width.saturating_sub(6) as usize;

        if self.micro_cursor_y < self.micro_scroll_y {
            self.micro_scroll_y = self.micro_cursor_y;
        } else if self.micro_cursor_y >= self.micro_scroll_y + editor_h {
            self.micro_scroll_y = self.micro_cursor_y - editor_h + 1;
        }

        let cur_line = &self.micro_editor_text[self.micro_cursor_y];
        let cursor_visual_x = UnicodeWidthStr::width(cur_line.chars().take(self.micro_cursor_x).collect::<String>().as_str());
        
        if cursor_visual_x < self.micro_scroll_x {
            self.micro_scroll_x = cursor_visual_x;
        } else if cursor_visual_x >= self.micro_scroll_x + editor_w {
            self.micro_scroll_x = cursor_visual_x - editor_w + 1;
        }
    }

    fn handle_quick_search_confirm(&mut self) {
        if self.quick_search_matches.is_empty() { return; }
        
        self.quick_search_idx = (self.quick_search_idx + 1) % self.quick_search_matches.len();
        let target_idx = self.quick_search_matches[self.quick_search_idx];
        
        let pane = if self.active_left { &mut self.left_pane } else { &mut self.right_pane };
        pane.state.select(Some(target_idx));
    }

    fn update_quick_search_results(&mut self) {
        let query = self.text_input.to_lowercase();
        if query.is_empty() {
            self.quick_search_matches.clear();
            return;
        }

        let items = if self.active_left { &self.left_pane.items } else { &self.right_pane.items };
        self.quick_search_matches = items.iter().enumerate()
            .filter(|(_, p)| p.file_name().unwrap_or_default().to_string_lossy().to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();
        
        self.quick_search_idx = 0;
        if let Some(&first) = self.quick_search_matches.first() {
            let pane = if self.active_left { &mut self.left_pane } else { &mut self.right_pane };
            pane.state.select(Some(first));
        }
    }

    fn load_and_mount_drives(&mut self) {
        self.notify("󰋊 Discovering and mounting drives...");
        let output = StdCommand::new("lsblk").args(["-J", "-o", "NAME,LABEL,MOUNTPOINT,SIZE,TYPE,FSTYPE"]).output();
        if let Ok(out) = output {
            let json_str = String::from_utf8_lossy(&out.stdout);
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
                let mut discovered = Vec::new();
                if let Some(blockdevices) = v.get("blockdevices").and_then(|b| b.as_array()) {
                    let mut queue = blockdevices.clone();
                    while let Some(dev) = queue.pop() {
                        if let Some(children) = dev.get("children").and_then(|c| c.as_array()) {
                            queue.extend(children.clone());
                        }
                        let fstype = dev.get("fstype").and_then(|s| s.as_str()).unwrap_or("");
                        let ptype = dev.get("type").and_then(|s| s.as_str()).unwrap_or("");
                        
                        if ptype == "part" || ptype == "disk" || ptype == "rom" {
                            if fstype.is_empty() || fstype == "swap" || fstype == "crypto_LUKS" || fstype == "zfs_member" || fstype == "LVM2_member" { continue; }
                            
                            let name = dev.get("name").and_then(|s| s.as_str()).unwrap_or("");
                            let label = dev.get("label").and_then(|s| s.as_str()).unwrap_or(name);
                            let size = dev.get("size").and_then(|s| s.as_str()).unwrap_or("");
                            let mut mp = dev.get("mountpoint").and_then(|s| s.as_str()).map(|s| s.to_string());
                            
                            if mp.is_none() {
                                let dev_path = format!("/dev/{}", name);
                                let _ = StdCommand::new("udisksctl").args(["mount", "-b", &dev_path]).output();
                                
                                if let Ok(m_out) = StdCommand::new("lsblk").args(["-n", "-o", "MOUNTPOINT", &dev_path]).output() {
                                    let m_str = String::from_utf8_lossy(&m_out.stdout);
                                    let m_trim = m_str.trim();
                                    if !m_trim.is_empty() && m_trim != "null" {
                                        mp = Some(m_trim.to_string());
                                    }
                                }
                            }
                            
                            if let Some(mnt) = mp {
                                discovered.push((format!("󰋊 {} ({}) [{}]", label, size, mnt), mnt));
                            }
                        }
                    }
                }
                if !discovered.iter().any(|d| d.1 == "/") {
                    discovered.insert(0, ("󰋊 Root [/]".to_string(), "/".to_string()));
                }
                self.drives = discovered;
                self.drives_state.select(Some(0));
            }
        }
    }


    fn next(&mut self) {
        if self.input_mode == InputMode::MicroMenu {
            if self.micro_show_autocomplete {
                let len = self.micro_suggestions.len();
                if len > 0 {
                    self.micro_suggestion_idx = (self.micro_suggestion_idx + 1) % len;
                }
            } else if self.ide_focus == IdeFocus::ProjectTree {
                let len = self.micro_project_tree.len();
                if len > 0 {
                    let i = self.micro_project_state.selected().map(|i| (i + 1) % len).unwrap_or(0);
                    self.micro_project_state.select(Some(i));
                }
            }
            return;
        }
        if self.input_mode == InputMode::CustomScripts {
            let len = self.custom_scripts.len();
            if len > 0 {
                let i = self.scripts_state.selected().map(|i| (i + 1) % len).unwrap_or(0);
                self.scripts_state.select(Some(i));
            }
            return;
        }
        if self.input_mode == InputMode::OpenWith {
            let len = self.open_with_apps.len();
            if len > 0 {
                let i = self.open_with_state.selected().map(|i| (i + 1) % len).unwrap_or(0);
                self.open_with_state.select(Some(i));
            }
            return;
        }
        if self.is_searching {
            let len = self.search_results.lock().unwrap().len();
            if len > 0 {
                let i = self.search_state.selected().map(|i| (i + 1) % len).unwrap_or(0);
                self.search_state.select(Some(i));
            }
            return;
        }
        let pane = self.active_pane();
        let len = pane.items.len();
        if len > 0 {
            let i = pane.state.selected().map(|i| (i + 1) % len).unwrap_or(0);
            pane.state.select(Some(i));
        }
    }

    fn previous(&mut self) {
        if self.input_mode == InputMode::MicroMenu {
            if self.micro_show_autocomplete {
                let len = self.micro_suggestions.len();
                if len > 0 {
                    self.micro_suggestion_idx = if self.micro_suggestion_idx == 0 { len - 1 } else { self.micro_suggestion_idx - 1 };
                }
            } else if self.ide_focus == IdeFocus::ProjectTree {
                let len = self.micro_project_tree.len();
                if len > 0 {
                    let i = self.micro_project_state.selected().map(|i| if i == 0 { len - 1 } else { i - 1 }).unwrap_or(0);
                    self.micro_project_state.select(Some(i));
                }
            }
            return;
        }
        if self.input_mode == InputMode::CustomScripts {
            let len = self.custom_scripts.len();
            if len > 0 {
                let i = self.scripts_state.selected().map(|i| if i == 0 { len - 1 } else { i - 1 }).unwrap_or(0);
                self.scripts_state.select(Some(i));
            }
            return;
        }
        if self.input_mode == InputMode::OpenWith {
            let len = self.open_with_apps.len();
            if len > 0 {
                let i = self.open_with_state.selected().map(|i| if i == 0 { len - 1 } else { i - 1 }).unwrap_or(0);
                self.open_with_state.select(Some(i));
            }
            return;
        }
        if self.is_searching {
            let len = self.search_results.lock().unwrap().len();
            if len > 0 {
                let i = self.search_state.selected().map(|i| if i == 0 { len - 1 } else { i - 1 }).unwrap_or(0);
                self.search_state.select(Some(i));
            }
            return;
        }
        let pane = self.active_pane();
        let len = pane.items.len();
        if len > 0 {
            let i = pane.state.selected().map(|i| if i == 0 { len - 1 } else { i - 1 }).unwrap_or(0);
            pane.state.select(Some(i));
        }
    }

    fn jump_to_start(&mut self) {
        let pane = self.active_pane();
        if !pane.items.is_empty() {
            pane.state.select(Some(0));
            self.notify("Jumped to top");
        }
    }

    fn jump_to_end(&mut self) {
        let pane = self.active_pane();
        if !pane.items.is_empty() {
            pane.state.select(Some(pane.items.len().saturating_sub(1)));
            self.notify("Jumped to bottom");
        }
    }

    fn enter(&mut self) {
        if self.input_mode == InputMode::Bookmarks {
            if let Some(i) = self.bookmarks_state.selected() {
                if let Some((_, path)) = self.bookmarks.get(i).cloned() {
                    self.navigate_to(path);
                    self.input_mode = InputMode::None;
                }
            }
            return;
        }
        if self.input_mode == InputMode::OpenWith {
            self.execute_open_with();
            return;
        }
        if self.input_mode == InputMode::CreateFile || self.input_mode == InputMode::CreateFolder || self.input_mode == InputMode::Rename {
            self.create_at_active();
            return;
        }
        if self.input_mode == InputMode::DeleteConfirm {
            self.delete_item();
            return;
        }
        if self.input_mode == InputMode::QuickSearch {
            self.handle_quick_search_confirm();
            return;
        }
        let path = {
            let pane = self.active_pane();
            pane.state.selected().and_then(|i| pane.items.get(i).cloned())
        };
        if let Some(p) = path {
            if p.is_dir() { self.navigate_to(p); }
            else {
                let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
                match StdCommand::new(opener)
                    .arg(&p)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn() 
                {
                    Ok(_) => self.notify(&format!("Opened file: {}", p.display())),
                    Err(_) => self.notify("Error opening file"),
                }
            }
        }
    }

    fn backward(&mut self) {
        let p = self.active_pane().current_dir.parent().map(|p| p.to_path_buf());
        if let Some(path) = p { self.navigate_to(path); }
    }

    fn toggle_sort(&mut self) {
        let mode_name = {
            let pane = self.active_pane();
            pane.sort_mode = match pane.sort_mode {
                SortMode::Name => SortMode::Extension, SortMode::Extension => SortMode::Size,
                SortMode::Size => SortMode::Date, SortMode::Date => SortMode::Name,
            };
            pane.refresh();
            pane.sort_mode.to_str().to_string()
        };
        self.notify(&format!("Sort: {}", mode_name));
    }

    fn handle_search_confirm(&mut self) {
        let path_opt = {
            let results = self.search_results.lock().unwrap();
            let idx = self.search_state.selected().unwrap_or(0);
            if idx < results.len() {
                Some(results[idx].clone())
            } else {
                None
            }
        };

        if let Some(path) = path_opt {
            if path.is_dir() { 
                self.navigate_to(path); 
            } else { 
                let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
                let _ = StdCommand::new(opener)
                    .arg(path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
            }
            self.is_searching = false;
        }
    }

    fn jump_to_file_dir(&mut self) {
        let path_opt = {
            let results = self.search_results.lock().unwrap();
            let idx = self.search_state.selected().unwrap_or(0);
            if idx < results.len() {
                Some(results[idx].clone())
            } else {
                None
            }
        };

        if let Some(path) = path_opt {
            if let Some(parent) = path.parent() {
                self.navigate_to(parent.to_path_buf());
                self.is_searching = false;
            }
        }
    }

    fn archive_operation(&mut self) {
        let targets = self.active_pane().get_targets();
        if targets.is_empty() { return; }

        let current_dir = self.active_pane().current_dir.clone();
        let progress = Arc::clone(&self.task_progress);
        let targets_clone = targets.clone();

        {
            let mut p = progress.lock().unwrap();
            p.active = true;
            p.percentage = 0;
            p.message = "Archive task...".to_string();
        }

        thread::spawn(move || {
            let has_ouch = StdCommand::new("type")
                .arg("ouch")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if targets_clone.len() == 1 && targets_clone[0].extension().map_or(false, |e| matches!(e.to_str().unwrap_or(""), "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar")) {
                let file_path = &targets_clone[0];
                let filename = file_path.file_name().unwrap_or_default();
                let mut out_dir = file_path.clone();
                out_dir.set_extension(""); 

                if has_ouch {
                    let _ = StdCommand::new("ouch")
                        .arg("decompress")
                        .arg(filename)
                        .current_dir(&current_dir)
                        .output();
                } else {
                    let _ = fs::create_dir_all(&out_dir);
                    if filename.to_string_lossy().ends_with(".zip") {
                        let _ = StdCommand::new("unzip").arg(file_path).arg("-d").arg(&out_dir).output();
                    } else {
                        let _ = StdCommand::new("tar").arg("-xf").arg(file_path).arg("-C").arg(&out_dir).output();
                    }
                }
            } else {
                let base_name = targets_clone[0].file_stem().unwrap_or_default().to_string_lossy();
                let output_file = current_dir.join(format!("{}.tar.xz", base_name));

                if has_ouch {
                    let mut cmd = StdCommand::new("ouch");
                    cmd.arg("compress");
                    for t in &targets_clone { cmd.arg(t); }
                    cmd.arg(&output_file);
                    let _ = cmd.output();
                } else {
                    let mut cmd = StdCommand::new("tar");
                    cmd.arg("-cJf").arg(&output_file).arg("-C").arg(&current_dir);
                    for t in &targets_clone {
                        cmd.arg(t.file_name().unwrap_or_default());
                    }
                    let _ = cmd.output();
                }
            }

            {
                let mut p = progress.lock().unwrap();
                p.percentage = 100;
            }
            thread::sleep(Duration::from_millis(300));
            let mut p = progress.lock().unwrap();
            p.active = false;
        });

        self.active_pane().selected_items.clear();
    }

    fn check_action(&self, key: event::KeyEvent, action: &str) -> bool {
        if let Some(bind_str) = self.config.binds.get(action) {
            let bind_str = bind_str.to_lowercase();
            let mut expected_mods = KeyModifiers::NONE;
            let mut expected_code = KeyCode::Null;

            let parts: Vec<&str> = bind_str.split('+').collect();
            for part in parts {
                match part {
                    "ctrl" => expected_mods.insert(KeyModifiers::CONTROL),
                    "cmd" | "command" | "super" => expected_mods.insert(KeyModifiers::SUPER),
                    "alt" => expected_mods.insert(KeyModifiers::ALT),
                    "shift" => expected_mods.insert(KeyModifiers::SHIFT),
                    "f1" => expected_code = KeyCode::F(1),
                    "f2" => expected_code = KeyCode::F(2),
                    "f3" => expected_code = KeyCode::F(3),
                    "f4" => expected_code = KeyCode::F(4),
                    "f5" => expected_code = KeyCode::F(5),
                    "f6" => expected_code = KeyCode::F(6),
                    "f7" => expected_code = KeyCode::F(7),
                    "f8" => expected_code = KeyCode::F(8),
                    "f9" => expected_code = KeyCode::F(9),
                    "f10" => expected_code = KeyCode::F(10),
                    "f11" => expected_code = KeyCode::F(11),
                    "f12" => expected_code = KeyCode::F(12),
                    "enter" => expected_code = KeyCode::Enter,
                    "esc" => expected_code = KeyCode::Esc,
                    "del" | "delete" => expected_code = KeyCode::Delete,
                    "space" => expected_code = KeyCode::Char(' '),
                    "up" => expected_code = KeyCode::Up,
                    "down" => expected_code = KeyCode::Down,
                    "left" => expected_code = KeyCode::Left,
                    "right" => expected_code = KeyCode::Right,
                    "tab" => expected_code = KeyCode::Tab,
                    "backspace" => expected_code = KeyCode::Backspace,
                    c if c.len() == 1 => expected_code = KeyCode::Char(c.chars().next().unwrap()),
                    _ => {}
                }
            }
            if let KeyCode::Char(c) = key.code {
                if let KeyCode::Char(ec) = expected_code {
                    return c.to_ascii_lowercase() == ec.to_ascii_lowercase() && key.modifiers.contains(expected_mods);
                }
            }
            return key.code == expected_code && key.modifiers.contains(expected_mods);
        }
        false
    }

    fn get_bind_text(&self, cmd: &str) -> String {
        self.config.binds.get(cmd).cloned().unwrap_or_else(|| "none".to_string())
    }

    fn get_colors(&self) -> (Color, Color, Color, Color, Color, Color, Color, Color, Color) {
        let c = &self.config.colors;
        (
            Color::Rgb(hex_to_rgb(&c.bg)[0], hex_to_rgb(&c.bg)[1], hex_to_rgb(&c.bg)[2]),
            Color::Rgb(hex_to_rgb(&c.text)[0], hex_to_rgb(&c.text)[1], hex_to_rgb(&c.text)[2]),
            Color::Rgb(hex_to_rgb(&c.dim)[0], hex_to_rgb(&c.dim)[1], hex_to_rgb(&c.dim)[2]),
            Color::Rgb(hex_to_rgb(&c.teal)[0], hex_to_rgb(&c.teal)[1], hex_to_rgb(&c.teal)[2]),
            Color::Rgb(hex_to_rgb(&c.green)[0], hex_to_rgb(&c.green)[1], hex_to_rgb(&c.green)[2]),
            Color::Rgb(hex_to_rgb(&c.grey)[0], hex_to_rgb(&c.grey)[1], hex_to_rgb(&c.grey)[2]),
            Color::Rgb(hex_to_rgb(&c.accent)[0], hex_to_rgb(&c.accent)[1], hex_to_rgb(&c.accent)[2]),
            Color::Rgb(hex_to_rgb(&c.hint_bg)[0], hex_to_rgb(&c.hint_bg)[1], hex_to_rgb(&c.hint_bg)[2]),
            Color::Rgb(hex_to_rgb(&c.error)[0], hex_to_rgb(&c.error)[1], hex_to_rgb(&c.error)[2]),
        )
    }
}

fn hex_to_rgb(hex: &str) -> [u8; 3] {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
        [r, g, b]
    } else {
        [255, 255, 255]
    }
}

fn get_icon(path: &PathBuf, teal: Color, green: Color, grey: Color, text: Color) -> (&str, Color) {
    if path.is_dir() {
        return (" ", teal);
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "rs" => (" ", Color::Rgb(150, 60, 50)),
        "py" => (" ", green),
        "js" | "ts" | "jsx" | "tsx" => (" ", Color::Rgb(150, 130, 35)),
        "json" | "toml" | "yaml" | "yml" => (" ", teal),
        "md" => (" ", Color::Rgb(80, 130, 170)),
        "sh" | "bash" | "zsh" | "fish" => ("󱆃 ", green),
        "zip" | "tar" | "gz" | "7z" => ("󰗚 ", grey),
        _ => ("󰈔 ", text),
    }
}

fn format_size(size: u64) -> String {
    let mb = size as f64 / 1_048_576.0;
    if mb < 1.0 {
        let kb = size as f64 / 1024.0;
        format!("{:.1} KB", kb)
    } else if mb > 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else {
        format!("{:.2} MB", mb)
    }
}

fn format_time(time: SystemTime) -> String {
    let dt: DateTime<Local> = time.into();
    let now = Local::now();
    let duration = now.signed_duration_since(dt);

    if duration.num_seconds() < 60 {
        format!("{}s ago", duration.num_seconds().max(0))
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 && dt.naive_local().date() == now.naive_local().date() {
        format!("Today {}", dt.format("%H:%M"))
    } else if dt.naive_local().date() == now.naive_local().date().pred_opt().unwrap_or(now.naive_local().date()) {
        format!("Yesterday {}", dt.format("%H:%M"))
    } else {
        dt.format("%Y-%m-%d %H:%M").to_string()
    }
}

fn generic_syntax_highlight<'a>(line: &'a str, file_ext: &'a str, line_idx: usize, select_start: Option<(usize, usize)>, cursor_pos: (usize, usize), colors: (Color, Color, Color, Color, Color, Color, Color, Color, Color)) -> Line<'a> {
    let (_, _text_clr, dim_clr, teal_clr, green_clr, grey_clr, accent_clr, _, error_clr) = colors;
    let mut spans = Vec::new();
    
    let rust_kw = ["fn", "let", "mut", "match", "if", "else", "loop", "pub", "struct", "enum", "use", "mod", "crate", "impl", "trait", "return", "type", "self", "Self", "where", "async", "await", "unsafe", "dyn"];
    let py_kw = ["def", "import", "from", "as", "if", "elif", "else", "while", "for", "in", "return", "class", "with", "try", "except", "lambda", "None", "True", "False", "global", "nonlocal", "pass", "break", "continue"];
    let js_kw = ["function", "const", "let", "var", "if", "else", "for", "while", "return", "class", "export", "import", "from", "async", "await", "try", "catch", "null", "undefined", "this", "new", "delete", "typeof"];
    let cpp_kw = ["int", "char", "float", "double", "if", "else", "while", "for", "return", "class", "public", "private", "protected", "using", "namespace", "include", "void", "constexpr", "static"];
    let go_kw = ["func", "package", "import", "var", "type", "struct", "interface", "map", "chan", "go", "select", "case", "default", "if", "for", "range", "return", "defer"];
    let sql_kw = ["SELECT", "FROM", "WHERE", "INSERT", "UPDATE", "DELETE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER", "ON", "GROUP", "BY", "ORDER", "LIMIT", "CREATE", "TABLE"];
    let java_kw = ["public", "private", "protected", "class", "interface", "extends", "implements", "new", "import", "package", "void", "int", "double", "float", "boolean", "if", "else", "while", "for", "return", "static", "final"];
    let rb_kw = ["def", "end", "class", "module", "attr_reader", "attr_writer", "attr_accessor", "require", "if", "else", "elsif", "unless", "while", "until", "for", "in", "do", "begin", "rescue", "ensure", "nil", "true", "False"];

    let keywords = match file_ext {
        "rs" => &rust_kw[..],
        "py" => &py_kw[..],
        "js" | "ts" | "jsx" | "tsx" => &js_kw[..],
        "cpp" | "c" | "h" | "hpp" => &cpp_kw[..],
        "go" => &go_kw[..],
        "sql" => &sql_kw[..],
        "java" => &java_kw[..],
        "rb" => &rb_kw[..],
        _ => &[][..],
    };

    let mut current_word = String::new();
    let mut x_offset = 0;
    let mut chars = line.chars().peekable();
    let mut last_selected: Option<bool> = None;

    let open_brackets = ['(', '[', '{'];
    let close_brackets = [')', ']', '}'];

    while let Some(c) = chars.next() {
        let mut style = Style::default();
        let mut is_selected = false;

        if let Some((sy, sx)) = select_start {
            let (ey, ex) = cursor_pos;
            let (start_pos, end_pos) = if (sy, sx) < (ey, ex) { ((sy, sx), (ey, ex)) } else { ((ey, ex), (sy, sx)) };
            if (line_idx, x_offset) >= start_pos && (line_idx, x_offset) < end_pos {
                style = style.bg(Color::Rgb(40, 55, 55));
                is_selected = true;
            }
        }

        if !current_word.is_empty() && Some(is_selected) != last_selected {
            let is_kw = keywords.contains(&current_word.as_str());
            let is_type = current_word.chars().next().map_or(false, |first| first.is_uppercase()) && current_word.len() > 1;
            let mut word_style = if is_kw {
                Style::default().fg(teal_clr).add_modifier(Modifier::BOLD)
            } else if is_type {
                Style::default().fg(accent_clr)
            } else {
                Style::default()
            };
            if last_selected.unwrap_or(false) { word_style = word_style.bg(Color::Rgb(40, 55, 55)); }
            spans.push(Span::styled(current_word.clone(), word_style));
            current_word.clear();
        }
        last_selected = Some(is_selected);

        let is_py_comment = c == '#' && matches!(file_ext, "py" | "rb" | "sh" | "yaml" | "yml" | "toml");
        let is_slash_comment = c == '/' && chars.peek() == Some(&'/');
        let is_dash_comment = c == '-' && chars.peek() == Some(&'-') && file_ext == "sql";

        if is_py_comment || is_slash_comment || is_dash_comment {
            if !current_word.is_empty() {
                spans.push(Span::styled(current_word.clone(), style));
                current_word.clear();
            }
            let mut comment = String::from(c);
            while let Some(next_c) = chars.next() {
                comment.push(next_c);
            }
            spans.push(Span::styled(comment, style.fg(dim_clr).add_modifier(Modifier::ITALIC)));
            break;
        } 
        else if c == '"' || c == '\'' {
            if !current_word.is_empty() {
                spans.push(Span::styled(current_word.clone(), style));
                current_word.clear();
            }
            let mut string_lit = String::from(c);
            let mut closed = false;
            while let Some(&next_c) = chars.peek() {
                string_lit.push(chars.next().unwrap());
                if next_c == c { closed = true; break; }
            }
            let string_style = if closed { style.fg(green_clr) } else { style.fg(error_clr).add_modifier(Modifier::UNDERLINED) };
            spans.push(Span::styled(string_lit, string_style));
        } 
        else if c.is_alphanumeric() || c == '_' {
            current_word.push(c);
        } else {
            if !current_word.is_empty() {
                let is_kw = keywords.contains(&current_word.as_str());
                let is_type = current_word.chars().next().map_or(false, |first| first.is_uppercase()) && current_word.len() > 1;
                
                let mut word_style = if is_kw {
                    Style::default().fg(teal_clr).add_modifier(Modifier::BOLD)
                } else if is_type {
                    Style::default().fg(accent_clr)
                } else {
                    Style::default()
                };
                if is_selected { word_style = word_style.bg(Color::Rgb(40, 55, 55)); }
                spans.push(Span::styled(current_word.clone(), word_style));
                current_word.clear();
            }
            
            let bracket_style = if open_brackets.contains(&c) || close_brackets.contains(&c) {
                style.fg(accent_clr)
            } else {
                style.fg(grey_clr)
            };
            spans.push(Span::styled(c.to_string(), bracket_style));
        }
        x_offset += 1;
    }

    if !current_word.is_empty() { 
        let is_kw = keywords.contains(&current_word.as_str());
        let mut word_style = if is_kw {
            Style::default().fg(teal_clr).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        if last_selected.unwrap_or(false) { word_style = word_style.bg(Color::Rgb(40, 55, 55)); }
        spans.push(Span::styled(current_word, word_style)); 
    }
    Line::from(spans)
}

fn main_run_loop<B: io::Write>(terminal: &mut Terminal<CrosstermBackend<B>>, app: &mut App) -> io::Result<()> {
    loop {
        app.check_notification();
        
        if let Some(time) = app.search_debounce_task {
            if time.elapsed() > Duration::from_millis(app.config.settings.search_debounce_ms) {
                app.spawn_search();
                app.search_debounce_task = None;
            }
        }

        if !app.task_progress.lock().unwrap().active {
            app.left_pane.refresh();
            app.right_pane.refresh();
        }

        let size = terminal.size()?;

        if app.input_mode == InputMode::PreviewPopup {
            let popup_rect = centered_rect(80, 80, size);
            app.update_preview(popup_rect);
        } else {
            app.clear_kitty_preview();
        }
        
        terminal.draw(|f| {
            ui(f, app);
        })?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Paste(ref s) => {
                    if app.input_mode == InputMode::None {
                        let path_str = s.trim();
                        let p = PathBuf::from(path_str);
                        if p.exists() {
                            app.clipboard_items = vec![p];
                            app.is_cut = false;
                            app.paste_item();
                        }
                    } else if app.input_mode != InputMode::None && app.input_mode != InputMode::MicroMenu {
                        for c in s.chars() {
                            app.text_input.insert(app.input_cursor_pos, c);
                            app.input_cursor_pos += 1;
                        }
                        if app.input_mode == InputMode::QuickSearch { app.update_quick_search_results(); }
                    }
                }
                Event::Mouse(_) => continue,
                Event::Key(key) => {
                    if app.input_mode == InputMode::PreviewPopup {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('o') => app.input_mode = InputMode::None,
                            KeyCode::Up | KeyCode::Char('k') => app.preview_scroll = app.preview_scroll.saturating_sub(1),
                            KeyCode::Down | KeyCode::Char('j') => app.preview_scroll = app.preview_scroll.saturating_add(1),
                            _ => {}
                        }
                    } else if app.input_mode == InputMode::Help {
                        match key.code {
                            KeyCode::Esc => app.input_mode = InputMode::None,
                            KeyCode::Up | KeyCode::Char('k') => app.help_scroll = app.help_scroll.saturating_sub(1),
                            KeyCode::Down | KeyCode::Char('j') => app.help_scroll = app.help_scroll.saturating_add(1),
                            _ => {}
                        }
                    } else if app.input_mode == InputMode::Bookmarks {
                        match key.code {
                            KeyCode::Esc => app.input_mode = InputMode::None,
                            KeyCode::Enter => app.enter(),
                            KeyCode::Down | KeyCode::Char('j') => {
                                let len = app.bookmarks.len();
                                if len > 0 {
                                    let i = app.bookmarks_state.selected().map(|i| (i + 1) % len).unwrap_or(0);
                                    app.bookmarks_state.select(Some(i));
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                let len = app.bookmarks.len();
                                if len > 0 {
                                    let i = app.bookmarks_state.selected().map(|i| if i == 0 { len - 1 } else { i - 1 }).unwrap_or(0);
                                    app.bookmarks_state.select(Some(i));
                                }
                            }
                            KeyCode::Char('D') | KeyCode::Delete => {
                                if let Some(i) = app.bookmarks_state.selected() {
                                    if i < app.bookmarks.len() {
                                        app.bookmarks.remove(i);
                                        app.save_bookmarks();
                                    }
                                }
                            }
                            _ => {}
                        }
                    } else if app.input_mode == InputMode::Drives {
                        match key.code {
                            KeyCode::Esc => app.input_mode = InputMode::None,
                            KeyCode::Enter => {
                                if let Some(i) = app.drives_state.selected() {
                                    if let Some((_, path_str)) = app.drives.get(i).cloned() {
                                        app.navigate_to(PathBuf::from(path_str));
                                        app.input_mode = InputMode::None;
                                    }
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                let i = match app.drives_state.selected() {
                                    Some(i) => if i >= app.drives.len().saturating_sub(1) { 0 } else { i + 1 },
                                    None => 0,
                                };
                                app.drives_state.select(Some(i));
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                let i = match app.drives_state.selected() {
                                    Some(i) => if i == 0 { app.drives.len().saturating_sub(1) } else { i - 1 },
                                    None => 0,
                                };
                                app.drives_state.select(Some(i));
                            }
                            _ => {}
                        }
                    } else if app.input_mode == InputMode::IdeSaveConfirm {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                app.micro_save_file();
                                if app.micro_current_file.is_some() { app.input_mode = InputMode::None; }
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') => {
                                app.input_mode = InputMode::None;
                            }
                            KeyCode::Esc => app.input_mode = InputMode::MicroMenu,
                            _ => {}
                        }
                    } else if app.input_mode == InputMode::IdePromptFileName {
                        match key.code {
                            KeyCode::Esc => app.input_mode = InputMode::MicroMenu,
                            KeyCode::Enter => {
                                let dir = app.active_pane().current_dir.clone();
                                let path = dir.join(&app.text_input);
                                app.micro_current_file = Some(path);
                                app.micro_save_file();
                                app.left_pane.refresh();
                                app.right_pane.refresh();
                                app.input_mode = InputMode::MicroMenu;
                            }
                            KeyCode::Char(c) => {
                                app.text_input.insert(app.input_cursor_pos, c);
                                app.input_cursor_pos += 1;
                            }
                            KeyCode::Backspace => {
                                if app.input_cursor_pos > 0 {
                                    app.text_input.remove(app.input_cursor_pos - 1);
                                    app.input_cursor_pos -= 1;
                                }
                            }
                            KeyCode::Left => app.input_cursor_pos = app.input_cursor_pos.saturating_sub(1),
                            KeyCode::Right => if app.input_cursor_pos < app.text_input.chars().count() { app.input_cursor_pos += 1; },
                            _ => {}
                        }
                    } else if app.input_mode != InputMode::None && app.input_mode != InputMode::MicroMenu && app.input_mode != InputMode::FileInfo && app.input_mode != InputMode::CustomScripts && app.input_mode != InputMode::OpenWith {
                         match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => app.input_mode = InputMode::None,
                            (KeyCode::Enter, _) => app.enter(),
                            (KeyCode::Char(c), _) => {
                                app.text_input.insert(app.input_cursor_pos, c);
                                app.input_cursor_pos += 1;
                                if app.input_mode == InputMode::QuickSearch { app.update_quick_search_results(); }
                            }
                            (KeyCode::Backspace, _) => {
                                if app.input_cursor_pos > 0 {
                                    app.text_input.remove(app.input_cursor_pos - 1);
                                    app.input_cursor_pos -= 1;
                                    if app.input_mode == InputMode::QuickSearch { app.update_quick_search_results(); }
                                }
                            }
                            (KeyCode::Left, _) => app.input_cursor_pos = app.input_cursor_pos.saturating_sub(1),
                            (KeyCode::Right, _) => if app.input_cursor_pos < app.text_input.chars().count() { app.input_cursor_pos += 1; },
                            _ => {}
                         }
                    } else if app.input_mode == InputMode::MicroMenu {
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) | (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                                if app.micro_is_dirty {
                                    app.input_mode = InputMode::IdeSaveConfirm;
                                } else {
                                    app.input_mode = InputMode::None;
                                }
                            }
                            (KeyCode::Char('s'), KeyModifiers::CONTROL) => app.micro_save_file(),
                            (KeyCode::Char('c'), KeyModifiers::CONTROL) => app.micro_copy(),
                            (KeyCode::Char('v'), KeyModifiers::CONTROL) => app.micro_paste(),
                            (KeyCode::Char('z'), KeyModifiers::CONTROL) => app.micro_undo(),
                            (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                                app.ide_focus = match app.ide_focus {
                                    IdeFocus::Editor => IdeFocus::ProjectTree,
                                    IdeFocus::ProjectTree => IdeFocus::Editor,
                                };
                            }

                            (KeyCode::Tab, mods) => {
                                if app.micro_show_autocomplete {
                                    if mods.contains(KeyModifiers::SHIFT) {
                                        app.previous();
                                    } else {
                                        app.next();
                                    }
                                }
                            }
                            
                            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                                if app.ide_focus == IdeFocus::Editor {
                                    app.micro_handle_input(c);
                                }
                            }
                            (KeyCode::Backspace, _) => {
                                if app.ide_focus == IdeFocus::Editor {
                                    if app.micro_select_start.is_some() {
                                        app.micro_delete_selection();
                                    } else if app.micro_cursor_x > 0 {
                                        app.micro_record_undo();
                                        
                                        let line_chars: Vec<char> = app.micro_editor_text[app.micro_cursor_y].chars().collect();
                                        let mut new_chars = line_chars.clone();
                                        
                                        let is_pair = if app.micro_cursor_x < line_chars.len() {
                                            let next_c = line_chars[app.micro_cursor_x];
                                            let cur_c = line_chars[app.micro_cursor_x - 1];
                                            matches!((cur_c, next_c), ('(', ')') | ('[', ']') | ('{', '}') | ('"', '"') | ('\'', '\''))
                                        } else { false };

                                        if is_pair {
                                            new_chars.remove(app.micro_cursor_x);
                                        }

                                        new_chars.remove(app.micro_cursor_x - 1);
                                        app.micro_editor_text[app.micro_cursor_y] = new_chars.into_iter().collect();
                                        app.micro_cursor_x -= 1;
                                    } else if app.micro_cursor_y > 0 {
                                        app.micro_record_undo();
                                        let line = app.micro_editor_text.remove(app.micro_cursor_y);
                                        app.micro_cursor_y -= 1;
                                        app.micro_cursor_x = app.micro_editor_text[app.micro_cursor_y].chars().count();
                                        app.micro_editor_text[app.micro_cursor_y].push_str(&line);
                                    }
                                    app.micro_update_autocomplete();
                                    app.micro_check_scrolling();
                                }
                            }
                            (KeyCode::Enter, _) => {
                                if app.micro_show_autocomplete {
                                    app.micro_apply_suggestion();
                                } else if app.ide_focus == IdeFocus::Editor {
                                    app.micro_record_undo();
                                    if app.micro_select_start.is_some() { app.micro_delete_selection(); }
                                    
                                    let cursor_y = app.micro_cursor_y;
                                    let line_chars: Vec<char> = app.micro_editor_text[cursor_y].chars().collect();
                                    let indent_count = line_chars.iter().take_while(|c| c.is_whitespace()).count();
                                    let mut indent = " ".repeat(indent_count);
                                    
                                    if app.micro_editor_text[cursor_y].trim_end().ends_with('{') {
                                        indent.push_str("    ");
                                    }

                                    let cursor_x = app.micro_cursor_x;
                                    let prefix: String = line_chars[..cursor_x.min(line_chars.len())].iter().collect();
                                    let suffix: String = line_chars[cursor_x.min(line_chars.len())..].iter().collect();
                                    
                                    app.micro_editor_text[cursor_y] = prefix;
                                    app.micro_cursor_y += 1;
                                    app.micro_editor_text.insert(app.micro_cursor_y, format!("{}{}", indent, suffix));
                                    app.micro_cursor_x = indent.chars().count();
                                    
                                    app.micro_show_autocomplete = false;
                                    app.micro_check_scrolling();
                                } else if app.ide_focus == IdeFocus::ProjectTree {
                                    if let Some(i) = app.micro_project_state.selected() {
                                        if let Some(path) = app.micro_project_tree.get(i).cloned() {
                                            app.micro_open_file(path);
                                            app.ide_focus = IdeFocus::Editor;
                                        }
                                    }
                                }
                            }
                            (KeyCode::Up, mods) => {
                                if app.micro_show_autocomplete { app.previous(); }
                                else {
                                    match app.ide_focus {
                                        IdeFocus::Editor => {
                                            if mods.contains(KeyModifiers::SHIFT) && app.micro_select_start.is_none() {
                                                app.micro_select_start = Some((app.micro_cursor_y, app.micro_cursor_x));
                                            } else if !mods.contains(KeyModifiers::SHIFT) {
                                                app.micro_select_start = None;
                                            }
                                            app.micro_cursor_y = app.micro_cursor_y.saturating_sub(1);
                                            app.micro_cursor_x = app.micro_cursor_x.min(app.micro_editor_text[app.micro_cursor_y].chars().count());
                                            app.micro_check_scrolling();
                                        }
                                        IdeFocus::ProjectTree => app.previous(),
                                    }
                                }
                            }
                            (KeyCode::Down, mods) => {
                                if app.micro_show_autocomplete { app.next(); }
                                else {
                                    match app.ide_focus {
                                        IdeFocus::Editor => {
                                            if app.micro_cursor_y < app.micro_editor_text.len().saturating_sub(1) {
                                                if mods.contains(KeyModifiers::SHIFT) && app.micro_select_start.is_none() {
                                                    app.micro_select_start = Some((app.micro_cursor_y, app.micro_cursor_x));
                                                } else if !mods.contains(KeyModifiers::SHIFT) {
                                                    app.micro_select_start = None;
                                                }
                                                app.micro_cursor_y += 1;
                                                app.micro_cursor_x = app.micro_cursor_x.min(app.micro_editor_text[app.micro_cursor_y].chars().count());
                                            }
                                            app.micro_check_scrolling();
                                        }
                                        IdeFocus::ProjectTree => app.next(),
                                    }
                                }
                            }
                            (KeyCode::Left, mods) => {
                                if app.ide_focus == IdeFocus::Editor {
                                    if mods.contains(KeyModifiers::SHIFT) && app.micro_select_start.is_none() {
                                        app.micro_select_start = Some((app.micro_cursor_y, app.micro_cursor_x));
                                    } else if !mods.contains(KeyModifiers::SHIFT) {
                                        app.micro_select_start = None;
                                    }

                                    if mods.contains(KeyModifiers::CONTROL) {
                                        let line = &app.micro_editor_text[app.micro_cursor_y];
                                        let chars: Vec<char> = line.chars().collect();
                                        let mut new_x = app.micro_cursor_x;
                                        while new_x > 0 && !chars[new_x - 1].is_alphanumeric() {
                                            new_x -= 1;
                                        }
                                        while new_x > 0 && chars[new_x - 1].is_alphanumeric() {
                                            new_x -= 1;
                                        }
                                        app.micro_cursor_x = new_x;
                                    } else if app.micro_cursor_x > 0 {
                                        app.micro_cursor_x -= 1;
                                    } else if app.micro_cursor_y > 0 {
                                        app.micro_cursor_y -= 1;
                                        app.micro_cursor_x = app.micro_editor_text[app.micro_cursor_y].chars().count();
                                    }
                                    app.micro_check_scrolling();
                                } else if app.ide_focus == IdeFocus::Editor {
                                    app.ide_focus = IdeFocus::ProjectTree;
                                }
                            }
                            (KeyCode::Right, mods) => {
                                if app.ide_focus == IdeFocus::Editor {
                                    if mods.contains(KeyModifiers::SHIFT) && app.micro_select_start.is_none() {
                                        app.micro_select_start = Some((app.micro_cursor_y, app.micro_cursor_x));
                                    } else if !mods.contains(KeyModifiers::SHIFT) {
                                        app.micro_select_start = None;
                                    }

                                    let line_width = app.micro_editor_text[app.micro_cursor_y].chars().count();
                                    if mods.contains(KeyModifiers::CONTROL) {
                                        let line = &app.micro_editor_text[app.micro_cursor_y];
                                        let chars: Vec<char> = line.chars().collect();
                                        let mut new_x = app.micro_cursor_x;
                                        while new_x < line_width && !chars[new_x].is_alphanumeric() {
                                            new_x += 1;
                                        }
                                        while new_x < line_width && chars[new_x].is_alphanumeric() {
                                            new_x += 1;
                                        }
                                        app.micro_cursor_x = new_x;
                                    } else if app.micro_cursor_x < line_width {
                                        app.micro_cursor_x += 1;
                                    } else if app.micro_cursor_y < app.micro_editor_text.len().saturating_sub(1) {
                                        app.micro_cursor_y += 1;
                                        app.micro_cursor_x = 0;
                                    }
                                    app.micro_check_scrolling();
                                } else if app.ide_focus == IdeFocus::ProjectTree {
                                    app.ide_focus = IdeFocus::Editor;
                                }
                            }
                            _ => {}
                        }
                    } else if app.is_shell_mode {
                        match (key.code, key.modifiers) {
                            (KeyCode::Esc, _) => {
                                app.is_shell_mode = false;
                                app.input_cursor_pos = 0;
                            }
                            (KeyCode::Enter, _) => app.execute_shell_command(),
                            (KeyCode::Char('k'), KeyModifiers::CONTROL) => app.shell_output.lock().unwrap().clear(),
                            (KeyCode::Tab, _) if !app.shell_suggestion.is_empty() => {
                                app.shell_input.push_str(&app.shell_suggestion);
                                app.shell_suggestion.clear();
                                app.input_cursor_pos = app.shell_input.chars().count();
                                app.shell_update_suggestion();
                            }
                            (KeyCode::Up, mods) => {
                                if mods.contains(KeyModifiers::CONTROL) {
                                    app.shell_scroll = app.shell_scroll.saturating_add(1);
                                } else {
                                    app.shell_history_up();
                                }
                            }
                            (KeyCode::Down, mods) => {
                                if mods.contains(KeyModifiers::CONTROL) {
                                    app.shell_scroll = app.shell_scroll.saturating_sub(1);
                                } else {
                                    app.shell_history_down();
                                }
                            }
                            (KeyCode::Left, _) => {
                                app.input_cursor_pos = app.input_cursor_pos.saturating_sub(1);
                            }
                            (KeyCode::Right, _) => {
                                if app.input_cursor_pos < app.shell_input.chars().count() {
                                    app.input_cursor_pos += 1;
                                } else if !app.shell_suggestion.is_empty() {
                                    app.shell_input.push_str(&app.shell_suggestion);
                                    app.shell_suggestion.clear();
                                    app.input_cursor_pos = app.shell_input.chars().count();
                                    app.shell_update_suggestion();
                                }
                            }
                            (KeyCode::Backspace, _) => {
                                if app.input_cursor_pos > 0 {
                                    let mut chars: Vec<char> = app.shell_input.chars().collect();
                                    chars.remove(app.input_cursor_pos - 1);
                                    app.shell_input = chars.into_iter().collect();
                                    app.input_cursor_pos -= 1;
                                    app.shell_update_suggestion();
                                }
                            }
                            (KeyCode::Char(c), _) => {
                                let mut chars: Vec<char> = app.shell_input.chars().collect();
                                chars.insert(app.input_cursor_pos, c);
                                app.shell_input = chars.into_iter().collect();
                                app.input_cursor_pos += 1;
                                app.shell_update_suggestion();
                            }
                            _ => {}
                        }
        } else if app.is_searching {
            match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => {
                    app.is_searching = false;
                    app.input_cursor_pos = 0;
                }
                (KeyCode::Enter, _) => app.handle_search_confirm(),
                (KeyCode::Char('\\'), _) => app.jump_to_file_dir(),
                (KeyCode::Left, _) => {
                    app.input_cursor_pos = app.input_cursor_pos.saturating_sub(1);
                }
                (KeyCode::Right, _) => {
                    if app.input_cursor_pos < app.search_query.chars().count() {
                        app.input_cursor_pos += 1;
                    }
                }
                (KeyCode::Backspace, _) => {
                    if app.input_cursor_pos > 0 {
                        let mut chars: Vec<char> = app.search_query.chars().collect();
                        chars.remove(app.input_cursor_pos - 1);
                        app.search_query = chars.into_iter().collect();
                        app.input_cursor_pos -= 1;
                        app.search_debounce_task = Some(Instant::now());
                    }
                }
                _ if app.check_action(key, "open_ide") => { app.open_micro_menu(); }
                (KeyCode::Down, _) => app.next(),
                (KeyCode::Up, _) => app.previous(),
                (KeyCode::Char(c), _) => {
                    let mut chars: Vec<char> = app.search_query.chars().collect();
                    chars.insert(app.input_cursor_pos, c);
                    app.search_query = chars.into_iter().collect();
                    app.input_cursor_pos += 1;
                    app.search_debounce_task = Some(Instant::now());
                }
                _ => {}
            }
                    } else if app.input_mode == InputMode::FileInfo {
                        if key.code == KeyCode::Esc { app.input_mode = InputMode::None; }
                    } else if app.input_mode == InputMode::CustomScripts {
                        match key.code {
                            KeyCode::Esc => app.input_mode = InputMode::None,
                            KeyCode::Enter => app.execute_custom_script(),
                            KeyCode::Down | KeyCode::Char('j') => app.next(), 
                            KeyCode::Up | KeyCode::Char('k') => app.previous(),
                            _ => {}
                        }
                    } else if app.input_mode == InputMode::OpenWith {
                        match key.code {
                            KeyCode::Esc => app.input_mode = InputMode::None,
                            KeyCode::Enter => app.enter(),
                            KeyCode::Down | KeyCode::Char('j') => app.next(), 
                            KeyCode::Up | KeyCode::Char('k') => app.previous(),
                            _ => {}
                        }
                    } else {
                        if app.check_action(key, "quit") { return Ok(()); }
                        else if app.check_action(key, "sync_panes") { app.sync_panes(); }
                        else if app.check_action(key, "toggle_hidden") {
                            app.active_pane().show_hidden = !app.active_pane().show_hidden;
                            let msg = format!("Show hidden: {}", app.active_pane().show_hidden);
                            app.active_pane().refresh();
                            app.notify(&msg);
                        }
                        else if app.check_action(key, "quick_search") {
                            app.input_mode = InputMode::QuickSearch;
                            app.text_input.clear();
                            app.input_cursor_pos = 0;
                            app.quick_search_matches.clear();
                            app.notify("Quick search active");
                        }
                        else if app.check_action(key, "toggle_selection") {
                            app.active_pane().toggle_selection();
                            app.active_pane().anchor_index = app.active_pane().state.selected();
                        }
                        else if app.check_action(key, "select_all") {
                            app.active_pane().select_all_toggle();
                            app.notify("Select all toggled");
                        }
                        else if app.check_action(key, "switch_pane") { app.active_left = !app.active_left; }
                        else if app.check_action(key, "copy") { 
                            app.copy_item(false); 
                        }
                        else if app.check_action(key, "cut") { app.copy_item(true); }
                        else if app.check_action(key, "paste") { app.paste_item(); }
                        else if app.check_action(key, "open_term") { app.open_external_term(); }
                        else if app.check_action(key, "create_folder") { 
                            app.input_mode = InputMode::CreateFolder; 
                            app.text_input.clear();
                            app.input_cursor_pos = 0;
                        }
                        else if app.check_action(key, "create_file") { 
                            app.input_mode = InputMode::CreateFile; 
                            app.text_input.clear();
                            app.input_cursor_pos = 0;
                        }
                        else if app.check_action(key, "open_with") {
                            app.load_open_with_apps();
                            app.open_with_state.select(Some(0));
                            app.input_mode = InputMode::OpenWith;
                        }
                        else if app.check_action(key, "copy_path") { app.copy_path_to_clipboard(); }
                        else if app.check_action(key, "delete") { app.input_mode = InputMode::DeleteConfirm; }
                        else if app.check_action(key, "rename") {
                            if let Some(i) = app.active_pane().state.selected() {
                                if let Some(p) = app.active_pane().items.get(i) {
                                    app.text_input = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                                    app.input_cursor_pos = app.text_input.chars().count();
                                    app.input_mode = InputMode::Rename;
                                }
                            }
                        }
                        else if app.check_action(key, "search") {
                            app.is_searching = true;
                            app.zoxide_mode = false;
                            app.search_query.clear();
                            app.input_cursor_pos = 0;
                            app.spawn_search();
                        }
                        else if app.check_action(key, "zoxide") { 
                            app.is_searching = true; 
                            app.zoxide_mode = true; 
                            app.search_query.clear(); 
                            app.input_cursor_pos = 0;
                            app.spawn_search(); 
                        }
                        else if app.check_action(key, "shell") { 
                            app.is_shell_mode = true; 
                            app.shell_input.clear(); 
                            app.input_cursor_pos = 0;
                            app.shell_scroll = 0;
                            app.shell_history_index = app.shell_history.len(); 
                            app.shell_update_suggestion();
                        }
                        else if app.check_action(key, "archive") { app.archive_operation(); }
                        else if app.check_action(key, "sort") { app.toggle_sort(); }
                        else if app.check_action(key, "open_scripts") { app.open_custom_scripts(); }
                        else if app.check_action(key, "help") { app.input_mode = InputMode::Help; }
                        else if app.check_action(key, "info") { app.show_file_info(); }
                        else if app.check_action(key, "open_ide") { app.open_micro_menu(); }
                        else if app.check_action(key, "bookmarks") { app.input_mode = InputMode::Bookmarks; }
                        else if app.check_action(key, "drives") { 
                            app.input_mode = InputMode::Drives; 
                            app.load_and_mount_drives(); 
                        }
                        else if app.check_action(key, "add_bookmark") {
                            let path = {
                                let pane = app.active_pane();
                                pane.state.selected().and_then(|i| pane.items.get(i).cloned()).unwrap_or_else(|| pane.current_dir.clone())
                            };
                            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            app.bookmarks.push((name.clone(), path));
                            app.save_bookmarks();
                            app.notify(&format!("Added to bookmarks: {}", name));
                        }
                        else if app.check_action(key, "preview_popup") { app.input_mode = InputMode::PreviewPopup; }
                        else if app.check_action(key, "refresh") { 
                            let _ = terminal.clear();
                            app.left_pane.refresh();
                            app.right_pane.refresh();
                        }
                        else {
                            match (key.code, key.modifiers) {
                                (KeyCode::Char('h'), KeyModifiers::ALT) | (KeyCode::Left, KeyModifiers::ALT) => app.go_back(),
                                (KeyCode::Char('l'), KeyModifiers::ALT) | (KeyCode::Right, KeyModifiers::ALT) => app.go_forward(),
                                (KeyCode::Char('j'), KeyModifiers::NONE) | (KeyCode::Down, KeyModifiers::NONE) | (KeyCode::Down, KeyModifiers::SHIFT) => {
                                    let mods = key.modifiers;
                                    let old_idx = app.active_pane().state.selected().unwrap_or(0);
                                    if !mods.contains(KeyModifiers::SHIFT) {
                                        app.active_pane().anchor_index = None;
                                    } else if app.active_pane().anchor_index.is_none() {
                                        app.active_pane().anchor_index = Some(old_idx);
                                    }
                                    
                                    app.next();
                                    let new_idx = app.active_pane().state.selected().unwrap_or(0);
                                    
                                    if mods.contains(KeyModifiers::SHIFT) {
                                        app.active_pane().update_shift_selection(new_idx);
                                    }
                                }
                                (KeyCode::Char('k'), KeyModifiers::NONE) | (KeyCode::Up, KeyModifiers::NONE) | (KeyCode::Up, KeyModifiers::SHIFT) => {
                                    let mods = key.modifiers;
                                    let old_idx = app.active_pane().state.selected().unwrap_or(0);
                                    if !mods.contains(KeyModifiers::SHIFT) {
                                        app.active_pane().anchor_index = None;
                                    } else if app.active_pane().anchor_index.is_none() {
                                        app.active_pane().anchor_index = Some(old_idx);
                                    }
                                    
                                    app.previous();
                                    let new_idx = app.active_pane().state.selected().unwrap_or(0);
                                    
                                    if mods.contains(KeyModifiers::SHIFT) {
                                        app.active_pane().update_shift_selection(new_idx);
                                    }
                                }
                                (KeyCode::PageDown, _) | (KeyCode::Char('G'), _) => {
                                    app.jump_to_end();
                                }
                                (KeyCode::PageUp, _) | (KeyCode::Char('g'), _) => {
                                    app.jump_to_start();
                                }
                                (KeyCode::Enter, _) | (KeyCode::Right, _) | (KeyCode::Char('l'), _) => app.enter(),
                                (KeyCode::Backspace, _) | (KeyCode::Left, _) | (KeyCode::Char('h'), _) => app.backward(),
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let colors = app.get_colors();
    let (bg_clr, text_clr, dim_clr, teal_clr, green_clr, grey_clr, accent_clr, hint_bg_clr, _) = colors;
    
    let size = f.size();
    
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0), Constraint::Length(1)])
        .split(size);

    f.render_widget(Clear, size); 

    let dir_display = app.active_pane().current_dir.display().to_string();
    let max_path_len = (size.width as usize).saturating_sub(20);
    let truncated_path = if dir_display.len() > max_path_len {
        format!("...{}", &dir_display[dir_display.len().saturating_sub(max_path_len-3)..])
    } else {
        dir_display
    };
    
    //let logo_text_mid = Span::styled("   ƒ--x  ==> ", Style::default().fg(green_clr).add_modifier(Modifier::BOLD));
    let path_text_ka = Span::styled(" ka", Style::default().fg(text_clr).add_modifier(Modifier::BOLD));
    let path_text_mid = Span::styled(format!(" | {}", truncated_path), Style::default().fg(text_clr));
    
    let notification_span = if !app.notification.is_empty() {
        format!(" | 󰀦 {} ", app.notification)
    } else {
        "".to_string()
    };
    
    let top_block = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(0), Constraint::Min(0)])
        .split(chunks[0]);

    let top_line = Line::from(vec![path_text_ka, path_text_mid, Span::styled(notification_span, Style::default().fg(accent_clr))]);
    f.render_widget(Paragraph::new(top_line).wrap(Wrap { trim: true }), top_block[1]);

    let main_area = chunks[1];
    let split_x = (main_area.width as f32 * (app.pane_split_percent as f32 / 100.0)) as u16;
    
    let left_area = Rect::new(main_area.x, main_area.y, split_x, main_area.height).inner(&Margin { vertical: 1, horizontal: 2 });
    let right_area = Rect::new(main_area.x + split_x, main_area.y, main_area.width.saturating_sub(split_x), main_area.height).inner(&Margin { vertical: 1, horizontal: 2 });
    
    render_pane(f, left_area, &mut app.left_pane, app.active_left, colors);
    render_pane(f, right_area, &mut app.right_pane, !app.active_left, colors);

    if app.input_mode == InputMode::Bookmarks {
        let area = centered_rect(50, 50, size);
        f.render_widget(Clear, area);
        let items: Vec<ListItem> = app.bookmarks.iter().map(|(name, p)| {
            ListItem::new(format!(" 󰉋 {} ({})", name, p.display()))
        }).collect();
        f.render_stateful_widget(List::new(items)
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" 󰃃 Bookmarks (Enter: Go, Del: Delete) ").border_style(Style::default().fg(teal_clr)))
            .highlight_style(Style::default().bg(hint_bg_clr).add_modifier(Modifier::BOLD)).highlight_symbol(">> "), area, &mut app.bookmarks_state);
    }

    if app.input_mode == InputMode::Drives {
        let area = centered_rect(50, 50, size);
        f.render_widget(Clear, area);
        let items: Vec<ListItem> = app.drives.iter().map(|(label, _)| {
            ListItem::new(format!(" {}", label))
        }).collect();
        f.render_stateful_widget(
            List::new(items)
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" 󰋊 Mounted Drives ").border_style(Style::default().fg(teal_clr)))
                .highlight_style(Style::default().bg(hint_bg_clr).add_modifier(Modifier::BOLD))
                .highlight_symbol(">> "),
            area,
            &mut app.drives_state
        );
    }

    if app.input_mode == InputMode::PreviewPopup {
        let area = centered_rect(80, 80, size);
        f.render_widget(Clear, area);
        let preview_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" 󰈈 File Preview ")
            .border_style(Style::default().fg(teal_clr));
        f.render_widget(Paragraph::new(app.preview_content.as_str())
            .block(preview_block)
            .wrap(Wrap { trim: false })
            .scroll((app.preview_scroll, 0))
            .style(Style::default().fg(text_clr)), area);
    }

    if app.input_mode == InputMode::IdePromptFileName {
        let area = centered_rect(40, 10, size);
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" 󰈔 Name for new file ")
            .border_style(Style::default().fg(accent_clr));
        
        let content = if app.text_input.is_empty() { " " } else { app.text_input.as_str() };
        f.render_widget(Paragraph::new(content).block(block).style(Style::default().fg(text_clr)), area);
        let cursor_x = UnicodeWidthStr::width(app.text_input.chars().take(app.input_cursor_pos).collect::<String>().as_str()) as u16;
        f.set_cursor(area.x + cursor_x + 1, area.y + 1);
    }

    if app.input_mode == InputMode::MicroMenu || app.input_mode == InputMode::IdeSaveConfirm {
        let area = centered_rect(98, 95, size);
        f.render_widget(Clear, area);
        
        let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" 󰅩 ka-IDE ").border_style(Style::default().fg(teal_clr));
        f.render_widget(block, area);

        let micro_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(80),
            ])
            .split(area.inner(&Margin { vertical: 1, horizontal: 1 }));

        let project_rect = micro_layout[0];
        let editor_rect = micro_layout[1];

        app.micro_rects = vec![project_rect, editor_rect];

        let tree_border_style = if app.ide_focus == IdeFocus::ProjectTree { 
            Style::default().fg(accent_clr).add_modifier(Modifier::BOLD) 
        } else { 
            Style::default().fg(dim_clr) 
        };
        
        let tree_items: Vec<ListItem> = app.micro_project_tree.iter().map(|p| {
            let (icon, color) = get_icon(p, teal_clr, green_clr, grey_clr, text_clr);
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            
            let item_style = if Some(p) == app.micro_current_file.as_ref() {
                Style::default().fg(accent_clr).bg(hint_bg_clr)
            } else {
                Style::default().fg(color)
            };
            
            ListItem::new(format!(" {} {}", icon, name)).style(item_style)
        }).collect();

        f.render_stateful_widget(
            List::new(tree_items)
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" 󰙅 Project ").border_style(tree_border_style))
                .highlight_style(Style::default().bg(hint_bg_clr).add_modifier(Modifier::BOLD))
                .highlight_symbol("󰁔 "), 
            project_rect, 
            &mut app.micro_project_state
        );

        let current_ext = app.micro_current_file.as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let editor_border_style = if app.ide_focus == IdeFocus::Editor { 
            Style::default().fg(accent_clr).add_modifier(Modifier::BOLD) 
        } else { 
            Style::default().fg(dim_clr) 
        };
        
        let file_name_display = app.micro_current_file.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("untitled");
        let editor_title = if app.micro_is_dirty { format!(" 󰷈 {}* ", file_name_display) } else { format!(" 󰷈 {} ", file_name_display) };

        let editor_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(editor_rect.inner(&Margin { vertical: 1, horizontal: 1 }));

        let spans_lines: Vec<Line> = app.micro_editor_text.iter()
            .enumerate()
            .skip(app.micro_scroll_y)
            .take(editor_chunks[0].height as usize)
            .map(|(idx, line)| {
                let line_num = format!("{:3} ", idx + 1);
                let line_content = generic_syntax_highlight(line, current_ext, idx, app.micro_select_start, (app.micro_cursor_y, app.micro_cursor_x), colors);
                
                let mut spans = vec![Span::styled(line_num, Style::default().fg(dim_clr))];
                let mut current_visual_pos = 0;
                let viewport_width = editor_rect.width.saturating_sub(6) as usize;

                for span in line_content.spans {
                    let span_width = UnicodeWidthStr::width(span.content.as_ref());
                    
                    if current_visual_pos + span_width > app.micro_scroll_x {
                        let slice_start = app.micro_scroll_x.saturating_sub(current_visual_pos);
                        let available_width = viewport_width.saturating_sub(current_visual_pos.saturating_sub(app.micro_scroll_x));
                        
                        let text_slice = get_visible_slice(span.content.as_ref(), slice_start, available_width);
                        if !text_slice.is_empty() {
                            spans.push(Span::styled(text_slice, span.style));
                        }
                    }
                    current_visual_pos += span_width;
                    if current_visual_pos >= app.micro_scroll_x + viewport_width { break; }
                }
                Line::from(spans)
            })
    .collect();

        f.render_widget(
            Paragraph::new(spans_lines)
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(editor_title).border_style(editor_border_style))
                .style(Style::default().fg(text_clr)), 
            editor_rect
        );

        let mode_str = if app.ide_focus == IdeFocus::Editor { " EDITOR " } else { " TREE " };
        let status_left = Span::styled(mode_str, Style::default().bg(teal_clr).fg(bg_clr).add_modifier(Modifier::BOLD));
        let status_mid = Span::styled(format!("  Ln {}, Col {}  ", app.micro_cursor_y + 1, app.micro_cursor_x + 1), Style::default().fg(grey_clr));
        let status_right = Span::styled(format!("  {}  ", current_ext.to_uppercase()), Style::default().fg(accent_clr));
        
        f.render_widget(Paragraph::new(Line::from(vec![status_left, status_mid, status_right])).alignment(ratatui::layout::Alignment::Right), editor_chunks[1]);
        
        if app.ide_focus == IdeFocus::Editor && app.micro_cursor_y >= app.micro_scroll_y {
            let line = &app.micro_editor_text[app.micro_cursor_y];
            let cursor_visual_x = UnicodeWidthStr::width(line.chars().take(app.micro_cursor_x).collect::<String>().as_str());
            if cursor_visual_x >= app.micro_scroll_x {
                f.set_cursor(
                    editor_rect.x + 5 + (cursor_visual_x - app.micro_scroll_x) as u16, 
                    editor_rect.y + 1 + (app.micro_cursor_y - app.micro_scroll_y) as u16
                );
            }
        }

        if app.micro_show_autocomplete {
            let line = &app.micro_editor_text[app.micro_cursor_y];
            let cursor_visual_x = UnicodeWidthStr::width(line.chars().take(app.micro_cursor_x).collect::<String>().as_str());
            let draw_x = if cursor_visual_x >= app.micro_scroll_x {
                 editor_rect.x + 5 + (cursor_visual_x - app.micro_scroll_x) as u16
            } else {
                 editor_rect.x + 5
            };
            let draw_y = editor_rect.y + 1 + (app.micro_cursor_y.saturating_sub(app.micro_scroll_y)) as u16 + 1;

            let sug_area = Rect::new(draw_x, draw_y, 30, (app.micro_suggestions.len().min(10) as u16) + 2);
            let sug_area = sug_area.intersection(editor_rect);
            f.render_widget(Clear, sug_area);
            let sug_items: Vec<ListItem> = app.micro_suggestions.iter().enumerate().map(|(i, s)| {
                let style = if i == app.micro_suggestion_idx { 
                    Style::default().bg(accent_clr).fg(bg_clr).add_modifier(Modifier::BOLD) 
                } else { 
                    Style::default().bg(hint_bg_clr).fg(text_clr) 
                };
                ListItem::new(format!(" 󰌵 {}", s)).style(style)
            }).collect();
            f.render_stateful_widget(List::new(sug_items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" 󰘦 Hints ").border_style(Style::default().fg(accent_clr))), sug_area, &mut ListState::default());
        }

        if app.input_mode == InputMode::IdeSaveConfirm {
            let confirm_area = centered_rect(40, 10, size);
            f.render_widget(Clear, confirm_area);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" 󰆓 Save changes? ")
                .border_style(Style::default().fg(accent_clr).add_modifier(Modifier::BOLD));
            let text = Paragraph::new("\n  File modified. Save? (y/n)")
                .block(block)
                .alignment(ratatui::layout::Alignment::Center)
                .style(Style::default().fg(text_clr));
            f.render_widget(text, confirm_area);
        }
    }

    if !app.clipboard_items.is_empty() {
        let label = if app.is_cut { "󰪹 Move" } else { "󰆏 Copy" };
        let clip_text = if app.clipboard_items.len() == 1 {
            format!(" {} : {} ", label, app.clipboard_items[0].file_name().unwrap_or_default().to_string_lossy())
        } else {
            format!(" {} : {} items ", label, app.clipboard_items.len())
        };
        let clip_area = Rect::new(size.width.saturating_sub(40), 0, 40, 1);
        f.render_widget(Paragraph::new(clip_text).style(Style::default().fg(accent_clr).bg(bg_clr)), clip_area);
    }

    {
        let p = app.task_progress.lock().unwrap();
        if p.active {
            let area = centered_rect(60, 10, size);
            f.render_widget(Clear, area);
            let gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(p.message.as_str()))
                .gauge_style(Style::default().fg(green_clr).bg(bg_clr))
                .percent(p.percentage);
            f.render_widget(gauge, area);
        }
    }

    let hint_str = format!(
        " {} Hidden | {} Drives | {} AddBookmark | {} OpenWith | {} Term | {} Rename | {} Sync | {} Copy | {} Cut | {} Paste | {} Quit ",
        app.get_bind_text("toggle_hidden"),
        app.get_bind_text("drives"),
        app.get_bind_text("add_bookmark"),
        app.get_bind_text("open_with"),
        app.get_bind_text("open_term"),
        app.get_bind_text("rename"),
        app.get_bind_text("sync_panes"),
        app.get_bind_text("copy"),
        app.get_bind_text("cut"),
        app.get_bind_text("paste"),
        app.get_bind_text("quit")
    );
    f.render_widget(Paragraph::new(hint_str).style(Style::default().fg(dim_clr)), chunks[2]);

    if app.input_mode == InputMode::Help {
        let area = centered_rect(80, 90, size);
        f.render_widget(Clear, area);
        
        let help_rows = vec![
            Line::from(vec![Span::styled("--- 󰴔 Global General ---", Style::default().fg(accent_clr).add_modifier(Modifier::BOLD))]),
            Line::from("  Esc       : Close current menu/modal"),
            Line::from(format!("  {}         : Exit application", app.get_bind_text("quit"))),
            Line::from(format!("  {}         : Toggle active pane (Left/Right)", app.get_bind_text("switch_pane"))),
            Line::from(format!("  {}         : Toggle hidden files", app.get_bind_text("toggle_hidden"))),
            Line::from(format!("  {}         : Quick search in current directory (Enter to cycle)", app.get_bind_text("quick_search"))),
            Line::from(format!("  {}         : Open this help menu", app.get_bind_text("help"))),
            Line::from(format!("  {}         : Clear screen & Refresh panes", app.get_bind_text("refresh"))),
            Line::from(""),
            Line::from(vec![Span::styled("--- 󰉖 File Operations ---", Style::default().fg(teal_clr).add_modifier(Modifier::BOLD))]),
            Line::from("  Enter / l : Open directory or file with default app"),
            Line::from("  Bksp / h  : Go to parent directory"),
            Line::from(format!("  {}         : Create new empty file", app.get_bind_text("create_file"))),
            Line::from(format!("  {}         : Create new folder", app.get_bind_text("create_folder"))),
            Line::from(format!("  {}         : Rename selected item", app.get_bind_text("rename"))),
            Line::from(format!("  {}         : Delete selected items to TRASH", app.get_bind_text("delete"))),
            Line::from(format!("  {}         : Show detailed file info", app.get_bind_text("info"))),
            Line::from(format!("  {}         : Sync path of inactive pane with active one", app.get_bind_text("sync_panes"))),
            Line::from("  Alt+H / L : Navigate directory history (Back/Forward)"),
            Line::from(""),
            Line::from(vec![Span::styled("--- 󰆏 Clipboard & Select ---", Style::default().fg(green_clr).add_modifier(Modifier::BOLD))]),
            Line::from(format!("  {}         : Toggle selection of current item", app.get_bind_text("toggle_selection"))),
            Line::from("  Shift+Up/Dn: Multi-select items"),
            Line::from(format!("  {}         : Select/Deselect all items", app.get_bind_text("select_all"))),
            Line::from(format!("  {}         : Copy selected items to clipboard", app.get_bind_text("copy"))),
            Line::from(format!("  {}         : Cut selected items to clipboard", app.get_bind_text("cut"))),
            Line::from(format!("  {}         : Paste items from clipboard", app.get_bind_text("paste"))),
            Line::from(format!("  {}         : Copy absolute path to system clipboard", app.get_bind_text("copy_path"))),
            Line::from(""),
            Line::from(vec![Span::styled("--- 󰀺 Tools & Advanced ---", Style::default().fg(accent_clr).add_modifier(Modifier::BOLD))]),
            Line::from(format!("  {}         : Fast search using 'fd'", app.get_bind_text("search"))),
            Line::from(format!("  {}         : Zoxide jump path search", app.get_bind_text("zoxide"))),
            Line::from(format!("  {}         : Quick terminal mode", app.get_bind_text("shell"))),
            Line::from(format!("  {}         : Open system default terminal emulator", app.get_bind_text("open_term"))),
            Line::from(format!("  {}         : Open file preview popup", app.get_bind_text("preview_popup"))),
            Line::from(format!("  {}         : Compress to .tar.xz or Decompress archive", app.get_bind_text("archive"))),
            Line::from(format!("  {}         : Show mounted drives panel (Auto-mounts)", app.get_bind_text("drives"))),
            Line::from(format!("  {}         : Open bookmarks panel", app.get_bind_text("bookmarks"))),
            Line::from(format!("  {}         : Add current item to bookmarks", app.get_bind_text("add_bookmark"))),
            Line::from(format!("  {}         : Open custom scripts menu (~/kaf)", app.get_bind_text("open_scripts"))),
            Line::from(format!("  {}         : Open file with specific app menu", app.get_bind_text("open_with"))),
            Line::from(format!("  {}         : Cycle sort mode (Name, Ext, Size, Date)", app.get_bind_text("sort"))),
            Line::from(""),
            Line::from(vec![Span::styled("--- 󰷈 Micro IDE Mode ---", Style::default().fg(green_clr).add_modifier(Modifier::BOLD))]),
            Line::from(format!("  {}         : Open IDE mode", app.get_bind_text("open_ide"))),
            Line::from("  Ctrl+a    : Toggle focus between Tree and Editor"),
            Line::from("  Ctrl+s    : Save current file"),
            Line::from("  Ctrl+z    : Undo last change"),
            Line::from("  Ctrl+c / v: Copy/Paste text"),
            Line::from("  Ctrl+q    : Close IDE mode"),
            Line::from("  Tab       : Select next autocomplete hint"),
            Line::from("  Shift+Tab : Select previous autocomplete hint"),
            Line::from("  Enter     : Apply autocomplete or New line with auto-indent"),
        ];

        f.render_widget(
            Paragraph::new(help_rows)
                .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" 󰞋 ka - Complete Keybindings ").border_style(Style::default().fg(teal_clr)))
                .scroll((app.help_scroll, 0))
                .wrap(Wrap { trim: false }), 
            area
        );
    }

    if app.input_mode == InputMode::FileInfo {
        let area = centered_rect(50, 45, size);
        f.render_widget(Clear, area);
        let info_text = app.file_info_data.join("\n");
        f.render_widget(Paragraph::new(info_text).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" 󰋗 File Stats ").border_style(Style::default().fg(green_clr))), area);
    }

    if app.input_mode == InputMode::OpenWith {
        let area = centered_rect(40, 50, size);
        f.render_widget(Clear, area);
        let items: Vec<ListItem> = app.open_with_apps.iter().map(|(label, _)| ListItem::new(label.clone())).collect();
        f.render_stateful_widget(List::new(items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" 󰀻 Open With ").border_style(Style::default().fg(teal_clr))).highlight_style(Style::default().bg(hint_bg_clr).add_modifier(Modifier::BOLD)).highlight_symbol(">> "), area, &mut app.open_with_state);
    }

    if app.input_mode == InputMode::CustomScripts {
        let area = centered_rect(60, 60, size);
        f.render_widget(Clear, area);
        let items: Vec<ListItem> = app.custom_scripts.iter().map(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            ListItem::new(format!(" 󱆃 {}", name))
        }).collect();
        f.render_stateful_widget(List::new(items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" 󰆍 Run Script ").border_style(Style::default().fg(green_clr))).highlight_style(Style::default().bg(hint_bg_clr)).highlight_symbol("> "), area, &mut app.scripts_state);
    }

    if app.is_searching {
        let area = centered_rect(60, 50, size);
        f.render_widget(Clear, area);
        let search_block = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).split(area);

        let title = if app.zoxide_mode { " 󰉖 Zoxide Path " } 
                    else { " 󰀺 Fast Search " };

        let display_text = if app.search_query.is_empty() { " " } else { app.search_query.as_str() };
        f.render_widget(
            Paragraph::new(display_text)
                .style(Style::default().fg(text_clr))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(title)
                        .border_style(Style::default().fg(if app.zoxide_mode { accent_clr } else { teal_clr }))
                ),
            search_block[0]
        );

        if app.input_mode == InputMode::Bookmarks {
            let items: Vec<ListItem> = app.bookmarks.iter().map(|(name, _)| {
                ListItem::new(format!(" 󰆣 {}", name)).style(Style::default().fg(text_clr))
            }).collect();
            f.render_stateful_widget(List::new(items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(dim_clr))).highlight_style(Style::default().bg(hint_bg_clr)).highlight_symbol(">> "), search_block[1], &mut app.bookmarks_state);
        } else {
            let results = app.search_results.lock().unwrap();
            let items: Vec<ListItem> = results.iter().map(|p| {
                let (icon, color) = get_icon(p, teal_clr, green_clr, grey_clr, text_clr);
                ListItem::new(format!("{} {}", icon, p.display())).style(Style::default().fg(color))
            }).collect();
            f.render_stateful_widget(List::new(items).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(dim_clr))).highlight_style(Style::default().bg(hint_bg_clr)).highlight_symbol(">> "), search_block[1], &mut app.search_state);
        }
        let cursor_x = UnicodeWidthStr::width(app.search_query.chars().take(app.input_cursor_pos).collect::<String>().as_str()) as u16;
        f.set_cursor(search_block[0].x + cursor_x + 1, search_block[0].y + 1);
    };

    if app.is_shell_mode {
        let area = centered_rect(85, 30, size);
        f.render_widget(Clear, area);
        let inner = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).split(area);
        
        let shell_input_rect = inner[0];

        let mut input_spans = vec![Span::styled("> ", Style::default().fg(green_clr))];
        let parts: Vec<&str> = app.shell_input.splitn(2, ' ').collect();
        if !parts.is_empty() {
            let cmd_color = if app.shell_cmd_valid { accent_clr } else { Color::Rgb(150, 50, 50) };
            input_spans.push(Span::styled(parts[0], Style::default().fg(cmd_color).add_modifier(Modifier::BOLD)));
            if parts.len() > 1 {
                input_spans.push(Span::styled(format!(" {}", parts[1]), Style::default().fg(text_clr)));
            }
        }
        if !app.shell_suggestion.is_empty() {
            input_spans.push(Span::styled(&app.shell_suggestion, Style::default().fg(dim_clr)));
        }

        f.render_widget(Paragraph::new(Line::from(input_spans))
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" 󰆍 Quick Terminal ").border_style(Style::default().fg(teal_clr))), shell_input_rect);
        
        let output_guard = app.shell_output.lock().unwrap();
        let out_log = output_guard.iter().rev().cloned().collect::<Vec<_>>().join("\n");
        f.render_widget(Paragraph::new(out_log)
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(dim_clr)))
            .style(Style::default().fg(text_clr))
            .scroll((app.shell_scroll, 0))
            .wrap(Wrap { trim: false }), inner[1]);

        let cursor_x = UnicodeWidthStr::width(app.shell_input.chars().take(app.input_cursor_pos).collect::<String>().as_str()) as u16;
        f.set_cursor(shell_input_rect.x + cursor_x + 3, shell_input_rect.y + 1);
    }

    if app.input_mode != InputMode::None && app.input_mode != InputMode::FileInfo && app.input_mode != InputMode::Help && app.input_mode != InputMode::CustomScripts && app.input_mode != InputMode::OpenWith && app.input_mode != InputMode::MicroMenu && app.input_mode != InputMode::PreviewPopup && app.input_mode != InputMode::IdeSaveConfirm && app.input_mode != InputMode::IdePromptFileName && app.input_mode != InputMode::Bookmarks && app.input_mode != InputMode::Drives {
        let area = centered_rect(50, 7, size);
        f.render_widget(Clear, area);
        
        let mut title = String::new();
        let mut border_color = dim_clr;
        
        match app.input_mode {
            InputMode::CreateFile => { border_color = green_clr; title = " 󰈔 New File ".to_string(); },
            InputMode::CreateFolder => { border_color = teal_clr; title = "  New Folder ".to_string(); },
            InputMode::DeleteConfirm => { border_color = Color::Rgb(140, 70, 70); title = " 󰆴 Confirm Delete (Trash)? ".to_string(); },
            InputMode::QuickSearch => {
                border_color = accent_clr;
                let info = if app.quick_search_matches.is_empty() {
                    "(no matches)".to_string()
                } else {
                    format!("({}/{})", app.quick_search_idx + 1, app.quick_search_matches.len())
                };
                title = format!(" 󰀺 Find In Dir {} ", info);
            },
            InputMode::Rename => { border_color = accent_clr; title = " 󰷈 Rename Item ".to_string(); },
            _ => {},
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(title)
            .border_style(Style::default().fg(border_color));
        
        let content = if app.input_mode == InputMode::DeleteConfirm {
            "Press [ENTER] to move to trash, [ESC] to cancel".to_string()
        } else if app.text_input.is_empty() {
            " ".to_string()
        } else {
            app.text_input.clone()
        };

        f.render_widget(Paragraph::new(content).block(block).style(Style::default().fg(text_clr)), area);
        
        if app.input_mode != InputMode::DeleteConfirm {
             let cursor_x = UnicodeWidthStr::width(app.text_input.chars().take(app.input_cursor_pos).collect::<String>().as_str()) as u16;
             let visual_cursor = cursor_x.min(47);
             f.set_cursor(area.x + visual_cursor + 1, area.y + 1);
        }
    }
}

fn render_pane(f: &mut Frame, area: Rect, pane: &mut Pane, is_active: bool, colors: (Color, Color, Color, Color, Color, Color, Color, Color, Color)) {
    let (_, text_clr, dim_clr, teal_clr, green_clr, grey_clr, accent_clr, hint_bg_clr, _) = colors;
    
    f.render_widget(Clear, area); 
    let header_style = Style::default().fg(accent_clr).add_modifier(Modifier::BOLD);
    let rows: Vec<Row> = pane.items.iter()
        .map(|p| {
            let (icon, color) = get_icon(p, teal_clr, green_clr, grey_clr, text_clr);
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            let size = if p.is_file() {
                format_size(fs::metadata(p).map(|m| m.len()).unwrap_or(0))
            } else {
                "Dir".to_string()
            };
            let meta = fs::metadata(p);
            let date = meta.as_ref().map(|m| m.modified().map(format_time).unwrap_or_default()).unwrap_or_default();
            
            let is_selected = pane.selected_items.contains(p);
            let style = if is_selected {
                Style::default().bg(hint_bg_clr)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(format!("{} {}", icon, name)).style(Style::default().fg(color)),
                Cell::from(size).style(Style::default().fg(text_clr)),
                Cell::from(date).style(Style::default().fg(text_clr)),
            ]).style(style)
        })
        .collect();
        
    let display_dir = pane.current_dir.to_string_lossy().to_string();
    let title = format!(" 󰴔 {} | {} ", display_dir, pane.sort_mode.to_str());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(Style::default().fg(if is_active { accent_clr } else { dim_clr }));

    let table = Table::new(rows, [
        Constraint::Min(0),
        Constraint::Length(10),
        Constraint::Length(19),
    ])
    .header(Row::new(vec![" 󰈚 Name", " 󰈄 Size", " 󰃭 Date"]).style(header_style))
    .block(block)
    .highlight_style(Style::default().bg(hint_bg_clr).add_modifier(Modifier::BOLD))
    .highlight_symbol("󰁔 ");

    f.render_stateful_widget(table, area, &mut pane.state);

    pane.offset = pane.state.offset();
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2)]).split(r);
    Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2)]).split(popup_layout[1])[1]
}

impl App {
    fn run<B: io::Write>(&mut self, terminal: &mut Terminal<CrosstermBackend<B>>) -> io::Result<()> {
        main_run_loop(terminal, self)
    }

}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let mut app = App::new();
    let res = app.run(&mut terminal);
    app.save_history();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    let last_dir = app.active_pane().current_dir.display().to_string();
    let _ = fs::write("/tmp/ka_last_dir", last_dir);
    res?;
    Ok(())
}