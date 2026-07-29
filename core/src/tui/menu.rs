//! Library grid — card-based book browser filling the terminal.
//!
//! Cards are arranged in a grid. Arrow keys navigate. Enter opens.
//! In kitty terminals, cover thumbnails are displayed via the
//! kitty graphics protocol.

use crate::tui::theme::Theme;
use volta_core::library::LibraryEntry;

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use std::path::PathBuf;

/// Card dimensions (in terminal cells).
pub const CARD_W: u16 = 26;
pub const CARD_H: u16 = 7;

pub struct MenuState {
    /// Selected card position: (col, row) in the grid.
    pub selected_col: usize,
    pub selected_row: usize,
    pub scroll: usize,
    /// Grid layout for current terminal size.
    pub cols: usize,
}

impl MenuState {
    pub fn new() -> Self {
        MenuState {
            selected_col: 0,
            selected_row: 0,
            cols: 1,
            scroll: 0,
        }
    }

    pub fn selected_path(&self, entries: &[(&str, &LibraryEntry)]) -> Option<PathBuf> {
        let idx = self.selected_row * self.cols + self.selected_col;
        entries.get(idx).map(|(path, _)| PathBuf::from(*path))
    }

    /// Render the full-screen card grid.
    pub fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        entries: &[(&str, &LibraryEntry)],
    ) {
        let header_height = 1u16;
        let footer_height = 1;
        let avail_height = area.height.saturating_sub(header_height + footer_height);
        let row_height = CARD_H + 1;

        let visible_rows = if avail_height >= CARD_H {
            (avail_height / row_height).max(1) as usize
        } else {
            0
        };

        let cols = (area.width.saturating_sub(2) / (CARD_W + 1)).max(1) as usize;
        self.cols = cols;
        let total_rows = entries.len().div_ceil(cols);

        // Empty state
        if entries.is_empty() {
            let msg = Paragraph::new("No books yet.\n\nPress Ctrl+O to browse for a file.")
                .style(Style::default().fg(theme.text))
                .alignment(Alignment::Center);
            frame.render_widget(msg, area);
            return;
        }

        // Clamp selection and scroll
        if self.selected_row >= total_rows {
            self.selected_row = total_rows.saturating_sub(1);
        }
        let max_col = self.max_col(entries.len(), self.selected_row);
        if self.selected_col > max_col {
            self.selected_col = max_col;
        }

        // Adjust scroll so selected row is visible
        if self.selected_row < self.scroll {
            self.scroll = self.selected_row;
        } else if self.selected_row >= self.scroll + visible_rows {
            self.scroll = self.selected_row - visible_rows + 1;
        }
        if total_rows > visible_rows {
            self.scroll = self.scroll.min(total_rows - visible_rows);
        } else {
            self.scroll = 0;
        }

        // Render visible cards
        let start_row = self.scroll;
        let end_row = (self.scroll + visible_rows).min(total_rows);
        for row in start_row..end_row {
            for col in 0..cols {
                let idx = row * cols + col;
                if idx >= entries.len() {
                    continue;
                }
                let (_path, entry) = entries[idx];
                let x = area.x + 1 + col as u16 * (CARD_W + 1);
                let y = area.y + header_height + (row - start_row) as u16 * (CARD_H + 1);
                let card_area = Rect::new(x, y, CARD_W, CARD_H);
                let is_selected =
                    col == self.selected_col && row == self.selected_row;
                self.render_card(frame, card_area, entry, is_selected, theme);
            }
        }

        // Draw scrollbar if needed
        if total_rows > visible_rows {
            let bar_x = area.x + area.width - 1;
            let bar_y = area.y + header_height;
            let bar_height = avail_height;
            let thumb_height =
                ((visible_rows as f32 / total_rows as f32) * bar_height as f32) as u16;
            let thumb_y = ((self.scroll as f32 / (total_rows - visible_rows) as f32)
                * (bar_height - thumb_height) as f32) as u16;
            for y in 0..bar_height {
                let ch = if y >= thumb_y && y < thumb_y + thumb_height {
                    '█'
                } else {
                    '░'
                };
                frame.render_widget(
                    ratatui::widgets::Paragraph::new(ch.to_string()),
                    Rect::new(bar_x, bar_y + y, 1, 1),
                );
            }
        }
    }

    /// Render a single card.
    fn render_card(
        &self,
        frame: &mut Frame,
        area: Rect,
        entry: &LibraryEntry,
        selected: bool,
        theme: &Theme,
    ) {
        let border_color = if selected {
            theme.cursor
        } else {
            Color::Rgb(60, 60, 60)
        };
        let bg = if selected {
            Color::Rgb(30, 20, 40)
        } else {
            Color::Rgb(10, 10, 15)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(bg));

        let (icon, fmt_label) = match entry.format.as_str() {
            "epub" => ("📖", "EPUB"),
            "pdf" => ("📄", "PDF"),
            "md" => ("📝", "MD"),
            _ => ("📘", "?"),
        };

        let pct = if entry.chapter_count > 0 {
            (entry.current_chapter as f64 / entry.chapter_count as f64 * 100.0) as u32
        } else {
            0
        };
        let bar_w = (area.width.saturating_sub(4)) as usize;
        let filled = (bar_w as f64 * pct as f64 / 100.0) as usize;
        let empty = bar_w.saturating_sub(filled);
        let bar = format!("{}{} {}%", "█".repeat(filled), "░".repeat(empty), pct);

        let inner = block.inner(area);

        let header = Line::from(vec![Span::styled(
            format!("{} {}", icon, fmt_label),
            Style::default().fg(theme.heading),
        )]);

        let title = truncate(&entry.title, inner.width.saturating_sub(2) as usize);
        let title_line = Line::from(Span::styled(
            title,
            Style::default().fg(if selected { theme.cursor } else { theme.text }),
        ));

        let author = if entry.author.is_empty() {
            "".to_string()
        } else {
            truncate(&entry.author, inner.width.saturating_sub(2) as usize)
        };
        let author_line = Line::from(Span::styled(author, Style::default().fg(Color::Gray)));

        let bar_line = Line::from(Span::styled(
            &bar,
            Style::default().fg(if pct > 0 { theme.cursor } else { Color::Gray }),
        ));

        let lines = vec![header, Line::from(""), title_line, author_line, bar_line];

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }

    pub fn max_col(&self, total: usize, row: usize) -> usize {
        if total == 0 {
            return 0;
        }
        let remaining = total.saturating_sub(row * self.cols).min(self.cols);
        remaining.saturating_sub(1)
    }

    pub fn max_row(&self, total: usize) -> usize {
        if total == 0 {
            return 0;
        }
        (total.saturating_sub(1)) / self.cols
    }

    /// Spawn zenity file picker, return chosen path if any.
    pub fn browse_file() -> Option<PathBuf> {
        let output = std::process::Command::new("zenity")
            .args(["--file-selection", "--title=Open Book"])
            .output()
            .ok()?;
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
        None
    }
}
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!(
            "{}…",
            s.chars()
                .take(max_len.saturating_sub(1))
                .collect::<String>()
        )
    }
}
