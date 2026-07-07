//! RSVP mode — single-word display with ORP highlighting.
//!
//! Port of the LÖVE `rsvp.lua` to ratatui. Renders one word at a
//! time centered on screen, with the ORP pivot character highlighted
//! in the theme's accent color.

use volta_core::doc::Document;
use volta_core::player::PlayerState;
use crate::tui::theme::Theme;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub struct RsvpState {
    pub wpm: u32,
    pub show_stats: bool,
}

impl RsvpState {
    pub fn new() -> Self {
        RsvpState {
            wpm: 300,
            show_stats: false,
        }
    }

    /// Render the RSVP word display.
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        player: &PlayerState,
        doc: &dyn Document,
    ) {
        let idx = player.current() as usize;
        let total = doc.word_count() as usize;
        let word = doc.word_at(idx as u32).text.as_str();

        // ── Center word with ORP highlighting ──
        let orp_spans = render_orp_word(word, 0.4, theme);
        let word_line = Line::from(orp_spans);

        let word_y = area.y + area.height.saturating_sub(2) / 2;
        frame.render_widget(
            Paragraph::new(word_line).alignment(Alignment::Center),
            Rect::new(area.x, word_y, area.width, 1),
        );

        // ── Bottom HUD ──
        let ch = doc.word_at(idx as u32).chapter_index as usize;
        let hud = format!(
            "WPM: {}  |  Word: {}/{}  |  Chapter: {}/{}",
            self.wpm,
            idx + 1,
            total,
            ch + 1,
            doc.chapter_count(),
        );
        let hud_line = Line::from(Span::styled(hud, Style::default().fg(theme.hud)));
        frame.render_widget(
            Paragraph::new(hud_line),
            Rect::new(area.x, area.y + area.height - 1, area.width, 1),
        );

        // ── Progress bar ──
        if total > 0 {
            let bar_width = area.width as usize;
            let filled = (bar_width * (idx + 1)) / total;
            let bar = "█".repeat(filled) + &"░".repeat(bar_width.saturating_sub(filled));
            let bar_line = Line::from(Span::styled(bar, Style::default().fg(theme.progress)));
            frame.render_widget(
                Paragraph::new(bar_line),
                Rect::new(area.x, area.y + area.height - 2, area.width, 1),
            );
        }

        // ── Stats overlay ──
        if self.show_stats {
            let pct = if total > 0 {
                ((idx + 1) as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            let stats = format!(
                "  WPM: {}\n  Words: {} / {}\n  Chapter: {} / {}\n  Progress: {:.1}%  ",
                self.wpm,
                idx + 1,
                total,
                ch + 1,
                doc.chapter_count(),
                pct,
            );
            let stats_line = Line::from(Span::styled(
                stats,
                Style::default().fg(theme.hud).bg(Color::Rgb(20, 20, 30)),
            ));
            let sx = area.x + area.width.saturating_sub(30) / 2;
            let sy = area.y + area.height.saturating_sub(6) / 2;
            frame.render_widget(
                Paragraph::new(stats_line),
                Rect::new(sx, sy, 30, 4),
            );
        }
    }
}

/// Split a word into (left, pivot, right) parts for ORP display.
/// `pivot_frac` is the fraction into the word for the fixation point
/// (0.4 = 40% in, standard ORP).
fn render_orp_word<'a>(word: &'a str, pivot_frac: f32, theme: &Theme) -> Vec<Span<'a>> {
    let char_count = word.chars().count();
    if char_count == 0 {
        return vec![Span::raw("")];
    }

    let pivot_idx = ((char_count as f32 * pivot_frac) as usize).min(char_count.saturating_sub(1));

    let left: String = word.chars().take(pivot_idx).collect();
    let pivot: String = word.chars().skip(pivot_idx).take(1).collect();
    let right: String = word.chars().skip(pivot_idx + 1).collect();

    vec![
        Span::styled(left, Style::default().fg(theme.orp_fade)),
        Span::styled(pivot, Style::default().fg(theme.orp)),
        Span::styled(right, Style::default().fg(theme.orp_fade)),
    ]
}
