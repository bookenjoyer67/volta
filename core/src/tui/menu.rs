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
    /// Grid layout for current terminal size.
    pub cols: usize,
}

impl MenuState {
    pub fn new() -> Self {
        MenuState {
            selected_col: 0,
            selected_row: 0,
            cols: 1,
        }
    }

    pub fn selected_path(
        &self,
        entries: &[(&str, &LibraryEntry)],
    ) -> Option<PathBuf> {
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
        // Empty state
        if entries.is_empty() {
            let msg = Paragraph::new("No books yet.\n\nPress Ctrl+O to browse for a file.")
                .style(Style::default().fg(theme.text))
                .alignment(Alignment::Center);
            frame.render_widget(msg, area);
            return;
        }

        // Calculate grid and sync state
        self.cols = (area.width.saturating_sub(2) / (CARD_W + 1)).max(1) as usize;

        // Render cards
        for (i, (_path, entry)) in entries.iter().enumerate() {
            let col = (i % self.cols) as u16;
            let row = (i / self.cols) as u16;
            let card_x = area.x + 1 + col * (CARD_W + 1);
            let card_y = area.y + 1 + row * (CARD_H + 1);

            let card_area = Rect::new(card_x, card_y, CARD_W, CARD_H);
            if card_area.y + CARD_H > area.y + area.height {
                break;
            }

            let is_selected = col as usize == self.selected_col
                && row as usize == self.selected_row;

            self.render_card(frame, card_area, entry, is_selected, theme);
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

        let ch_info = if entry.chapter_count > 0 {
            format!("Ch {}/{}", entry.current_chapter + 1, entry.chapter_count)
        } else {
            String::new()
        };
        let time_ago = relative_time(entry.last_opened);
        let footer = Line::from(Span::styled(
            format!("{}  ·  {}", ch_info, time_ago),
            Style::default().fg(theme.hud),
        ));

        let lines = vec![
            header,
            Line::from(""),
            title_line,
            author_line,
            bar_line,
            footer,
        ];

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }

    pub fn max_col(&self, total: usize) -> usize {
        if total == 0 {
            return 0;
        }
        total.saturating_sub(1) % self.cols
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
            .args(&["--file-selection", "--title=Open Book"])
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

fn relative_time(ts: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let diff = now.saturating_sub(ts);

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else if diff < 604800 {
        format!("{}d ago", diff / 86400)
    } else {
        "a while ago".to_string()
    }
}
