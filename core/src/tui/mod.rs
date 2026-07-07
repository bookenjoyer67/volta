//! TUI frontend — app state machine, event loop, key dispatch.

pub mod menu;
pub mod reader;
pub mod rsvp;
pub mod theme;

use menu::MenuState;
use reader::ReaderState;
use rsvp::RsvpState;

use volta_core::doc::Document;
use volta_core::epub::EpubDoc;
use volta_core::player::PlayerState;
use volta_core::DocEnum;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, layout::Rect, text::{Line, Span}, widgets::Paragraph, style::{Color, Style}};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

pub enum Mode {
    Menu(MenuState),
    Reader(ReaderState),
    Rsvp(RsvpState),
}

pub struct App {
    pub mode: Mode,
    pub doc: Option<DocEnum>,
    pub file_path: Option<String>,  // for progress save key
    pub should_quit: bool,
    pub last_tick: Instant,
    pub save_flash: f64,  // seconds remaining for "Saved" confirmation
}

impl App {
    /// Create app in menu mode.
    pub fn new_menu() -> Self {
        App {
            mode: Mode::Menu(MenuState::load()),
            doc: None,
            file_path: None,
            should_quit: false,
            last_tick: Instant::now(),
            save_flash: 0.0,
        }
    }

    /// Create app with a loaded document, starting in reader mode.
    pub fn new(doc: DocEnum, file_path: String) -> Self {
        let reader = ReaderState::new(doc.doc());
        App {
            mode: Mode::Reader(reader),
            doc: Some(doc),
            file_path: Some(file_path),
            should_quit: false,
            last_tick: Instant::now(),
            save_flash: 0.0,
        }
    }

    /// Set reader position after creation (for saved progress restore).
    pub fn set_position(&mut self, chapter: usize, cursor_word: usize) {
        if let Mode::Reader(ref mut state) = &mut self.mode {
            if let Some(ref doc) = self.doc {
                let count = doc.doc().chapter_count() as usize;
                state.chapter = chapter.min(count.saturating_sub(1));
                state.cursor_word = cursor_word;
                state.scroll_to_cursor(20);
            }
        }
    }

    /// Open a book from the menu, switching to reader mode.
    fn open_book(&mut self, path: &Path) {
        let epub = match EpubDoc::open(path) {
            Ok(_e) => _e,
            Err(_e) => {
                // Stay in menu on error
                return;
            }
        };
        let total = epub.word_count() as usize;
        let doc = DocEnum::Epub(epub, PlayerState::new(total, 300));

        // Try to restore saved position
        let saved = load_saved_position(path);
        let mut reader = ReaderState::new(doc.doc());
        if let Some((ch, cw)) = saved {
            let count = doc.doc().chapter_count() as usize;
            reader.chapter = ch.min(count.saturating_sub(1));
            reader.cursor_word = cw;
            reader.scroll_to_cursor(20);
        }

        MenuState::add_recent(&path.to_string_lossy());
        self.file_path = Some(path.to_string_lossy().to_string());
        self.doc = Some(doc);
        self.mode = Mode::Reader(reader);
    }

    pub fn tick(&mut self) {
        if let Mode::Rsvp(_) = &self.mode {
            if let Some(ref mut doc) = self.doc {
                let now = Instant::now();
                let dt_ms = (now - self.last_tick).as_millis() as f64;
                self.last_tick = now;
                doc.player_mut().tick(dt_ms);
            }
        }
        // Count down save flash
        if self.save_flash > 0.0 {
            self.save_flash = (self.save_flash - 0.016).max(0.0);
        }
    }

    fn save_progress(&mut self) {
        if let Some(ref doc) = self.doc {
            let (chapter, cursor_word) = match &self.mode {
                Mode::Reader(s) => (s.chapter, s.cursor_word),
                Mode::Rsvp(_) => {
                    let idx = doc.player().current() as usize;
                    let d = doc.doc();
                    let ch = d.word_at(idx as u32).chapter_index as usize;
                    let ch_start = doc.chapter_start(ch as u32) as usize;
                    (ch, idx.saturating_sub(ch_start))
                }
                _ => return,
            };
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            let dir = format!("{}/.local/share/volta", home);
            let _ = std::fs::create_dir_all(&dir);
            let progress_path = format!("{}/progress.json", dir);
            let mut data: serde_json::Value = std::fs::read_to_string(&progress_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            if let Some(obj) = data.as_object_mut() {
                let key = self.file_path.as_deref().unwrap_or("unknown");
                obj.insert(key.to_string(), serde_json::json!({
                    "chapter": chapter,
                    "cursor_word": cursor_word,
                    "last_ts": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                }));
            }
            let _ = std::fs::write(&progress_path, serde_json::to_string(&data).unwrap_or_default());
            self.save_flash = 1.5;
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        match &self.mode {
            Mode::Menu(state) => {
                state.render(frame, area, &theme::NEON);
            }
            Mode::Reader(state) => {
                if let Some(ref doc) = self.doc {
                    state.render(frame, area, &theme::NEON, doc.doc());
                }
            }
            Mode::Rsvp(state) => {
                if let Some(ref doc) = self.doc {
                    state.render(
                        frame,
                        area,
                        &theme::NEON,
                        doc.player(),
                        doc.doc(),
                    );
                }
            }
        }
        // "Saved" flash
        if self.save_flash > 0.0 {
            let alpha = self.save_flash.min(1.0);
            let style = Style::default().fg(Color::Rgb(
                (0.0 * 255.0 * alpha) as u8,
                (1.0 * 255.0 * alpha) as u8,
                (0.5 * 255.0 * alpha) as u8,
            ));
            let line = Line::from(Span::styled("Saved", style));
            frame.render_widget(
                Paragraph::new(line),
                Rect::new(area.x + 1, area.y + area.height.saturating_sub(1), area.width, 1),
            );
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        let action = match &mut self.mode {
            Mode::Menu(state) => Action::Menu(MenuAction::from_key(state, key)),
            Mode::Reader(state) => Action::Reader(ReaderAction::from_key(state, key)),
            Mode::Rsvp(state) => Action::Rsvp(RsvpAction::from_key(state, key)),
        };

        match action {
            Action::Menu(a) => self.handle_menu_action(a),
            Action::Reader(a) => self.handle_reader_action(a),
            Action::Rsvp(a) => self.handle_rsvp_action(a),
        }
    }

    // ── Menu actions ──

    fn handle_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::None => {}
            MenuAction::MoveUp => {
                if let Mode::Menu(ref mut state) = &mut self.mode {
                    state.selected = state.selected.saturating_sub(1);
                }
            }
            MenuAction::MoveDown => {
                if let Mode::Menu(ref mut state) = &mut self.mode {
                    state.selected = (state.selected + 1).min(state.max_index());
                }
            }
            MenuAction::Open => {
                let path = match &self.mode {
                    Mode::Menu(state) => state.selected_path(),
                    _ => None,
                };
                if let Some(p) = path {
                    self.open_book(&p);
                }
            }
            MenuAction::Browse => {
                if let Some(path) = MenuState::browse_file() {
                    self.open_book(&path);
                }
            }
            MenuAction::Quit => {
                self.should_quit = true;
            }
        }
    }

    // ── Reader actions ──

    fn handle_reader_action(&mut self, action: ReaderAction) {
        let doc = match &self.doc {
            Some(ref d) => d,
            None => return,
        };

        match action {
            ReaderAction::None => {}
            ReaderAction::ScrollTo { scroll, cursor } => {
                if let Mode::Reader(ref mut s) = &mut self.mode {
                    s.scroll = scroll;
                    s.cursor_word = cursor;
                }
            }
            ReaderAction::CursorUp => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    cursor_up(state);
                }
            }
            ReaderAction::CursorDown => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    cursor_down(state);
                }
            }
            ReaderAction::CursorLeft => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    state.cursor_word = state.cursor_word.saturating_sub(1);
                    state.scroll_to_cursor(20);
                }
            }
            ReaderAction::CursorRight => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    let max = state.line_word_offsets.last().copied().unwrap_or(0);
                    state.cursor_word = (state.cursor_word + 1).min(max);
                    state.scroll_to_cursor(20);
                }
            }
            ReaderAction::NextChapter => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    let count = doc.doc().chapter_count() as usize;
                    if state.chapter + 1 < count {
                        state.chapter += 1;
                        state.scroll = 0;
                        state.cursor_word = 0;
                    }
                }
            }
            ReaderAction::PrevChapter => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    if state.chapter > 0 {
                        state.chapter -= 1;
                        state.scroll = 0;
                        state.cursor_word = 0;
                    }
                }
            }
            ReaderAction::GgTop => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    state.scroll = 0;
                    state.cursor_word = 0;
                    state.gg_timer = None;
                }
            }
            ReaderAction::GgArm => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    state.gg_timer = Some(Instant::now());
                }
            }
            ReaderAction::GBottom => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    state.gg_timer = None;
                    state.scroll = state.wrapped_lines.len().saturating_sub(1);
                    state.cursor_word = state.line_word_offsets.last().copied().unwrap_or(0);
                }
            }
            ReaderAction::EnterRsvp { cursor_word, chapter } => {
                if let Some(ref mut doc) = self.doc {
                    let ch_start = doc.chapter_start(chapter as u32);
                    let global_idx = ch_start as usize + cursor_word;
                    let max = doc.doc().word_count().saturating_sub(1) as usize;
                    let idx = global_idx.min(max);
                    doc.player_mut().seek(idx as u32);
                    doc.player_mut().play();
                    self.last_tick = Instant::now();
                    self.mode = Mode::Rsvp(RsvpState::new());
                }
            }
            ReaderAction::Save => {
                self.save_progress();
            }
            ReaderAction::Quit => {
                self.should_quit = true;
            }
        }
    }

    // ── RSVP actions ──

    fn handle_rsvp_action(&mut self, action: RsvpAction) {
        let doc = match &mut self.doc {
            Some(ref mut d) => d,
            None => return,
        };

        match action {
            RsvpAction::None => {}
            RsvpAction::TogglePlay => {
                let p = doc.player_mut();
                if p.is_playing() {
                    p.pause();
                } else {
                    p.play();
                    self.last_tick = Instant::now();
                }
            }
            RsvpAction::SeekBack10 => {
                let idx = doc.player().current();
                doc.player_mut().seek(if idx >= 10 { idx - 10 } else { 0 });
            }
            RsvpAction::SeekForward10 => {
                let idx = doc.player().current();
                let total = doc.doc().word_count();
                doc.player_mut().seek((idx + 10).min(total.saturating_sub(1)));
            }
            RsvpAction::SeekBack100 => {
                let idx = doc.player().current();
                doc.player_mut().seek(if idx >= 100 { idx - 100 } else { 0 });
            }
            RsvpAction::SeekForward100 => {
                let idx = doc.player().current();
                let total = doc.doc().word_count();
                doc.player_mut().seek((idx + 100).min(total.saturating_sub(1)));
            }
            RsvpAction::SpeedUp => {
                if let Mode::Rsvp(ref mut s) = &mut self.mode {
                    s.wpm = (s.wpm + 25).min(1000);
                    doc.player_mut().set_wpm(s.wpm);
                }
            }
            RsvpAction::SpeedDown => {
                if let Mode::Rsvp(ref mut s) = &mut self.mode {
                    s.wpm = s.wpm.saturating_sub(25).max(50);
                    doc.player_mut().set_wpm(s.wpm);
                }
            }
            RsvpAction::Save => {
                self.save_progress();
            }
            RsvpAction::ExitToReader => {
                doc.player_mut().pause();
                let idx = doc.player().current() as usize;
                let d = doc.doc();
                let ch = d.word_at(idx as u32).chapter_index as usize;
                let ch_start = doc.chapter_start(ch as u32) as usize;
                let cursor = idx.saturating_sub(ch_start);
                let mut reader = ReaderState::new(d);
                reader.chapter = ch;
                reader.cursor_word = cursor;
                reader.reflow(d, 80);
                reader.scroll_to_cursor(20);
                self.mode = Mode::Reader(reader);
            }
            RsvpAction::Quit => {
                doc.player_mut().pause();
                self.should_quit = true;
            }
        }
    }
}

// ── Load saved position from progress.json ──

#[derive(serde::Deserialize, Default)]
struct ProgressEntry {
    chapter: Option<usize>,
    cursor_word: Option<usize>,
}

fn load_saved_position(path: &Path) -> Option<(usize, usize)> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let progress_file = format!("{}/.local/share/volta/progress.json", home);
    let data: std::collections::HashMap<String, ProgressEntry> =
        std::fs::read_to_string(&progress_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
    let key = path.to_string_lossy().to_string();
    data.get(&key).map(|e| {
        (e.chapter.unwrap_or(0), e.cursor_word.unwrap_or(0))
    })
}

// ── Action enums ──

enum Action {
    Menu(MenuAction),
    Reader(ReaderAction),
    Rsvp(RsvpAction),
}

enum MenuAction {
    None,
    MoveUp,
    MoveDown,
    Open,
    Browse,
    Quit,
}

impl MenuAction {
    fn from_key(_state: &MenuState, key: KeyEvent) -> Self {
        match key.code {
            KeyCode::Up => MenuAction::MoveUp,
            KeyCode::Down => MenuAction::MoveDown,
            KeyCode::Enter => MenuAction::Open,
            KeyCode::Char('o')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                MenuAction::Browse
            }
            KeyCode::Esc | KeyCode::Char('q') => MenuAction::Quit,
            _ => MenuAction::None,
        }
    }
}

enum ReaderAction {
    None,
    ScrollTo { scroll: usize, cursor: usize },
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    NextChapter,
    PrevChapter,
    GgTop,
    GgArm,
    GBottom,
    EnterRsvp { cursor_word: usize, chapter: usize },
    Save,
    Quit,
}

impl ReaderAction {
    fn from_key(state: &mut ReaderState, key: KeyEvent) -> Self {
        if key.code != KeyCode::Char('g') {
            state.gg_timer = None;
        }

        match key.code {
            KeyCode::Up => ReaderAction::CursorUp,
            KeyCode::Down => ReaderAction::CursorDown,
            KeyCode::Left => ReaderAction::CursorLeft,
            KeyCode::Right => ReaderAction::CursorRight,

            KeyCode::Char('j') => {
                let scroll = (state.scroll + 3).min(state.wrapped_lines.len().saturating_sub(1));
                let cursor = state.line_word_offsets.get(scroll).copied().unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }
            KeyCode::Char('k') => {
                let scroll = state.scroll.saturating_sub(3);
                let cursor = state.line_word_offsets.get(scroll).copied().unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }

            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let h = 10;
                let scroll = (state.scroll + h).min(state.wrapped_lines.len().saturating_sub(1));
                let cursor = state.line_word_offsets.get(scroll).copied().unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let h = 10;
                let scroll = state.scroll.saturating_sub(h);
                let cursor = state.line_word_offsets.get(scroll).copied().unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let h = 20;
                let scroll = (state.scroll + h).min(state.wrapped_lines.len().saturating_sub(1));
                let cursor = state.line_word_offsets.get(scroll).copied().unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let h = 20;
                let scroll = state.scroll.saturating_sub(h);
                let cursor = state.line_word_offsets.get(scroll).copied().unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }

            KeyCode::Char('n') => ReaderAction::NextChapter,
            KeyCode::Char('p') => ReaderAction::PrevChapter,

            KeyCode::Char('g') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                let now = Instant::now();
                if let Some(t) = state.gg_timer {
                    if now.duration_since(t) < Duration::from_millis(300) {
                        ReaderAction::GgTop
                    } else {
                        ReaderAction::GgArm
                    }
                } else {
                    ReaderAction::GgArm
                }
            }
            KeyCode::Char('G') | KeyCode::Char('g')
                if key.modifiers.contains(KeyModifiers::SHIFT) =>
            {
                ReaderAction::GBottom
            }

            KeyCode::Char('r') => ReaderAction::EnterRsvp {
                cursor_word: state.cursor_word,
                chapter: state.chapter,
            },

            KeyCode::Char('s')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                ReaderAction::Save
            }

            KeyCode::Char('q') | KeyCode::Esc => ReaderAction::Quit,

            _ => ReaderAction::None,
        }
    }
}

enum RsvpAction {
    None,
    TogglePlay,
    SeekBack10,
    SeekForward10,
    SeekBack100,
    SeekForward100,
    SpeedUp,
    SpeedDown,
    Save,
    ExitToReader,
    Quit,
}

impl RsvpAction {
    fn from_key(_state: &RsvpState, key: KeyEvent) -> Self {
        match key.code {
            KeyCode::Char(' ') => RsvpAction::TogglePlay,
            KeyCode::Left | KeyCode::Char('h') => RsvpAction::SeekBack10,
            KeyCode::Right | KeyCode::Char('l') => RsvpAction::SeekForward10,
            KeyCode::Up | KeyCode::Char('k') => RsvpAction::SeekForward100,
            KeyCode::Down | KeyCode::Char('j') => RsvpAction::SeekBack100,
            KeyCode::Char('=') => RsvpAction::SpeedUp,
            KeyCode::Char('-') => RsvpAction::SpeedDown,
            KeyCode::Char('s')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                RsvpAction::Save
            }
            KeyCode::Esc => RsvpAction::ExitToReader,
            KeyCode::Char('q') => RsvpAction::Quit,
            _ => RsvpAction::None,
        }
    }
}

// ── Cursor helpers ──

fn cursor_up(state: &mut ReaderState) {
    let cur_line = state.cursor_line();
    if cur_line > 0 {
        let prev = cur_line - 1;
        let offset = state.cursor_word.saturating_sub(state.line_word_offsets[cur_line]);
        let first = state.line_word_offsets[prev];
        state.cursor_word = first + offset;
        if prev + 1 < state.line_word_offsets.len() {
            let next_first = state.line_word_offsets[prev + 1];
            state.cursor_word = state.cursor_word.min(next_first.saturating_sub(1));
        }
    }
    state.scroll_to_cursor(20);
}

fn cursor_down(state: &mut ReaderState) {
    let cur_line = state.cursor_line();
    if cur_line + 1 < state.line_word_offsets.len() {
        let next = cur_line + 1;
        let offset = state.cursor_word.saturating_sub(state.line_word_offsets[cur_line]);
        let first = state.line_word_offsets[next];
        state.cursor_word = first + offset;
        if next + 1 < state.line_word_offsets.len() {
            let next_first = state.line_word_offsets[next + 1];
            state.cursor_word = state.cursor_word.min(next_first.saturating_sub(1));
        }
    }
    state.scroll_to_cursor(20);
}

// ── Event loop ──

pub fn run(mut app: App) -> io::Result<()> {
    let mut stdout = io::stdout();
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
    )?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Initial reflow for reader mode
    if let Mode::Reader(ref mut state) = &mut app.mode {
        if let Some(ref doc) = app.doc {
            let w = terminal.size()?.width;
            state.reflow(doc.doc(), w);
        }
    }

    while !app.should_quit {
        // Reflow reader on each frame
        if let Mode::Reader(ref mut state) = &mut app.mode {
            if let Some(ref doc) = app.doc {
                let w = terminal.size()?.width;
                state.reflow(doc.doc(), w);
            }
        }

        terminal.draw(|f| app.render(f))?;

        if event::poll(Duration::from_millis(16))? {
            if let Ok(Event::Key(key)) = event::read() {
                app.handle_key(key);
            }
        }

        app.tick();
    }

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
    )?;

    Ok(())
}
