//! TUI frontend — app state machine, event loop, key dispatch.

pub mod action;
pub mod menu;
pub mod reader;
pub mod rsvp;
pub mod theme;
pub mod toc;

use action::{Action, MenuAction, ReaderAction, RsvpAction, TocAction};
use menu::{CARD_H, CARD_W, MenuState};
use reader::ReaderState;
use rsvp::RsvpState;
use toc::TocState;

use volta_core::doc::Document;
use volta_core::epub::EpubDoc;
use volta_core::library::{Library, LibraryEntry};
use volta_core::md::MdDoc;
use volta_core::pdf::PdfDoc;
use volta_core::player::PlayerState;
use volta_core::DocEnum;

use crossterm::event::{self, Event, KeyCode, KeyEvent};
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
    Toc(TocState),
}

pub struct App {
    pub mode: Mode,
    pub doc: Option<DocEnum>,
    pub file_path: Option<String>, // for progress save key
    pub should_quit: bool,
    pub last_tick: Instant,
    pub flash: (f64, String),    // (seconds, message) for status-bar flash
    pub theme_index: usize, // index into theme::THEMES
    pub library: Library,
    // Search state
    pub search_query: String,
    pub search_matches: Vec<(usize, usize)>, // (chapter_idx, word_offset)
    pub search_idx: usize,
    pub search_input: bool, // true = typing search query
    pub jump_stack: Vec<(usize, usize)>, // (chapter, cursor_word) for Ctrl+o back
    /// Redraw only when something changed (events, RSVP ticks, save flash).
    pub needs_draw: bool,
    /// Kitty covers: true = emitted for the current menu view.
    pub kitty_covers_shown: bool,
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
            flash: (0.0, String::new()),
            theme_index: 0,
            library,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_idx: 0,
            search_input: false,
            jump_stack: Vec::new(),
            needs_draw: true,
            kitty_covers_shown: false,
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
            flash: (0.0, String::new()),
            theme_index: 0,
            library,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_idx: 0,
            search_input: false,
            jump_stack: Vec::new(),
            needs_draw: true,
            kitty_covers_shown: false,
        }
    }

    /// Set reader position after creation (for saved progress restore).
    pub fn set_position(&mut self, chapter: usize, cursor_word: usize) {
        if let Mode::Reader(ref mut state) = &mut self.mode {
            if let Some(ref doc) = self.doc {
                let count = doc.doc().chapter_count() as usize;
                state.chapter = chapter.min(count.saturating_sub(1));
                state.cursor_word = cursor_word;
                state.scroll_to_cursor();
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

        // Capture saved position before add_to_library resets it
        let path_str = path.to_string_lossy().to_string();
        let saved = self.library.get(&path_str)
            .map(|e| (e.current_chapter as usize, e.current_word));

        add_to_library(&mut self.library, &path_str, doc.doc());

        let mut reader = ReaderState::new(doc.doc());
        if let Some((ch, cw)) = saved {
            let count = doc.doc().chapter_count() as usize;
            if ch > 0 || cw > 0 {
                reader.chapter = ch.min(count.saturating_sub(1));
                reader.cursor_word = cw;
                reader.scroll_to_cursor();
            }
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
                if doc.player().is_playing() {
                    self.needs_draw = true;
                }
            }
        }
        // Count down status-bar flash
        if self.flash.0 > 0.0 {
            self.flash.0 = (self.flash.0 - 0.016).max(0.0);
            self.needs_draw = true;
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
            self.flash = (1.5, "Saved".into());
        }
    }


    /// Cycle theme: dir=1 for next, dir=-1 for previous.
    fn cycle_theme(&mut self, dir: i32) {
        self.theme_index = theme::cycle_theme(self.theme_index, dir);
    }

    /// Push current reader position onto jump-back stack (if different from top).
    fn push_jump(&mut self) {
        if let Mode::Reader(ref state) = &self.mode {
            let pos = (state.chapter, state.cursor_word);
            if self.jump_stack.last() != Some(&pos) {
                self.jump_stack.push(pos);
                if self.jump_stack.len() > 20 {
                    self.jump_stack.remove(0);
                }
            }
        }
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

            // Single pass: collect word start byte offsets, then map each
            // match position to a word index via binary search. The word
            // offset of a match at byte p = number of word starts < p
            // (identical to the old split_whitespace().count() semantics).
            let word_starts: Vec<usize> = word_byte_starts(text);

            let mut char_pos = 0;
            while let Some(found) = lower[char_pos..].find(&query) {
                let abs_pos = char_pos + found;
                let word_offset = word_starts.partition_point(|&s| s < abs_pos);
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

        self.push_jump();
        if let Mode::Reader(ref mut state) = &mut self.mode {
            state.chapter = ch;
            state.cursor_word = word_offset;
            // Reflow will be done by the event loop on next frame
            state.scroll_to_cursor();
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

        // Menu entries are only needed in Menu mode — avoid cloning the
        // whole library on every reader/RSVP/TOC frame.
        let menu_entries: Vec<(String, LibraryEntry)> = if matches!(self.mode, Mode::Menu(_)) {
            self.library.entries().iter().map(|(p, e)| {
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
            }).collect()
        } else {
            Vec::new()
        };
        let menu_refs: Vec<(&str, &LibraryEntry)> =
            menu_entries.iter().map(|(p, e)| (p.as_str(), e)).collect();
        let search_matches = self.search_matches.clone();
        let search_idx = self.search_idx;

        let thm = &theme::THEMES[thm_idx];

        match &mut self.mode {
            Mode::Menu(ref mut state) => {
                state.render(frame, area, thm, &menu_refs);
            }
            Mode::Reader(ref mut state) => {
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
            Mode::Toc(ref mut state) => {
                state.render(frame, area, thm);
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
        // Status-bar flash (Saved / Yanked / etc.)
        if self.flash.0 > 0.0 {
            let alpha = self.flash.0.min(1.0);
            let style = Style::default().fg(Color::Rgb(
                (0.0 * 255.0 * alpha) as u8,
                (1.0 * 255.0 * alpha) as u8,
                (0.5 * 255.0 * alpha) as u8,
            ));
            let line = Line::from(Span::styled(self.flash.1.clone(), style));
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

    /// Mouse input: wheel scrolls the reader viewport (3 lines, like
    /// j/k) and moves the menu selection. Arrow-key scroll emulation
    /// from the terminal keeps working alongside this.
    pub fn handle_mouse(&mut self, m: crossterm::event::MouseEvent) {
        use crossterm::event::MouseEventKind;
        match &mut self.mode {
            Mode::Reader(state) => {
                let delta: isize = match m.kind {
                    MouseEventKind::ScrollUp => -3,
                    MouseEventKind::ScrollDown => 3,
                    _ => return,
                };
                self.needs_draw = true;
                let max_scroll = state.wrapped_lines.len().saturating_sub(1);
                state.scroll = if delta < 0 {
                    state.scroll.saturating_sub((-delta) as usize)
                } else {
                    (state.scroll + delta as usize).min(max_scroll)
                };
                state.cursor_word = state
                    .line_word_offsets
                    .get(state.scroll)
                    .copied()
                    .unwrap_or(state.cursor_word);
            }
            Mode::Menu(state) => {
                let total = self.library.entries().len();
                match m.kind {
                    MouseEventKind::ScrollUp => {
                        self.needs_draw = true;
                        state.selected_row = state.selected_row.saturating_sub(1);
                    }
                    MouseEventKind::ScrollDown => {
                        self.needs_draw = true;
                        let max_row = state.max_row(total);
                        if state.selected_row < max_row {
                            state.selected_row += 1;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        self.needs_draw = true;
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
            Mode::Toc(state) => Action::Toc(TocAction::from_key(state, key)),
        };

        match action {
            Action::Menu(a) => self.handle_menu_action(a),
            Action::Reader(a) => self.handle_reader_action(a),
            Action::Rsvp(a) => self.handle_rsvp_action(a),
            Action::Toc(a) => self.handle_toc_action(a),
        }
    }

    // ── Menu actions ──

    fn handle_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::None => {}
            
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
            MenuAction::Delete => {
                let path = match &self.mode {
                    Mode::Menu(state) => {
                        let entries = self.library.entries();
                        state.selected_path(&entries)
                            .map(|p| p.to_string_lossy().to_string())
                    }
                    _ => None,
                };
                if let Some(p) = path {
                    self.library.remove(&p);
                    self.library.save();
                    // clamp selection after removal
                    if let Mode::Menu(ref mut state) = &mut self.mode {
                        let total = self.library.entries().len();
                        let max_row = if total > 0 { state.max_row(total) } else { 0 };
                        if state.selected_row > max_row {
                            state.selected_row = max_row;
                        }
                        let max_col = if total > 0 { state.max_col(total, state.selected_row) } else { 0 };
                        if state.selected_col > max_col {
                            state.selected_col = max_col;
                        }
                    }
                }
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
                    state.scroll_to_cursor();
                }
            }
            ReaderAction::CursorRight => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    let max = state.line_word_offsets.last().copied().unwrap_or(0);
                    state.cursor_word = (state.cursor_word + 1).min(max);
                    state.scroll_to_cursor();
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

            ReaderAction::GBottom => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    state.gg_timer = None;
                    state.scroll = state.wrapped_lines.len().saturating_sub(1);
                    state.cursor_word = state.line_word_offsets.last().copied().unwrap_or(0);
                }
            }
            ReaderAction::EnterToc {
                chapter,
                cursor_word,
            } => {
                self.push_jump();
                if let Some(ref doc) = self.doc {
                    let toc = TocState::new(doc.doc(), chapter, cursor_word);
                    self.mode = Mode::Toc(toc);
                }
            }
            ReaderAction::JumpBack => {
                if let Some((ch, cw)) = self.jump_stack.pop() {
                    if let Mode::Reader(ref mut state) = &mut self.mode {
                        let count = doc.doc().chapter_count() as usize;
                        state.chapter = ch.min(count.saturating_sub(1));
                        state.cursor_word = cw;
                        state.scroll_to_cursor();
                    }
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
            ReaderAction::MarginAdjust { delta } => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    if delta < 0 {
                        state.margin = state.margin.saturating_sub((-delta) as u16);
                    } else {
                        state.margin = (state.margin + delta as u16).min(40);
                    }
                }
            }
            ReaderAction::ColWidthAdjust { delta } => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    if delta > 0 {
                        // Off -> start at a readable measure
                        state.max_col_width = if state.max_col_width == 0 {
                            80
                        } else {
                            (state.max_col_width + delta as u16).min(200)
                        };
                    } else if state.max_col_width > 0 {
                        let next = state.max_col_width.saturating_sub((-delta) as u16);
                        // Below a readable floor, turn the limit off
                        state.max_col_width = if next < 50 { 0 } else { next };
                    }
                }
            }
            ReaderAction::ToggleVisual => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    if state.selection_anchor.is_some() {
                        state.selection_anchor = None;
                        state.visual_line_mode = false;
                    } else {
                        state.selection_anchor = Some(state.cursor_word);
                        state.visual_line_mode = false;
                    }
                }
            }
            ReaderAction::ToggleVisualLine => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    if state.selection_anchor.is_some() {
                        state.selection_anchor = None;
                        state.visual_line_mode = false;
                    } else {
                        // Snap anchor to line start, cursor to line end
                        let anchor_line = state.cursor_line();
                        let anchor_word = state.line_word_offsets[anchor_line];
                        let end_word = if anchor_line + 1 < state.line_word_offsets.len() {
                            state.line_word_offsets[anchor_line + 1].saturating_sub(1)
                        } else {
                            state.line_word_offsets.last().copied().unwrap_or(0)
                        };
                        state.selection_anchor = Some(anchor_word);
                        state.cursor_word = end_word;
                        state.visual_line_mode = true;
                    }
                }
            }
            ReaderAction::Yank => {
                if let Mode::Reader(ref mut state) = &mut self.mode {
                    if let Some(anchor) = state.selection_anchor {
                        let start = anchor.min(state.cursor_word);
                        let end = anchor.max(state.cursor_word);
                        let text = build_selection_text(&state.wrapped_lines, &state.line_word_offsets, start, end);
                        let word_count = end - start + 1;

                        // Copy to system clipboard via wl-copy (Wayland / X11)
                        #[cfg(feature = "tui")]
                        {
                            use std::io::Write;
                            let _ = std::process::Command::new("wl-copy")
                                .stdin(std::process::Stdio::piped())
                                .spawn()
                                .and_then(|mut child| {
                                    if let Some(mut stdin) = child.stdin.take() {
                                        stdin.write_all(text.as_bytes())?;
                                    }
                                    child.wait()
                                });
                        }

                        state.selection_anchor = None;
                        state.visual_line_mode = false;
                        let msg = format!("Yanked {} words", word_count);
                        self.flash = (1.5, msg);
                    }
                }
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
                reader.scroll_to_cursor();
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

impl App {
    fn handle_toc_action(&mut self, action: TocAction) {
        let doc = match &self.doc {
            Some(d) => d,
            None => return,
        };

        match action {
            TocAction::None => {}
            TocAction::MoveUp => {
                if let Mode::Toc(ref mut state) = &mut self.mode {
                    if state.selected > 0 {
                        state.selected -= 1;
                        state.ensure_visible(10);
                    }
                }
            }
            TocAction::MoveDown => {
                if let Mode::Toc(ref mut state) = &mut self.mode {
                    if state.selected + 1 < state.filtered.len() {
                        state.selected += 1;
                        state.ensure_visible(10);
                    }
                }
            }
            TocAction::Select => {
                let (target_ch, _source_ch, _source_cw) = if let Mode::Toc(ref state) = &self.mode {
                    if state.filtered.is_empty() {
                        return;
                    }
                    let idx = state.selected.min(state.filtered.len() - 1);
                    (
                        state.filtered[idx],
                        state.source_chapter,
                        state.source_cursor,
                    )
                } else {
                    return;
                };

                let count = doc.doc().chapter_count() as usize;
                let ch = target_ch.min(count.saturating_sub(1));

                // Switch to reader at the selected chapter
                let mut reader = ReaderState::new(doc.doc());
                reader.chapter = ch;
                reader.cursor_word = 0;
                reader.scroll_to_cursor();
                self.mode = Mode::Reader(reader);
            }
            TocAction::Cancel => {
                // Return to reader at the source position
                let (ch, cw) = if let Mode::Toc(ref state) = &self.mode {
                    (state.source_chapter, state.source_cursor)
                } else {
                    return;
                };
                let mut reader = ReaderState::new(doc.doc());
                let count = doc.doc().chapter_count() as usize;
                reader.chapter = ch.min(count.saturating_sub(1));
                reader.cursor_word = cw;
                reader.scroll_to_cursor();
                self.mode = Mode::Reader(reader);
            }
            TocAction::ToggleFilter => {
                if let Mode::Toc(ref mut state) = &mut self.mode {
                    state.filter_active = !state.filter_active;
                    if !state.filter_active {
                        state.filter.clear();
                        state.apply_filter();
                    }
                }
            }
            TocAction::FilterChar(c) => {
                if let Mode::Toc(ref mut state) = &mut self.mode {
                    state.filter.push(c);
                    state.apply_filter();
                }
            }
            TocAction::FilterBackspace => {
                if let Mode::Toc(ref mut state) = &mut self.mode {
                    state.filter.pop();
                    state.apply_filter();
                }
            }
            TocAction::GgTop => {
                if let Mode::Toc(ref mut state) = &mut self.mode {
                    state.selected = 0;
                    state.scroll = 0;
                }
            }
            TocAction::GBottom => {
                if let Mode::Toc(ref mut state) = &mut self.mode {
                    let last = state.filtered.len().saturating_sub(1);
                    state.selected = last;
                    state.scroll = last.saturating_sub(9);
                }
            }
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
    state.scroll_to_cursor();
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
    state.scroll_to_cursor();
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

    // Preserve existing progress so opening doesn't reset saved position
    let (saved_ch, saved_cw) = library
        .get(path)
        .map(|e| (e.current_chapter, e.current_word))
        .unwrap_or((0, 0));

    library.upsert(
        path,
        LibraryEntry {
            title,
            author: String::new(),
            format: format.to_string(),
            chapter_count: doc.chapter_count(),
            current_chapter: saved_ch,
            current_word: saved_cw,
            last_opened: now,
            added: now,
            cover_path,
        },
    );
    library.save();
}

// ── Event loop ──

/// Byte offset of every word start in `text` (ascending). Used to map
/// search-match byte positions to word indices via binary search.
fn word_byte_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut in_word = false;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            starts.push(i);
        }
    }
    starts
}

/// Build a plain-text string from a word range spanning wrapped lines.
/// word range is inclusive: [start_word, end_word].
/// Strips the 4-space paragraph indent from yanked text.
fn build_selection_text(
    wrapped_lines: &[String],
    line_word_offsets: &[usize],
    start_word: usize,
    end_word: usize,
) -> String {
    let mut result = String::new();
    let mut prev_line_has_words = false;

    for (li, line) in wrapped_lines.iter().enumerate() {
        let first = line_word_offsets[li];
        let words_in_line = line.split_whitespace().count();
        if words_in_line == 0 {
            continue;
        }
        let last = first + words_in_line - 1;

        // Does this line overlap our range?
        if last < start_word || first > end_word {
            continue;
        }

        // If we skipped lines and had content, insert a space
        if prev_line_has_words && !result.is_empty() {
            result.push(' ');
        }

        // Collect words in this line that fall within the range
        let mut line_words: Vec<&str> = Vec::new();
        for (wi, word) in line.split_whitespace().enumerate() {
            let global = first + wi;
            if global >= start_word && global <= end_word {
                line_words.push(word);
            }
        }

        if !line_words.is_empty() {
            result.push_str(&line_words.join(" "));
            prev_line_has_words = true;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::word_byte_starts;

    /// The original O(n) per-match algorithm, kept as a test oracle.
    fn oracle_offset(text: &str, abs_pos: usize) -> usize {
        text[..abs_pos].split_whitespace().count()
    }

    #[test]
    fn word_offset_matches_oracle() {
        let text = "the quick  brown\n\nfox jumps\tover the lazy dog the end";
        let starts = word_byte_starts(text);
        // Check every byte position that begins a match of "the"
        let lower = text.to_lowercase();
        let mut pos = 0;
        while let Some(found) = lower[pos..].find("the") {
            let abs = pos + found;
            let fast = starts.partition_point(|&s| s < abs);
            assert_eq!(fast, oracle_offset(text, abs), "mismatch at byte {}", abs);
            pos = abs + 3;
        }
        // Also check mid-word and whitespace positions exhaustively
        for abs in 0..text.len() {
            if !text.is_char_boundary(abs) {
                continue;
            }
            let fast = starts.partition_point(|&s| s < abs);
            assert_eq!(fast, oracle_offset(text, abs), "mismatch at byte {}", abs);
        }
    }
}

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

    let mut last_kitty_size = (0u16, 0u16);

    while !app.should_quit {
        // Reflow reader only when its inputs changed (chapter, width,
        // margins) — not on every frame.
        if let Mode::Reader(ref mut state) = &mut app.mode {
            if let Some(ref doc) = app.doc {
                let w = terminal.size()?.width;
                if state.reflow_if_needed(doc.doc(), w) {
                    app.needs_draw = true;
                }
            }
        }

        if app.needs_draw {
            terminal.draw(|f| app.render(f))?;
            app.needs_draw = false;
        }

        // Kitty cover images — emit once per menu view (and on resize),
        // clear once when leaving the menu.
        if volta_core::cover::is_kitty() {
            if let Mode::Menu(_) = &app.mode {
                let size = terminal.size()?;
                let wh = (size.width, size.height);
                if !app.kitty_covers_shown || wh != last_kitty_size {
                    last_kitty_size = wh;
                    let entries = app.library.entries();
                    let cols =
                        (size.width.saturating_sub(2) / (CARD_W + 1)).max(1) as usize;
                    for (i, (_path, entry)) in entries.iter().enumerate() {
                        if let Some(ref cover) = entry.cover_path {
                            let col = (i % cols) as u16;
                            let row = (i / cols) as u16;
                            let card_x = 1 + col * (CARD_W + 1);
                            let card_y = 1 + row * (CARD_H + 1);
                            volta_core::cover::kitty_display_image(
                                cover, card_y, card_x, 6, 4,
                            );
                        }
                    }
                    app.kitty_covers_shown = true;
                }
            } else if app.kitty_covers_shown {
                volta_core::cover::kitty_clear_all();
                app.kitty_covers_shown = false;
            }
        }

        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => app.handle_key(key),
                Event::Mouse(m) => app.handle_mouse(m),
                Event::Resize(_, _) => app.needs_draw = true,
                _ => {}
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
