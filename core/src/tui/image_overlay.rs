//! Full-screen image overlay mode.
//!
//! Entered from the reader when Enter is pressed near an image.
//! The image is rendered via kitty graphics protocol after the
//! ratatui frame. Any key dismisses back to the reader.

use crate::tui::theme::Theme;
use ratatui::{
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub struct ImageOverlayState {
    pub cached_path: String,
    pub img_width: u32,
    pub img_height: u32,
}

impl ImageOverlayState {
    pub fn new(path: String, w: u32, h: u32) -> Self {
        ImageOverlayState {
            cached_path: path,
            img_width: w,
            img_height: h,
        }
    }

    /// Minimal render — just a hint bar. The actual image is emitted
    /// via kitty protocol after terminal.draw() in the run loop.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let hint = Line::from(Span::styled(
            "Press any key to return to reading",
            ratatui::style::Style::default().fg(theme.hud),
        ));
        frame.render_widget(
            Paragraph::new(hint).alignment(Alignment::Center),
            Rect::new(area.x, area.y + area.height.saturating_sub(1), area.width, 1),
        );
    }
}
