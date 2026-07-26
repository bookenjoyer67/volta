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
use unicode_width::UnicodeWidthStr;
use volta_core::doc::Document;

pub struct ReaderState {
    pub chapter: usize,
    pub scroll: usize,
    pub cursor_word: usize,
    pub wrapped_lines: Vec<String>,
    pub line_word_offsets: Vec<usize>,
    pub gg_timer: Option<Instant>,
    /// Columns of margin on each side of the text.
    pub margin: u16,
    /// Max text column width (0 = fill available width). Centered when set.
    pub max_col_width: u16,
    /// Width the text was last reflowed to (for centering in render).
    pub content_width: u16,
    /// Visible text rows at the last render — used by scroll_to_cursor.
    pub last_visible_height: usize,
    /// Inputs of the last reflow: (chapter, width, margin, max_col_width).
    reflow_key: Option<(usize, u16, u16, u16)>,
    /// Visual selection anchor (None = not in visual mode).
    pub selection_anchor: Option<usize>,
    /// When true, selection is line-wise (V instead of v).
    pub visual_line_mode: bool,
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
            margin: 2,
            max_col_width: 0,
            content_width: 0,
            last_visible_height: 20,
            reflow_key: None,
            selection_anchor: None,
            visual_line_mode: false,
        };
        state.reflow(doc, 80);
        state
    }

    /// Reflow only if the reflow inputs changed since last time.
    /// Returns true if a reflow happened.
    pub fn reflow_if_needed(&mut self, doc: &dyn Document, width: u16) -> bool {
        let key = (self.chapter, width, self.margin, self.max_col_width);
        if self.reflow_key == Some(key) {
            return false;
        }
        self.reflow(doc, width);
        true
    }

    /// Re-wrap the current chapter to fit `width` columns.
    pub fn reflow(&mut self, doc: &dyn Document, width: u16) {
        let text = doc.chapter_text(self.chapter as u32);
        let avail = width.saturating_sub(2 * self.margin);
        let max_width = if self.max_col_width > 0 {
            avail.min(self.max_col_width)
        } else {
            avail
        };
        self.content_width = max_width;
        self.reflow_key = Some((self.chapter, width, self.margin, self.max_col_width));
        let (lines, offsets) = wrap_text(text, max_width as usize);
        self.wrapped_lines = lines;
        self.line_word_offsets = offsets;

        // Clamp cursor
        let max_word = self.line_word_offsets.last().copied().unwrap_or(0);
        self.cursor_word = self.cursor_word.min(max_word);
        self.scroll_to_cursor();
    }

    /// Find which wrapped line contains cursor_word.
    /// Offsets are ascending, so binary-search for the last offset
    /// that is <= cursor_word.
    pub fn cursor_line(&self) -> usize {
        self.line_word_offsets
            .partition_point(|&o| o <= self.cursor_word)
            .saturating_sub(1)
    }

    /// Adjust scroll so the cursor line is visible.
    pub fn scroll_to_cursor(&mut self) {
        let line = self.cursor_line();
        let visible_height = self.last_visible_height.max(1);
        if line < self.scroll {
            self.scroll = line.saturating_sub(1);
        } else if line >= self.scroll + visible_height {
            self.scroll = line.saturating_sub(visible_height - 2);
        }
    }

    /// Draw the reader view.
    pub fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        doc: &dyn Document,
        search_matches: &[(usize, usize)],
        search_idx: usize,
    ) {
        let visible_height = area.height.saturating_sub(4) as usize; // title + status bars
        self.last_visible_height = visible_height;

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

        // Text area (centered when a max column width is set)
        let text_w = if self.content_width > 0 && self.content_width < area.width {
            self.content_width
        } else {
            area.width
        };
        let text_x = area.x + (area.width.saturating_sub(text_w)) / 2;
        let text_area = Rect::new(
            text_x,
            area.y + 1,
            text_w,
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

            // Preserve leading whitespace (paragraph indent) as a raw span
            let lead = line_text.len() - line_text.trim_start().len();
            if lead > 0 {
                spans.push(Span::raw(&line_text[..lead]));
                byte_pos = lead;
            }

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
                    // Cursor word — brightest
                    Style::default()
                        .fg(theme.cursor)
                        .bg(Color::Rgb(60, 20, 50))
                } else if match_set.contains(&global_word) {
                    // Search match
                    Style::default()
                        .fg(Color::Rgb(255, 200, 50))
                        .bg(Color::Rgb(50, 40, 10))
                } else if let Some(anchor) = self.selection_anchor {
                    let start = anchor.min(self.cursor_word);
                    let end = anchor.max(self.cursor_word);
                    if global_word >= start && global_word <= end {
                        Style::default()
                            .fg(theme.text)
                            .bg(Color::Rgb(40, 50, 80))
                    } else {
                        Style::default().fg(theme.text)
                    }
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
                    frame.buffer_mut()[(sb_x, y)]
                        .set_char('│')
                        .set_fg(Color::Rgb(30, 30, 45))
                        .set_bg(Color::Reset);
                }
            }

            // Thumb (accent)
            for row in 0..thumb_h as u16 {
                let y = text_area.y + thumb_y + row;
                if y < area.y + area.height {
                    let cell = &mut frame.buffer_mut()[(sb_x, y)];
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
                    let cell = &mut frame.buffer_mut()[(sb_x.saturating_sub(1), dot_y)];
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
        let chapter_pct = {
            let total = self.line_word_offsets.last().copied().unwrap_or(0);
            if total > 0 {
                (self.cursor_word * 100) / total
            } else {
                100
            }
        };

        let status = if !search_matches.is_empty() {
            format!(
                "Match {}/{}  |  Page {}/{}  |  {}%  |  {}  |  n/N next/prev  Esc clear",
                search_idx + 1,
                search_matches.len(),
                current_page.min(pages.max(1)),
                pages.max(1),
                chapter_pct,
                theme.name,
            )
        } else {
            format!(
                "Page {}/{}  |  {}%  |  {}  |  / search  n/p chapter  j/k scroll  r RSVP  <>/{{}} width",
                current_page.min(pages.max(1)),
                pages.max(1),
                chapter_pct,
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

/// Wrap text to fit `max_width` display columns. Paragraph-aware:
/// splits on blank lines and indents the first line of every paragraph
/// by 4 columns. Widths are measured in display columns via
/// unicode-width (CJK/emoji take 2 columns), not bytes.
/// Returns (lines, word_offsets) where word_offsets[i] is the index
/// of the first word on line i.
const INDENT: usize = 4;

fn wrap_text(text: &str, max_width: usize) -> (Vec<String>, Vec<usize>) {
    let mut lines: Vec<String> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    let mut word_idx = 0usize;

    for paragraph in text.split("\n\n") {
        let mut current = String::new();
        let mut line_start = word_idx;
        let mut first_line = true;

        for word in paragraph.split_whitespace() {
            // First line of a paragraph reserves room for the indent.
            let capacity = if first_line {
                max_width.saturating_sub(INDENT)
            } else {
                max_width
            };
            let test = if current.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current, word)
            };

            if UnicodeWidthStr::width(test.as_str()) > capacity && !current.is_empty() {
                if first_line {
                    let mut indented = String::with_capacity(INDENT + current.len());
                    indented.push_str("    ");
                    indented.push_str(&current);
                    lines.push(indented);
                } else {
                    lines.push(current.clone());
                }
                offsets.push(line_start);
                current.clear();
                first_line = false;
                line_start = word_idx;
                current.push_str(word);
            } else {
                current = test;
            }

            word_idx += 1;
        }

        if !current.is_empty() {
            if first_line {
                let mut indented = String::with_capacity(INDENT + current.len());
                indented.push_str("    ");
                indented.push_str(&current);
                lines.push(indented);
            } else {
                lines.push(current);
            }
            offsets.push(line_start);
        }
    }

    // Ensure we always have at least one line
    if lines.is_empty() {
        lines.push(String::new());
        offsets.push(0);
    }

    (lines, offsets)
}

#[cfg(test)]
mod tests {
    use super::wrap_text;

    #[test]
    fn paragraphs_are_indented_and_offsets_track_words() {
        let text = "alpha beta gamma delta epsilon zeta eta theta\n\nsecond para starts here with more words to wrap around";
        let (lines, offsets) = wrap_text(text, 30);

        // Every paragraph's first line starts with the 4-col indent
        assert!(lines[0].starts_with("    alpha"), "first line: {:?}", lines[0]);

        // Find the first line of paragraph 2: "second" is word idx 8
        let p2_line = offsets.iter().position(|&o| o == 8).expect("no line starts at word 8");
        assert!(lines[p2_line].starts_with("    second"), "p2 line: {:?}", lines[p2_line]);

        // Continuation lines are NOT indented
        for (i, line) in lines.iter().enumerate() {
            let is_para_start = i == 0 || i == p2_line;
            if !is_para_start {
                assert!(!line.starts_with("    "), "continuation line indented: {:?}", line);
            }
        }

        // Word offsets are contiguous and cover all words (indent counted nowhere)
        let total_words = text.split_whitespace().count();
        let covered: usize = lines.iter().map(|l| l.split_whitespace().count()).sum();
        assert_eq!(covered, total_words);
    }

    #[test]
    fn single_paragraph_still_indents() {
        let (lines, offsets) = wrap_text("one two three", 80);
        assert_eq!(lines, vec!["    one two three"]);
        assert_eq!(offsets, vec![0]);
    }

    #[test]
    fn empty_text_yields_one_empty_line() {
        let (lines, offsets) = wrap_text("", 80);
        assert_eq!(lines.len(), 1);
        assert_eq!(offsets, vec![0]);
    }

    #[test]
    fn cjk_width_counts_display_columns_not_bytes() {
        // Each CJK char is 2 display columns but 3 bytes.
        // With width 8 and a 4-col indent on the first line, only
        // two chars (4 cols) fit on line 1; the rest wrap.
        let text = "一 二 三 四";
        let (lines, _) = wrap_text(text, 9);
        assert_eq!(lines[0], "    一 二"); // 4 indent + 5 cols = 9
        assert_eq!(lines[1], "三 四"); // 5 cols <= 9
    }
}
