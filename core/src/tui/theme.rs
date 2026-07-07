//! ANSI color themes for the TUI frontend.
//!
//! Mirrors the LÖVE themes in `frontend/themes/`. Currently ships
//! with the `neon` theme (black + hot pink — cyberpunk default).

use ratatui::style::Color;

/// A complete color palette for both reader and RSVP modes.
pub struct Theme {
    pub text: Color,
    pub cursor: Color,
    pub heading: Color,
    pub hud: Color,
    pub orp: Color,
    pub orp_fade: Color,
    pub progress: Color,
}

/// Neon — black background, cyan text, hot pink accents.
/// Matches the default LÖVE theme at `frontend/themes/neon.lua`.
pub const NEON: Theme = Theme {
    text: Color::Rgb(0, 229, 255),
    cursor: Color::Rgb(255, 20, 147),
    heading: Color::Rgb(0, 229, 255),
    hud: Color::Rgb(128, 128, 128),
    //Focus in the center of the word on RSVP mode
    orp: Color::Rgb(255, 20, 147),
    //The color of the word rendered in the RSVP reader
    orp_fade: Color::Rgb(225, 225, 225),
    progress: Color::Rgb(0, 229, 255),
};

//pub const OCEAN: Theme = Theme {
  //text: Color::Rgb(0, 229, 255),
    //cursor: Color::Rgb(255, 20, 147),
    //heading: Color::Rgb(0, 229, 255),
    //hud: Color::Rgb(128, 128, 128),
    //  Focus in the center of the word on RSVP mode
    //orp: Color::Rgb(255, 20, 147),
    //  The color of the word rendered in the RSVP reader
    //orp_fade: Color::Rgb(0, 64, 89),
    //progress: Color::Rgb(0, 229, 255),
//};
