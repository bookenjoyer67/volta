//! Menu mode — welcome screen with recent files and continue reading.
//!
//! Mirrors the LÖVE `ui/menu.lua`. Shows Volta title, "Continue reading"
//! for the last book with saved progress, recent files list, and footer.

use crate::tui::theme::Theme;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// A single entry in the menu list.
pub struct MenuEntry {
    pub path: PathBuf,
    pub label: String,       // filename
    pub extra: String,       // e.g. "(Ch. 5)"
    pub is_continue: bool,   // true = "Continue reading" item
}

pub struct MenuState {
    pub entries: Vec<MenuEntry>,
    pub selected: usize, // 0-based index
}

/// Saved progress entry matching the Lua progress.json schema.
#[derive(Deserialize, Default)]
struct ProgressEntry {
    cursor_word: Option<usize>,
}

impl MenuState {
    /// Load recent files and progress data from ~/.local/share/volta/
    pub fn load() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        let data_dir = format!("{}/.local/share/volta", home);
        let recent_path = format!("{}/recent.txt", data_dir);
        let progress_path = format!("{}/progress.json", data_dir);

        let recent: Vec<String> = fs::read_to_string(&recent_path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();

        let progress: HashMap<String, ProgressEntry> =
            fs::read_to_string(&progress_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();

        let mut entries = Vec::new();

        // "Continue reading" — first recent file with saved progress
        if let Some(first) = recent.first() {
            if let Some(prog) = progress.get(first) {
                let filename = Self::filename(first);
                let extra = if let Some(ch) = prog.cursor_word {
                    format!("(Ch. {})", ch + 1)
                } else {
                    String::new()
                };
                entries.push(MenuEntry {
                    path: PathBuf::from(first),
                    label: filename,
                    extra,
                    is_continue: true,
                });
            }
        }

        // Recent files (skip first if it's the continue item)
        let skip = if entries.is_empty() { 0 } else { 1 };
        for path_str in recent.iter().skip(skip).take(10) {
            let filename = Self::filename(path_str);
            let extra = progress.get(path_str).and_then(|p| p.cursor_word).map_or(
                String::new(),
                |ch| format!("(Ch. {})", ch + 1),
            );
            entries.push(MenuEntry {
                path: PathBuf::from(path_str),
                label: filename,
                extra,
                is_continue: false,
            });
        }

        MenuState {
            entries,
            selected: 0, // pre-select "Continue reading" if it exists
        }
    }

    fn filename(path: &str) -> String {
        std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_string()
    }

    /// Render the menu screen.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let w = area.width;
        let h = area.height;

        // ── Title ──
        let title = Paragraph::new("Volta")
            .style(Style::default().fg(theme.heading))
            .alignment(Alignment::Center);
        frame.render_widget(title, Rect::new(area.x, area.y + 2, w, 1));

        // ── Subtitle ──
        let subtitle = Paragraph::new("EPUB & PDF reader with RSVP speed reading")
            .style(Style::default().fg(theme.text))
            .alignment(Alignment::Center);
        frame.render_widget(subtitle, Rect::new(area.x, area.y + 3, w, 1));

        let mut y = area.y + 6;

        // ── Entries ──
        for (i, entry) in self.entries.iter().enumerate() {
            if y >= area.y + h - 2 {
                break;
            }

            let is_selected = i == self.selected;
            let color = if is_selected {
                theme.cursor
            } else {
                theme.text
            };

            let prefix: String = if entry.is_continue {
                if is_selected { "> Continue: " } else { "  Continue: " }.to_string()
            } else {
                let num = if self.has_continue() { i } else { i + 1 };
                if is_selected {
                    format!("> {}. ", num)
                } else {
                    format!("  {}. ", num)
                }
            };

            let line = if entry.extra.is_empty() {
                Line::from(Span::styled(
                    format!("{}{}", prefix, entry.label),
                    Style::default().fg(color),
                ))
            } else {
                Line::from(vec![
                    Span::styled(format!("{}{}  ", prefix, entry.label), Style::default().fg(color)),
                    Span::styled(&entry.extra, Style::default().fg(Color::Gray)),
                ])
            };

            frame.render_widget(Paragraph::new(line), Rect::new(area.x + 4, y, w - 4, 1));
            y += 2;
        }

        // ── Footer instructions ──
        let footer = Paragraph::new("Enter = open  |  Ctrl+O = browse  |  Esc = quit")
            .style(Style::default().fg(theme.hud))
            .alignment(Alignment::Center);
        frame.render_widget(footer, Rect::new(area.x, area.y + h - 1, w, 1));
    }

    pub fn has_continue(&self) -> bool {
        self.entries.first().map_or(false, |e| e.is_continue)
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.entries.get(self.selected).map(|e| e.path.clone())
    }

    pub fn max_index(&self) -> usize {
        self.entries.len().saturating_sub(1)
    }

    /// Spawn zenity file picker, return chosen path if any.
    pub fn browse_file() -> Option<PathBuf> {
        let output = std::process::Command::new("zenity")
            .args(&[
                "--file-selection",
                "--title=Open Book",
            ])
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

    /// Add a path to the top of recent.txt.
    pub fn add_recent(path: &str) {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        let data_dir = format!("{}/.local/share/volta", home);
        let recent_path = format!("{}/recent.txt", data_dir);

        let _ = fs::create_dir_all(&data_dir);

        let mut lines: Vec<String> = fs::read_to_string(&recent_path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.is_empty() && l != &path)
            .map(|l| l.to_string())
            .collect();

        lines.insert(0, path.to_string());
        lines.truncate(20);

        let _ = fs::write(&recent_path, lines.join("\n") + "\n");
    }
}
