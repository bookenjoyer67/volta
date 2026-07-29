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
    Ghostty,
}

impl Term {
    /// Detect terminal capabilities from environment variables.
    /// Safe to call from the TUI event loop — reads env vars only.
    pub fn detect() -> Self {
        let kind = Self::detect_kind();
        Term {
            can_font_zoom: matches!(kind, TermKind::Kitty),
            can_images: matches!(kind, TermKind::Kitty | TermKind::Ghostty),
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
            if prog == ("WezTerm") {
                return TermKind::WezTerm;
            }
            if prog == ("ghostty") {
                return TermKind::Ghostty;
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

#[cfg(test)]
mod tests {
    use super::Term;

    #[test]
    fn detects_kitty_via_window_id() {
        unsafe { std::env::set_var("KITTY_WINDOW_ID", "1") };
        let term = Term::detect();
        assert!(term.can_font_zoom);
        assert!(term.can_images);
        unsafe { std::env::remove_var("KITTY_WINDOW_ID") };
    }

    #[test]
    fn detects_kitty_via_term() {
        unsafe {
            std::env::remove_var("KITTY_WINDOW_ID");
            std::env::remove_var("TERM_PROGRAM");
            std::env::remove_var("GNOME_TERMINAL_SERVICE");
            std::env::remove_var("VTE_VERSION");
            std::env::remove_var("XTERM_VERSION");
            std::env::set_var("TERM", "xterm-kitty") 
        }
        let term = Term::detect();
        assert!(term.can_font_zoom);
        unsafe { std::env::remove_var("TERM") };
    }

    #[test]
    fn detects_ghostty_via_term() {
        let vars_to_clear = [
            "KITTY_WINDOW_ID",
            "TERM_PROGRAM",
            "GNOME_TERMINAL_SERVICE",
            "VTE_VERSION",
            "XTERM_VERSION",
        ];
        for &var in &vars_to_clear{
            std::env::remove_var(var);
        }
        let term = Term::detect();
        assert!(term.can_images);
        assert!(!term.can_font_zoom); // as expected
        std::env::remove_var("TERM");
    }

    #[test]
    fn generic_terminal_has_no_capabilities() {
        unsafe {
            std::env::remove_var("KITTY_WINDOW_ID");
            std::env::set_var("TERM", "xterm-256color");
        }
        let term = Term::detect();
        assert!(!term.can_font_zoom);
        assert!(!term.can_images);
        unsafe { std::env::remove_var("TERM") };
    }

    #[test]
    fn unknown_terminal_is_generic() {
        unsafe {
            std::env::remove_var("KITTY_WINDOW_ID");
            std::env::remove_var("TERM");
            std::env::remove_var("TERM_PROGRAM");
            std::env::remove_var("GNOME_TERMINAL_SERVICE");
            std::env::remove_var("VTE_VERSION");
            std::env::remove_var("XTERM_VERSION");
        }
        let term = Term::detect();
        assert!(!term.can_font_zoom);
        assert!(!term.can_images);
    }
}
