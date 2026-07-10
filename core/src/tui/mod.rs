//! TUI frontend — app state machine, event loop, key dispatch.

pub mod menu;
pub mod reader;
pub mod rsvp;
pub mod theme;

use menu::{CARD_H, CARD_W, MenuState};
use reader::ReaderState;
use rsvp::RsvpState;

use volta_core::doc::Document;
use volta_core::epub::EpubDoc;
use volta_core::library::{Library, LibraryEntry};
use volta_core::md::MdDoc;
use volta_core::pdf::PdfDoc;
use volta_core::player::PlayerState;
use volta_core::DocEnum;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
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
    pub file_path: Option<String>, // for progress save key
    pub should_quit: bool,
    pub last_tick: Instant,
    pub save_flash: f64,    // seconds remaining for "Saved" confirmation
    pub theme_index: usize, // index into theme::THEMES
    pub library: Library,
    // Search state
    pub search_query: String,
    pub search_matches: Vec<(usize, usize)>, // (chapter_idx, word_offset)
    pub search_idx: usize,
    pub search_input: bool, // true = typing search query
}

impl App {
    /// Create app in menu mode.
    pub fn new_menu() -> Self {
        let library = Library::load();
        App {
            mode: Mode::Menu(MenuState::new()),
            doc: None,
            file_path: None,
            should_quit: false,
            last_tick: Instant::now(),
            save_flash: 0.0,
            theme_index: 0,
            library,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_idx: 0,
            search_input: false,
        }
    }

    /// Create app with a loaded document, starting in reader mode.
    pub fn new(doc: DocEnum, file_path: String) -> Self {
        let mut library = Library::load();
        let reader = ReaderState::new(doc.doc());
        // Add to library (after reader is created so doc.doc() is available)
        add_to_library(&mut library, &file_path, doc.doc());
        App {
            mode: Mode::Reader(reader),
            doc: Some(doc),
            file_path: Some(file_path),
            should_quit: false,
            last_tick: Instant::now(),
            save_flash: 0.0,
            theme_index: 0,
            library,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_idx: 0,
            search_input: false,
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
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let doc: DocEnum = match ext.as_str() {
            "epub" => {
                let epub = match EpubDoc::open(path) {
                    Ok(e) => e,
                    Err(_) => return,
                };
                let total = epub.word_count() as usize;
                DocEnum::Epub(epub, PlayerState::new(total, 300))
            }
            "pdf" => {
                let pdf = match PdfDoc::open(path) {
                    Ok(p) => p,
                    Err(_) => return,
                };
                let total = pdf.word_count() as usize;
                DocEnum::Pdf(pdf, PlayerState::new(total, 300))
            }
            "md" => {
                let md = match MdDoc::open(path) {
                    Ok(m) => m,
                    Err(_) => return,
                };
                let total = md.word_count() as usize;
                DocEnum::Md(md, PlayerState::new(total, 300))
            }
            _ => return,
        };

        // Add to library
        let path_str = path.to_string_lossy().to_string();
        add_to_library(&mut self.library, &path_str, doc.doc());

        // Try to restore saved position
        let saved = load_saved_position(path);
        let mut reader = ReaderState::new(doc.doc());
        if let Some((ch, cw)) = saved {
            let count = doc.doc().chapter_count() as usize;
            reader.chapter = ch.min(count.saturating_sub(1));
            reader.cursor_word = cw;
            reader.scroll_to_cursor(20);
        }

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
            if let Some(path) = &self.file_path {
                self.library
                    .update_progress(path, chapter as u32, cursor_word);
                self.library.save();
            }
            self.save_flash = 1.5;
        }
    }

    /// Get the active theme from the THEMES array.
    fn active_theme(&self) -> &theme::Theme {
        &theme::THEMES[self.theme_index]
    }

    /// Cycle theme: dir=1 for next, dir=-1 for previous.
    fn cycle_theme(&mut self, dir: i32) {
        self.theme_index = theme::cycle_theme(self.theme_index, dir);
    }

    /// Execute a case-insensitive search across all chapters.
    /// Populates self.search_matches with (chapter_idx, word_offset) pairs.
    fn execute_search(&mut self) {
        self.search_matches.clear();
        self.search_idx = 0;

        let query = self.search_query.to_lowercase();
        if query.is_empty() {
            return;
        }

        let doc = match &self.doc {
            Some(d) => d.doc(),
            None => return,
        };

        for ch in 0..doc.chapter_count() {
            let text = doc.chapter_text(ch);
            let lower = text.to_lowercase();

            let mut char_pos = 0;
            while let Some(found) = lower[char_pos..].find(&query) {
                let abs_pos = char_pos + found;
                // Count words before this character position
                let word_offset = text[..abs_pos].split_whitespace().count();
                self.search_matches.push((ch as usize, word_offset));
                char_pos = abs_pos + query.len();
            }
        }
    }

    /// Jump to match at search_idx, updating reader chapter/cursor/scroll.
    fn jump_to_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }

        let idx = self.search_idx.min(self.search_matches.len() - 1);
        let (ch, word_offset) = self.search_matches[idx];

        if let Mode::Reader(ref mut state) = &mut self.mode {
            state.chapter = ch;
            state.cursor_word = word_offset;
            // Reflow will be done by the event loop on next frame
            state.scroll_to_cursor(20);
        }
    }

    /// Search next match (wraps around).
    fn search_next(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_idx = (self.search_idx + 1) % self.search_matches.len();
        self.jump_to_match();
    }

    /// Search previous match (wraps around).
    fn search_prev(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_idx = if self.search_idx == 0 {
            self.search_matches.len() - 1
        } else {
            self.search_idx - 1
        };
        self.jump_to_match();
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let thm_idx = self.theme_index;

        // Extract entries before mutable borrow of mode
        let menu_entries: Vec<(String, LibraryEntry)> =
            self.library.entries().iter().map(|(p, e)|
                (p.to_string(), LibraryEntry {
                    title: e.title.clone(),
                    author: e.author.clone(),
                    format: e.format.clone(),
                    chapter_count: e.chapter_count,
                    current_chapter: e.current_chapter,
                    current_word: e.current_word,
                    last_opened: e.last_opened,
                    added: e.added,
                    cover_path: e.cover_path.clone(),
                })
            ).collect();
        let menu_refs: Vec<(&str, &LibraryEntry)> =
            menu_entries.iter().map(|(p, e)| (p.as_str(), e)).collect();
        let search_matches = self.search_matches.clone();
        let search_idx = self.search_idx;
        let search_input = self.search_input;
        let search_query = self.search_query.clone();

        let thm = &theme::THEMES[thm_idx];

        match &mut self.mode {
            Mode::Menu(ref mut state) => {
                state.render(frame, area, thm, &menu_refs);
            }
            Mode::Reader(ref state) => {
                if let Some(ref doc) = self.doc {
                    state.render(
                        frame,
                        area,
                        thm,
                        doc.doc(),
                        &search_matches,
                        search_idx,
                    );
                }
            }
            Mode::Rsvp(ref state) => {
                if let Some(ref doc) = self.doc {
                    state.render(frame, area, thm, doc.player(), doc.doc());
                }
            }
        }
        if self.search_input {
            let prompt = format!("/{}", self.search_query);
            let style = Style::default().fg(thm.cursor);
            let line = Line::from(Span::styled(prompt, style));
            frame.render_widget(
                Paragraph::new(line),
                Rect::new(
                    area.x,
                    area.y + area.height.saturating_sub(1),
                    area.width,
                    1,
                ),
            );
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
                Rect::new(
                    area.x + 1,
                    area.y + area.height.saturating_sub(1),
                    area.width,
                    1,
                ),
            );
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Search input mode: capture all keystrokes
        if self.search_input {
            match key.code {
                KeyCode::Esc => {
                    self.search_input = false;
                    self.search_query.clear();
                }
                KeyCode::Enter => {
                    self.search_input = false;
                    self.execute_search();
                    if !self.search_matches.is_empty() {
                        self.jump_to_match();
                    }
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                }
                _ => {}
            }
            return;
        }

        let action = match &mut self.mode {
            Mode::Menu(state) => {
                let total = self.library.entries().len();
                Action::Menu(MenuAction::from_key(state, key, total))
            }
            Mode::Reader(state) => {
                // Pass search state to reader action dispatch
                Action::Reader(ReaderAction::from_key(
                    state,
                    key,
                    !self.search_matches.is_empty(),
                ))
            }
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
            MenuAction::MoveUp
            | MenuAction::MoveDown
            | MenuAction::MoveLeft
            | MenuAction::MoveRight => {
                // Handled in from_key directly
            }
            MenuAction::Open => {
                let path = match &self.mode {
                    Mode::Menu(state) => {
                        let entries = self.library.entries();
                        state.selected_path(&entries)
                    }
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
            ReaderAction::ThemeNext => {
                self.cycle_theme(1);
            }
            ReaderAction::ThemePrev => {
                self.cycle_theme(-1);
            }
            ReaderAction::Quit => {
                self.should_quit = true;
            }
            ReaderAction::BackToMenu => {
                self.mode = Mode::Menu(MenuState::new());
                // Clear search state
                self.search_query.clear();
                self.search_matches.clear();
                self.search_input = false;
            }
            ReaderAction::SearchStart => {
                self.search_input = true;
                self.search_query.clear();
            }
            ReaderAction::SearchNext => {
                self.search_next();
            }
            ReaderAction::SearchPrev => {
                self.search_prev();
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
            RsvpAction::ThemeNext => {
                self.cycle_theme(1);
            }
            RsvpAction::ThemePrev => {
                self.cycle_theme(-1);
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
    data.get(&key)
        .map(|e| (e.chapter.unwrap_or(0), e.cursor_word.unwrap_or(0)))
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
    MoveLeft,
    MoveRight,
    Open,
    Browse,
    Quit,
}

impl MenuAction {
    fn from_key(state: &mut MenuState, key: KeyEvent, total_entries: usize) -> Self {
        match key.code {
            KeyCode::Up => {
                if state.selected_row > 0 {
                    state.selected_row -= 1;
                }
                MenuAction::None
            }
            KeyCode::Down => {
                let max_row = state.max_row(total_entries);
                if state.selected_row < max_row {
                    state.selected_row += 1;
                }
                MenuAction::None
            }
            KeyCode::Left => {
                if state.selected_col > 0 {
                    state.selected_col -= 1;
                }
                MenuAction::None
            }
            KeyCode::Right => {
                let max_col = state.max_col(total_entries);
                if state.selected_col < max_col {
                    state.selected_col += 1;
                }
                MenuAction::None
            }
            KeyCode::Enter => MenuAction::Open,
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
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
    ThemeNext,
    ThemePrev,
    Quit,
    BackToMenu,
    SearchStart,
    SearchNext,
    SearchPrev,
}

impl ReaderAction {
    fn from_key(state: &mut ReaderState, key: KeyEvent, has_search: bool) -> Self {
        if key.code != KeyCode::Char('g') {
            state.gg_timer = None;
        }

        match key.code {
            KeyCode::Esc => ReaderAction::BackToMenu,
            KeyCode::Up => ReaderAction::CursorUp,
            KeyCode::Down => ReaderAction::CursorDown,
            KeyCode::Left => ReaderAction::CursorLeft,
            KeyCode::Right => ReaderAction::CursorRight,

            // Search: / enters search mode
            KeyCode::Char('/') => ReaderAction::SearchStart,

            // n/N: next/prev match if search active, else next/prev chapter
            KeyCode::Char('n') => {
                if has_search {
                    ReaderAction::SearchNext
                } else {
                    ReaderAction::NextChapter
                }
            }
            KeyCode::Char('N') => {
                if has_search {
                    ReaderAction::SearchPrev
                } else {
                    ReaderAction::None
                }
            }

            KeyCode::Char('p') => ReaderAction::PrevChapter,

            KeyCode::Char('j') => {
                let scroll =
                    (state.scroll + 3).min(state.wrapped_lines.len().saturating_sub(1));
                let cursor = state
                    .line_word_offsets
                    .get(scroll)
                    .copied()
                    .unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }
            KeyCode::Char('k') => {
                let scroll = state.scroll.saturating_sub(3);
                let cursor = state
                    .line_word_offsets
                    .get(scroll)
                    .copied()
                    .unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }

            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let h = 10;
                let scroll =
                    (state.scroll + h).min(state.wrapped_lines.len().saturating_sub(1));
                let cursor = state
                    .line_word_offsets
                    .get(scroll)
                    .copied()
                    .unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let h = 10;
                let scroll = state.scroll.saturating_sub(h);
                let cursor = state
                    .line_word_offsets
                    .get(scroll)
                    .copied()
                    .unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let h = 20;
                let scroll =
                    (state.scroll + h).min(state.wrapped_lines.len().saturating_sub(1));
                let cursor = state
                    .line_word_offsets
                    .get(scroll)
                    .copied()
                    .unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let h = 20;
                let scroll = state.scroll.saturating_sub(h);
                let cursor = state
                    .line_word_offsets
                    .get(scroll)
                    .copied()
                    .unwrap_or(state.cursor_word);
                ReaderAction::ScrollTo { scroll, cursor }
            }

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

            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ReaderAction::Save
            }

            // Theme cycling
            KeyCode::Char('t') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                ReaderAction::ThemeNext
            }
            KeyCode::Char('T') => ReaderAction::ThemePrev,

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
    ThemeNext,
    ThemePrev,
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
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => RsvpAction::Save,
            KeyCode::Char('t') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                RsvpAction::ThemeNext
            }
            KeyCode::Char('T') => RsvpAction::ThemePrev,
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
        let offset = state
            .cursor_word
            .saturating_sub(state.line_word_offsets[cur_line]);
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
        let offset = state
            .cursor_word
            .saturating_sub(state.line_word_offsets[cur_line]);
        let first = state.line_word_offsets[next];
        state.cursor_word = first + offset;
        if next + 1 < state.line_word_offsets.len() {
            let next_first = state.line_word_offsets[next + 1];
            state.cursor_word = state.cursor_word.min(next_first.saturating_sub(1));
        }
    }
    state.scroll_to_cursor(20);
}

// ── Library helpers ──

/// Add or update a book in the library from its Document trait.
fn add_to_library(library: &mut Library, path: &str, doc: &dyn Document) {
    let format = if path.ends_with(".epub") {
        "epub"
    } else if path.ends_with(".pdf") {
        "pdf"
    } else if path.ends_with(".md") {
        "md"
    } else {
        return;
    };

    let title = doc.title().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Extract cover thumbnail (async-friendly — cached on disk)
    let cover_path = volta_core::cover::extract_cover(path, format);

    library.upsert(
        path,
        LibraryEntry {
            title,
            author: String::new(),
            format: format.to_string(),
            chapter_count: doc.chapter_count(),
            current_chapter: 0,
            current_word: 0,
            last_opened: now,
            added: now,
            cover_path,
        },
    );
    library.save();
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

        // Kitty cover images — clear when not in menu, emit when in menu
        if volta_core::cover::is_kitty() {
            if let Mode::Menu(_) = &app.mode {
                let entries = app.library.entries();
                let size = terminal.size()?;
                let cols = (size.width.saturating_sub(2) / (CARD_W + 1)).max(1) as usize;
                for (i, (_path, entry)) in entries.iter().enumerate() {
                    if let Some(ref cover) = entry.cover_path {
                        let col = (i % cols) as u16;
                        let row = (i / cols) as u16;
                        let card_x = 1 + col * (CARD_W + 1);
                        let card_y = 1 + row * (CARD_H + 1);
                        volta_core::cover::kitty_display_image(cover, card_y, card_x, 6, 4);
                    }
                }
            } else {
                // Clear images when leaving menu mode
                volta_core::cover::kitty_clear_all();
            }
        }

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
