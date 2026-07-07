//! RSVP player state machine.
//!
//! The player owns the current position, WPM setting, and
//! play/pause flag.  The Lua frontend feeds it frame delta-times
//! and the player advances through words at the configured speed.
//!
//! Design: the player does NOT own a document reference.  It only
//! knows the total word count and its own position.  The FFI layer
//! in lib.rs ties a PlayerState to each DocEnum variant.

/// Tracks RSVP playback position and speed.
///
/// `accumulator` collects fractional milliseconds between frames.
/// When it exceeds `ms_per_word` (derived from WPM), the player
/// advances one word and subtracts one word's worth of time.
pub struct PlayerState {
    /// Current word index (0-based, always < total_words).
    pub current_index: usize,
    /// Words per minute (clamped to 50–2000).
    pub wpm: u32,
    /// Whether playback is active.
    pub is_playing: bool,

    /// Fractional ms accumulated since last word advance.
    accumulator: f64,
    /// Total words in the document (fixed at open time).
    total_words: usize,
}

impl PlayerState {
    /// Create a fresh player at word 0, paused, with the given WPM.
    pub fn new(total_words: usize, wpm: u32) -> Self {
        PlayerState {
            current_index: 0,
            wpm,
            is_playing: false,
            accumulator: 0.0,
            total_words,
        }
    }

    /// Advance the player by `dt_ms` milliseconds.
    ///
    /// Returns the new word index.  If paused or at end-of-document,
    /// returns the current index immediately.
    ///
    /// The while-loop handles large delta-times (e.g. app resumed
    /// after suspend) without skipping words — every word advance
    /// consumes exactly `ms_per_word` from the accumulator.
    pub fn tick(&mut self, dt_ms: f64) -> u32 {
        if !self.is_playing || self.total_words == 0 {
            return self.current_index as u32;
        }

        // 60000 ms/minute ÷ WPM = ms per word
        let ms_per_word = 60000.0 / self.wpm as f64;
        self.accumulator += dt_ms;

        while self.accumulator >= ms_per_word
            && self.current_index + 1 < self.total_words
        {
            self.current_index += 1;
            self.accumulator -= ms_per_word;
        }

        // Clamp when extremely fast WPM + large dt would overshoot
        if self.current_index >= self.total_words {
            self.current_index = self.total_words.saturating_sub(1);
            self.accumulator = 0.0;
        }

        self.current_index as u32
    }

    /// Jump to a specific word index.  Resets the time accumulator.
    pub fn seek(&mut self, i: u32) {
        let max = self.total_words.saturating_sub(1);
        self.current_index = (i as usize).min(max);
        self.accumulator = 0.0;
    }

    /// Change reading speed.  Clamped to [50, 2000] WPM.
    pub fn set_wpm(&mut self, wpm: u32) {
        self.wpm = wpm.max(50).min(2000);
    }

    /// Resume playback.
    pub fn play(&mut self) {
        self.is_playing = true;
    }

    /// Pause playback (position preserved).
    pub fn pause(&mut self) {
        self.is_playing = false;
    }

    /// Current word index (0-based).
    pub fn current(&self) -> u32 {
        self.current_index as u32
    }

    /// Whether the player is currently advancing on ticks.
    pub fn is_playing(&self) -> bool {
        self.is_playing
    }
}
