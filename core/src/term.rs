//! Terminal capability detection.
//!
//! Detects the terminal type once at startup and exposes boolean
//! capabilities so feature code can silently fall back when the
//! terminal doesn't support a feature.

/// Terminal capabilities detected at startup.
pub struct Term {
    /// Terminal family.
    pub kind: TermKind,
    /// Can change font size programmatically (kitty @ set-font-size).
    pub can_font_zoom: bool,
    /// Can display inline images (kitty graphics protocol).
    pub can_images: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermKind {
    Kitty,
    GnomeTerminal,
    Alacritty,
    WezTerm,
    Foot,
    Xterm,
    Generic,
}

impl Term {
    /// Detect terminal capabilities from environment variables.
    /// Safe to call from the TUI event loop — reads env vars only.
    pub fn detect() -> Self {
        let kind = Self::detect_kind();
        Term {
            can_font_zoom: matches!(kind, TermKind::Kitty),
            can_images: matches!(kind, TermKind::Kitty),
            kind,
        }
    }

    fn detect_kind() -> TermKind {
        // Kitty: sets KITTY_WINDOW_ID on every window
        if std::env::var("KITTY_WINDOW_ID").is_ok() {
            return TermKind::Kitty;
        }
        // TERM fallback: some setups don't export KITTY_WINDOW_ID
        // but set TERM=kitty or TERM=xterm-kitty
        if let Ok(term) = std::env::var("TERM") {
            if term.contains("kitty") {
                return TermKind::Kitty;
            }
            if term.contains("alacritty") {
                return TermKind::Alacritty;
            }
            if term.contains("foot") {
                return TermKind::Foot;
            }
        }
        // WezTerm sets TERM_PROGRAM
        if let Ok(prog) = std::env::var("TERM_PROGRAM") {
            if prog.contains("WezTerm") {
                return TermKind::WezTerm;
            }
        }
        // GNOME Terminal / VTE-based terminals
        if std::env::var("GNOME_TERMINAL_SERVICE").is_ok() {
            return TermKind::GnomeTerminal;
        }
        if std::env::var("VTE_VERSION").is_ok() {
            return TermKind::GnomeTerminal;
        }
        // xterm
        if std::env::var("XTERM_VERSION").is_ok() {
            return TermKind::Xterm;
        }
        TermKind::Generic
    }
}
