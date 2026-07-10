//! Table of Contents overlay — scrollable chapter list with live filter.
//!
//! Opened from reader mode with the `gt` chord (press `g` then `t` within 300ms).
//! j/k navigate, Enter jumps, Esc cancels, / toggles filter.

use crate::tui::theme::Theme;
use volta_core::doc::Document;

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub struct TocState {
    /// Full list of chapter titles (unfiltered).
    pub chapters: Vec<String>,
    /// Indices into `chapters` matching the current filter.
    pub filtered: Vec<usize>,
    /// Selected row within `filtered`.
    pub selected: usize,
    /// Scroll offset (first visible filtered row).
    pub scroll: usize,
    /// Live filter string.
    pub filter: String,
    /// Whether the filter bar is active (typing).
    pub filter_active: bool,
    /// The chapter we entered from (for highlight).
    pub source_chapter: usize,
    /// Word cursor we entered from (for jump-back stack — pushed by caller).
    pub source_cursor: usize,
    /// Timer for gg chord detection.
    pub gg_timer: Option<std::time::Instant>,
}

impl TocState {
    /// Build TOC from a document. `current_chapter` is highlighted in the list.
    pub fn new(doc: &dyn Document, current_chapter: usize, cursor_word: usize) -> Self {
        let count = doc.chapter_count() as usize;
        let mut chapters = Vec::with_capacity(count);
        for i in 0..count {
            let title = doc.chapter_title(i as u32).to_string();
            chapters.push(if title.is_empty() {
                format!("Chapter {}", i + 1)
            } else {
                title
            });
        }
        let filtered: Vec<usize> = (0..count).collect();
        let sel = current_chapter.min(count.saturating_sub(1));
        TocState {
            chapters,
            filtered,
            selected: sel,
            scroll: 0,
            filter: String::new(),
            filter_active: false,
            source_chapter: current_chapter,
            source_cursor: cursor_word,
            gg_timer: None,
        }
    }

    /// Rebuild the filtered index from the current filter string.
    pub fn apply_filter(&mut self) {
        if self.filter.is_empty() {
            self.filtered = (0..self.chapters.len()).collect();
        } else {
            let q = self.filter.to_lowercase();
            self.filtered = (0..self.chapters.len())
                .filter(|&i| self.chapters[i].to_lowercase().contains(&q))
                .collect();
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
        self.scroll = self.scroll.min(self.filtered.len().saturating_sub(1));
    }

    /// Clamp scroll so the selected row is visible within `height` rows.
    pub fn ensure_visible(&mut self, height: usize) {
        if self.filtered.is_empty() {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected.saturating_sub(height - 1);
        }
    }

    /// Render the TOC overlay.
    pub fn render(&mut self, frame: &mut Frame, _area: Rect, theme: &Theme) {
        let term_w = frame.area().width;
        let term_h = frame.area().height;

        // Overlay dimensions: 60% width, 70% height, centered
        let box_w = ((term_w as f32) * 0.60).max(30.0) as u16;
        let box_h = ((term_h as f32) * 0.70).max(10.0) as u16;
        let box_x = (term_w.saturating_sub(box_w)) / 2;
        let box_y = (term_h.saturating_sub(box_h)) / 2;

        // Dimming backdrop
        let backdrop = Paragraph::new("")
            .style(Style::default().bg(Color::Rgb(0, 0, 0)))
            .block(Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0))));
        frame.render_widget(Clear, Rect::new(box_x, box_y, box_w, box_h));
        frame.render_widget(backdrop, Rect::new(box_x, box_y, box_w, box_h));

        // Border block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.cursor))
            .style(Style::default().bg(Color::Rgb(10, 10, 18)));
        let inner = block.inner(Rect::new(box_x, box_y, box_w, box_h));
        frame.render_widget(block, Rect::new(box_x, box_y, box_w, box_h));

        // Title bar
        let title = format!(
            " Table of Contents  —  {}/{} chapters {}",
            self.filtered.len(),
            self.chapters.len(),
            if self.filter.is_empty() {
                String::new()
            } else {
                format!("(filter: \"{}\")", self.filter)
            }
        );
        let title_line = Line::from(Span::styled(title, Style::default().fg(theme.heading)));
        frame.render_widget(Paragraph::new(title_line), Rect::new(inner.x, inner.y, inner.width, 1));

        // Separator
        let sep = "─".repeat(inner.width as usize);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                &sep,
                Style::default().fg(Color::Rgb(40, 40, 60)),
            ))),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );

        // List area
        let list_top = inner.y + 3;
        let list_h = inner.height.saturating_sub(5); // room for title + sep + filter bar + footer
        let visible = list_h as usize;

        self.ensure_visible(visible);

        if self.filtered.is_empty() {
            let msg = Line::from(Span::styled(
                "(no matching chapters)",
                Style::default().fg(Color::Gray),
            ));
            frame.render_widget(Paragraph::new(msg), Rect::new(inner.x + 2, list_top, inner.width, 1));
        } else {
            for row in 0..visible {
                let fi = self.scroll + row;
                if fi >= self.filtered.len() {
                    break;
                }
                let ci = self.filtered[fi];
                let is_sel = fi == self.selected;
                let is_current = ci == self.source_chapter;

                let prefix = if is_sel { "▶" } else { " " };
                let text = format!(" {} {:3}. {}", prefix, ci + 1, self.chapters[ci]);
                let truncated = truncate(&text, inner.width.saturating_sub(4) as usize);

                let fg = if is_sel {
                    theme.cursor
                } else if is_current {
                    Color::Rgb(100, 100, 160)
                } else {
                    theme.text
                };
                let bg = if is_sel {
                    Color::Rgb(30, 20, 45)
                } else {
                    Color::Rgb(10, 10, 18)
                };

                let style = Style::default().fg(fg).bg(bg);
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(truncated, style))),
                    Rect::new(inner.x + 2, list_top + row as u16, inner.width.saturating_sub(4), 1),
                );
            }
        }

        // Filter bar
        let filter_y = inner.y + inner.height.saturating_sub(3);
        if self.filter_active {
            let prompt = format!("/{}", self.filter);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    prompt,
                    Style::default().fg(theme.cursor),
                ))),
                Rect::new(inner.x + 2, filter_y, inner.width.saturating_sub(4), 1),
            );
        }

        // Footer
        let footer_y = inner.y + inner.height.saturating_sub(1);
        let footer = " j/k: move  Enter: jump  /: filter  gg/G: top/bottom  Esc: cancel ";
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                footer,
                Style::default().fg(theme.hud),
            ))),
            Rect::new(inner.x + 1, footer_y, inner.width.saturating_sub(2), 1),
        );
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!(
            "{}…",
            s.chars().take(max_len.saturating_sub(1)).collect::<String>()
        )
    }
}
