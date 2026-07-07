//! ANSI color themes for the TUI frontend.
//!
//! Mirrors the LÖVE themes in `frontend/themes/`. Themes are stored
//! in a static array and cycled with `THEMES` / `cycle_theme()`.

use ratatui::style::Color;

/// A complete color palette for both reader and RSVP modes.
pub struct Theme {
    pub name: &'static str,
    pub text: Color,
    pub cursor: Color,
    pub heading: Color,
    pub hud: Color,
    pub orp: Color,
    pub orp_fade: Color,
    pub progress: Color,
}

/// All 8 built-in themes, matching frontend/themes/.
pub static THEMES: &[Theme] = &[
    NEON, DAYLIGHT, SEPIA, NIGHT, DUSK, FOREST, OCEAN, AMBER,
];

/// Cycle to the next (dir=1) or previous (dir=-1) theme.
pub fn cycle_theme(current: usize, dir: i32) -> usize {
    let len = THEMES.len() as i32;
    let next = current as i32 + dir;
    if next < 0 {
        (len - 1) as usize
    } else if next >= len {
        0
    } else {
        next as usize
    }
}

// ── Theme definitions ──

/// Neon — black background, cyan text, hot pink accents. Cyberpunk default.
pub const NEON: Theme = Theme {
    name: "Neon",
    text: Color::Rgb(0, 229, 255),
    cursor: Color::Rgb(255, 20, 147),
    heading: Color::Rgb(0, 229, 255),
    hud: Color::Rgb(128, 128, 128),
    orp: Color::Rgb(255, 20, 147),
    orp_fade: Color::Rgb(225, 225, 225),
    progress: Color::Rgb(0, 229, 255),
};

/// Daylight — clean white with dark text.
pub const DAYLIGHT: Theme = Theme {
    name: "Daylight",
    text: Color::Rgb(26, 26, 26),
    cursor: Color::Rgb(0, 102, 204),
    heading: Color::Rgb(50, 50, 50),
    hud: Color::Rgb(100, 100, 100),
    orp: Color::Rgb(0, 102, 204),
    orp_fade: Color::Rgb(128, 128, 128),
    progress: Color::Rgb(0, 102, 204),
};

/// Sepia — warm vintage paper tones.
pub const SEPIA: Theme = Theme {
    name: "Sepia",
    text: Color::Rgb(61, 43, 31),
    cursor: Color::Rgb(153, 77, 26),
    heading: Color::Rgb(77, 51, 38),
    hud: Color::Rgb(128, 102, 77),
    orp: Color::Rgb(153, 77, 26),
    orp_fade: Color::Rgb(153, 128, 102),
    progress: Color::Rgb(153, 77, 26),
};

/// Night — dark muted text, soft contrast.
pub const NIGHT: Theme = Theme {
    name: "Night",
    text: Color::Rgb(200, 200, 200),
    cursor: Color::Rgb(102, 153, 255),
    heading: Color::Rgb(230, 230, 230),
    hud: Color::Rgb(128, 128, 128),
    orp: Color::Rgb(102, 153, 255),
    orp_fade: Color::Rgb(102, 102, 102),
    progress: Color::Rgb(102, 153, 255),
};

/// Dusk — deep blue-black with warm text.
pub const DUSK: Theme = Theme {
    name: "Dusk",
    text: Color::Rgb(217, 204, 179),
    cursor: Color::Rgb(255, 153, 77),
    heading: Color::Rgb(242, 230, 217),
    hud: Color::Rgb(140, 128, 115),
    orp: Color::Rgb(255, 153, 77),
    orp_fade: Color::Rgb(89, 89, 89),
    progress: Color::Rgb(255, 153, 77),
};

/// Forest — dark green tones, easy on eyes.
pub const FOREST: Theme = Theme {
    name: "Forest",
    text: Color::Rgb(191, 217, 179),
    cursor: Color::Rgb(102, 217, 102),
    heading: Color::Rgb(217, 242, 204),
    hud: Color::Rgb(128, 153, 115),
    orp: Color::Rgb(102, 217, 102),
    orp_fade: Color::Rgb(77, 102, 77),
    progress: Color::Rgb(102, 217, 102),
};

/// Ocean — cool deep blue tones.
pub const OCEAN: Theme = Theme {
    name: "Ocean",
    text: Color::Rgb(179, 204, 230),
    cursor: Color::Rgb(77, 179, 255),
    heading: Color::Rgb(204, 230, 255),
    hud: Color::Rgb(128, 153, 179),
    orp: Color::Rgb(77, 179, 255),
    orp_fade: Color::Rgb(77, 89, 102),
    progress: Color::Rgb(77, 179, 255),
};

/// Amber — classic amber monochrome terminal.
pub const AMBER: Theme = Theme {
    name: "Amber",
    text: Color::Rgb(255, 179, 0),
    cursor: Color::Rgb(255, 204, 51),
    heading: Color::Rgb(255, 204, 51),
    hud: Color::Rgb(153, 102, 0),
    orp: Color::Rgb(255, 204, 51),
    orp_fade: Color::Rgb(77, 51, 0),
    progress: Color::Rgb(255, 179, 0),
};
