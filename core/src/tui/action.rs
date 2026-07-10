//! Reader action enum and keybinding dispatch.
//! Extracted from mod.rs to keep keybindings self-contained.

use crate::tui::reader::ReaderState;
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
