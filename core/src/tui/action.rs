//! Reader action enum and keybinding dispatch.
//! Extracted from mod.rs to keep keybindings self-contained.

use crate::tui::menu::MenuState;
use crate::tui::reader::ReaderState;
use crate::tui::toc::TocState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

pub enum ReaderAction {
    None,
    ScrollTo { scroll: usize, cursor: usize },
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    NextChapter,
    PrevChapter,
    GgTop,
    GBottom,
    EnterRsvp { cursor_word: usize, chapter: usize },
    EnterToc { chapter: usize, cursor_word: usize },
    JumpBack,
    Save,
    ThemeNext,
    ThemePrev,
    BackToMenu,
    SearchStart,
    SearchNext,
    SearchPrev,
}

impl ReaderAction {
    pub fn from_key(state: &mut ReaderState, key: KeyEvent, has_search: bool) -> Self {
        if key.code != KeyCode::Char('g') && key.code != KeyCode::Char('t') {
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

            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ReaderAction::JumpBack
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

            // Half-page scrolling
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

            // Full-page scrolling
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
                        state.gg_timer = None;
                        ReaderAction::GgTop
                    } else {
                        state.gg_timer = Some(now);
                        ReaderAction::None
                    }
                } else {
                    state.gg_timer = Some(now);
                    ReaderAction::None
                }
            }
            KeyCode::Char('t') if state.gg_timer.is_some() => {
                if let Some(t) = state.gg_timer {
                    if Instant::now().duration_since(t) < Duration::from_millis(300) {
                        state.gg_timer = None;
                        ReaderAction::EnterToc {
                            chapter: state.chapter,
                            cursor_word: state.cursor_word,
                        }
                    } else {
                        state.gg_timer = None;
                        ReaderAction::None
                    }
                } else {
                    ReaderAction::None
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

            _ => ReaderAction::None,
        }
    }
}

// ── RSVP actions ──

pub enum RsvpAction {
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
    pub fn from_key(_state: &crate::tui::rsvp::RsvpState, key: KeyEvent) -> Self {
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

// ── Menu actions ──

pub enum MenuAction {
    None,
    Open,
    Browse,
    Delete,
    Quit,
}

impl MenuAction {
    pub fn from_key(state: &mut MenuState, key: KeyEvent, total_entries: usize) -> Self {
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
                let max_col = state.max_col(total_entries, state.selected_row);
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
            KeyCode::Delete => MenuAction::Delete,
            _ => MenuAction::None,
        }
    }
}

// ── Toc actions ──

pub enum TocAction {
    None,
    MoveUp,
    MoveDown,
    Select,
    Cancel,
    ToggleFilter,
    FilterChar(char),
    FilterBackspace,
    GgTop,
    GBottom,
}

impl TocAction {
    pub fn from_key(state: &mut TocState, key: KeyEvent) -> Self {
        // Filter mode
        if state.filter_active {
            match key.code {
                KeyCode::Esc => {
                    state.filter_active = false;
                    return TocAction::None;
                }
                KeyCode::Enter => {
                    state.filter_active = false;
                    return TocAction::None;
                }
                KeyCode::Backspace => return TocAction::FilterBackspace,
                KeyCode::Char(c) => return TocAction::FilterChar(c),
                _ => return TocAction::None,
            }
        }

        // Clear gg timer on non-g keys
        if key.code != KeyCode::Char('g') {
            state.gg_timer = None;
        }

        match key.code {
            KeyCode::Esc => TocAction::Cancel,
            KeyCode::Enter => TocAction::Select,
            KeyCode::Up | KeyCode::Char('k') => TocAction::MoveUp,
            KeyCode::Down | KeyCode::Char('j') => TocAction::MoveDown,
            KeyCode::Char('/') => TocAction::ToggleFilter,
            KeyCode::Char('g') => {
                let now = std::time::Instant::now();
                if let Some(t) = state.gg_timer {
                    if now.duration_since(t) < std::time::Duration::from_millis(300) {
                        state.gg_timer = None;
                        return TocAction::GgTop;
                    }
                }
                state.gg_timer = Some(now);
                TocAction::None
            }
            KeyCode::Char('G') => TocAction::GBottom,
            _ => TocAction::None,
        }
    }
}

// ── Umbrella action enum ──

pub enum Action {
    Menu(MenuAction),
    Reader(ReaderAction),
    Rsvp(RsvpAction),
    Toc(TocAction),
}
