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
        self.wpm = wpm.clamp(50, 2000);
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

#[cfg(test)]
mod tests {
    use super::PlayerState;

    #[test]
    fn new_player_starts_paused_at_zero() {
        let p = PlayerState::new(100, 300);
        assert_eq!(p.current(), 0);
        assert!(!p.is_playing());
    }

    #[test]
    fn tick_does_nothing_when_paused() {
        let mut p = PlayerState::new(100, 300);
        let idx = p.tick(1000.0);
        assert_eq!(idx, 0);
    }

    #[test]
    fn tick_advances_at_300_wpm() {
        // 300 WPM = 200ms per word
        let mut p = PlayerState::new(100, 300);
        p.play();
        let idx = p.tick(200.0);
        assert_eq!(idx, 1);
    }

    #[test]
    fn tick_accumulates_partial_time() {
        // 300 WPM = 200ms per word
        let mut p = PlayerState::new(100, 300);
        p.play();
        assert_eq!(p.tick(100.0), 0); // not enough yet
        assert_eq!(p.tick(100.0), 1); // crosses threshold
    }

    #[test]
    fn tick_does_not_advance_past_end() {
        let mut p = PlayerState::new(3, 600); // 3 words, 100ms each
        p.play();
        p.tick(1000.0);
        assert_eq!(p.current(), 2); // last word, not past end
    }

    #[test]
    fn seek_jumps_to_position() {
        let mut p = PlayerState::new(100, 300);
        p.seek(42);
        assert_eq!(p.current(), 42);
    }

    #[test]
    fn seek_clamps_to_max() {
        let mut p = PlayerState::new(10, 300);
        p.seek(999);
        assert_eq!(p.current(), 9);
    }

    #[test]
    fn seek_resets_accumulator() {
        let mut p = PlayerState::new(100, 300);
        p.play();
        p.tick(100.0); // accumulate half a word
        p.seek(50);
        // fresh accumulator — half-word from before shouldn't count
        assert_eq!(p.tick(100.0), 50);
    }

    #[test]
    fn play_and_pause_toggle() {
        let mut p = PlayerState::new(100, 300);
        assert!(!p.is_playing());
        p.play();
        assert!(p.is_playing());
        p.pause();
        assert!(!p.is_playing());
    }

    #[test]
    fn wpm_clamped_to_range() {
        let mut p = PlayerState::new(100, 300);
        p.set_wpm(10);
        assert_eq!(p.wpm, 50);
        p.set_wpm(9999);
        assert_eq!(p.wpm, 2000);
        p.set_wpm(500);
        assert_eq!(p.wpm, 500);
    }

    #[test]
    fn empty_document_never_advances() {
        let mut p = PlayerState::new(0, 300);
        p.play();
        assert_eq!(p.tick(10000.0), 0);
    }

    #[test]
    fn fast_wpm_with_large_dt_does_not_overshoot() {
        // 2000 WPM = 30ms per word, 5 seconds of dt
        let mut p = PlayerState::new(50, 2000);
        p.play();
        let idx = p.tick(5000.0);
        assert_eq!(idx, 49);
    }

    #[test]
    fn tick_returns_u32() {
        let mut p = PlayerState::new(100, 300);
        p.play();
        let idx: u32 = p.tick(200.0);
        assert_eq!(idx, 1u32);
    }
}
