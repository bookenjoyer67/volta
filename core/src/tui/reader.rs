//! Reader mode — word-wrapped text display with vim-style cursor.
//!
//! Port of the LÖVE `reader.lua` to ratatui. Same logic: wrap
//! chapter text to terminal width, track a `cursor_word` index,
//! highlight it, and scroll to keep it visible.

use crate::tui::theme::Theme;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::collections::HashSet;
use std::time::Instant;
use volta_core::doc::Document;

pub struct ReaderState {
    pub chapter: usize,
    pub scroll: usize,
    pub cursor_word: usize,
    pub wrapped_lines: Vec<String>,
    pub line_word_offsets: Vec<usize>,
    pub gg_timer: Option<Instant>,
}

impl ReaderState {
    /// Create a reader at the start of the document.
    pub fn new(doc: &dyn Document) -> Self {
        let mut state = ReaderState {
            chapter: 0,
            scroll: 0,
            cursor_word: 0,
            wrapped_lines: Vec::new(),
            line_word_offsets: Vec::new(),
            gg_timer: None,
        };
        state.reflow(doc, 80);
        state
    }

    /// Re-wrap the current chapter to fit `width` columns.
    pub fn reflow(&mut self, doc: &dyn Document, width: u16) {
        let text = doc.chapter_text(self.chapter as u32);
        let max_width = width.saturating_sub(2) as usize; // margin
        let (lines, offsets) = wrap_text(text, max_width);
        self.wrapped_lines = lines;
        self.line_word_offsets = offsets;

        // Clamp cursor
        let max_word = self.line_word_offsets.last().copied().unwrap_or(0);
        self.cursor_word = self.cursor_word.min(max_word);
        self.scroll_to_cursor(width);
    }

    /// Find which wrapped line contains cursor_word.
    pub fn cursor_line(&self) -> usize {
        for i in (0..self.line_word_offsets.len()).rev() {
            if self.line_word_offsets[i] <= self.cursor_word {
                return i;
            }
        }
        0
    }

    /// Adjust scroll so the cursor line is visible.
    pub fn scroll_to_cursor(&mut self, _height: u16) {
        let line = self.cursor_line();
        let visible_height = 20usize; // rough; refined during render
        if line < self.scroll {
            self.scroll = line.saturating_sub(1);
        } else if line >= self.scroll + visible_height {
            self.scroll = line.saturating_sub(visible_height - 2);
        }
    }

    /// Draw the reader view.
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        doc: &dyn Document,
        search_matches: &[(usize, usize)],
        search_idx: usize,
    ) {
        let visible_height = area.height.saturating_sub(4) as usize; // title + status bars

        // Build set of (ch, word_offset) match positions in the current chapter
        let match_set: HashSet<usize> = search_matches
            .iter()
            .filter(|(ch, _)| *ch == self.chapter)
            .map(|(_, wo)| *wo)
            .collect();

        let title = format!(
            "{}  |  Chapter {}/{}",
            doc.title(),
            self.chapter + 1,
            doc.chapter_count()
        );

        // Title bar
        let title_line = Line::from(Span::styled(title, Style::default().fg(theme.heading)));
        frame.render_widget(
            Paragraph::new(title_line),
            Rect::new(area.x, area.y, area.width, 1),
        );

        // Text area
        let text_area = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(3),
        );
        let mut lines: Vec<Line> = Vec::new();

        let start = self.scroll;
        let end = (start + visible_height).min(self.wrapped_lines.len());

        for i in start..end {
            let line_text = &self.wrapped_lines[i];

            // Build spans for this line, highlighting cursor + matches
            let words: Vec<&str> = line_text.split_whitespace().collect();
            let first_word = self.line_word_offsets[i];
            let mut spans = Vec::new();
            let mut byte_pos = 0;

            for (wi, word) in words.iter().enumerate() {
                let global_word = first_word + wi;

                // Find this word's byte position in the line
                while byte_pos < line_text.len()
                    && line_text.as_bytes()[byte_pos].is_ascii_whitespace()
                {
                    byte_pos += 1;
                }
                let word_start = byte_pos;
                let word_end = word_start + word.len();

                let style = if global_word == self.cursor_word {
                    // Cursor word
                    Style::default()
                        .fg(theme.cursor)
                        .bg(Color::Rgb(60, 20, 50))
                } else if match_set.contains(&global_word) {
                    // Search match
                    Style::default()
                        .fg(Color::Rgb(255, 200, 50))
                        .bg(Color::Rgb(50, 40, 10))
                } else {
                    Style::default().fg(theme.text)
                };

                spans.push(Span::styled(
                    &line_text[word_start..word_end],
                    style,
                ));

                byte_pos = word_end;
                // Add trailing space if not last word
                if wi < words.len() - 1 && byte_pos < line_text.len() {
                    spans.push(Span::raw(" "));
                    byte_pos += 1;
                }
            }
            lines.push(Line::from(spans));
        }

        frame.render_widget(Paragraph::new(lines), text_area);

        // Scrollbar on right edge
        let sb_x = area.x + area.width.saturating_sub(2);
        let sb_h = text_area.height.saturating_sub(0) as usize;
        if sb_h > 0 && area.width > 20 {
            let max_scroll = self.wrapped_lines.len().saturating_sub(1).max(1);
            let thumb_h = ((sb_h as f64) * (sb_h as f64 / max_scroll as f64)).max(1.0) as usize;
            let thumb_y = if max_scroll > 1 {
                ((self.scroll as f64 / max_scroll as f64) * (sb_h - thumb_h) as f64) as u16
            } else {
                0
            };

            // Track (dim)
            for row in 0..sb_h as u16 {
                let y = text_area.y + row;
                if y < area.y + area.height {
                    frame.buffer_mut()
                        .get_mut(sb_x, y)
                        .set_char('│')
                        .set_fg(Color::Rgb(30, 30, 45))
                        .set_bg(Color::Reset);
                }
            }

            // Thumb (accent)
            for row in 0..thumb_h as u16 {
                let y = text_area.y + thumb_y + row;
                if y < area.y + area.height {
                    let cell = frame.buffer_mut().get_mut(sb_x, y);
                    cell.set_char('█').set_fg(theme.cursor).set_bg(Color::Reset);
                }
            }

            // Global position dot
            let total_ch = doc.chapter_count() as usize;
            if total_ch > 0 && max_scroll > 0 {
                let ch_ratio = self.chapter as f64 / total_ch as f64;
                let scroll_ratio = self.scroll as f64 / max_scroll as f64;
                let global_pct = ch_ratio + scroll_ratio / total_ch as f64;
                let dot_y = text_area.y + (global_pct * sb_h as f64) as u16;
                if dot_y < area.y + area.height {
                    let cell = frame.buffer_mut().get_mut(sb_x.saturating_sub(1), dot_y);
                    cell.set_char('·')
                        .set_fg(Color::Rgb(80, 80, 120))
                        .set_bg(Color::Reset);
                }
            }
        }

        // Status bar
        let visible = visible_height;
        let pages = (self
            .wrapped_lines
            .len()
            .saturating_add(visible.saturating_sub(1)))
            / visible.max(1);
        let current_page = (self.scroll / visible.max(1)) + 1;

        let status = if !search_matches.is_empty() {
            format!(
                "Match {}/{}  |  Page {}/{}  |  Word {}  |  {}  |  n/N next/prev  Esc clear",
                search_idx + 1,
                search_matches.len(),
                current_page.min(pages.max(1)),
                pages.max(1),
                self.cursor_word + 1,
                theme.name,
            )
        } else {
            format!(
                "Page {}/{}  |  Word {}  |  {}  |  / search  n/p chapter  j/k scroll  r RSVP",
                current_page.min(pages.max(1)),
                pages.max(1),
                self.cursor_word + 1,
                theme.name,
            )
        };
        let status_line = Line::from(Span::styled(status, Style::default().fg(theme.hud)));
        frame.render_widget(
            Paragraph::new(status_line),
            Rect::new(area.x, area.y + area.height - 1, area.width, 1),
        );
    }
}

/// Wrap text to fit `max_width` columns (in bytes — good enough for
/// ASCII-heavy prose; CJK would need grapheme-aware wrapping).
/// Returns (lines, word_offsets) where word_offsets[i] is the index
/// of the first word on line i.
fn wrap_text(text: &str, max_width: usize) -> (Vec<String>, Vec<usize>) {
    let mut lines: Vec<String> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    let mut current = String::new();
    let mut word_idx = 0usize;
    let mut line_start = 0usize;

    for word in text.split_whitespace() {
        let test = if current.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current, word)
        };

        if test.len() > max_width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            offsets.push(line_start);
            current = word.to_string();
            line_start = word_idx;
        } else {
            current = test;
        }

        word_idx += 1;
    }

    if !current.is_empty() {
        lines.push(current);
        offsets.push(line_start);
    }

    // Ensure we always have at least one line
    if lines.is_empty() {
        lines.push(String::new());
        offsets.push(0);
    }

    (lines, offsets)
}
