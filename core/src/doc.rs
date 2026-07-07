//! Shared trait for every document format Volta supports.
//!
//! EpubDoc and PdfDoc both implement this trait, letting the
//! DocEnum dispatcher in lib.rs treat them uniformly without
//! caring about format-specific details.

use crate::types::Word;

/// Common read-only interface over EPUB and PDF documents.
///
/// All indices are zero-based.  `word_at` panics on out-of-bounds
/// (callers must guard with `word_count` first).
pub trait Document {
    /// Human-readable title (from metadata or filename fallback).
    fn title(&self) -> &str;

    /// Total number of extracted words across all chapters.
    fn word_count(&self) -> u32;

    /// Borrow the Word at index `i`.
    fn word_at(&self, i: u32) -> &Word;

    /// Number of chapters (or pages, for PDF).
    fn chapter_count(&self) -> u32;

    /// Chapter title string.
    fn chapter_title(&self, i: u32) -> &str;

    /// Full plain-text content of chapter `i`.
    fn chapter_text(&self, i: u32) -> &str;
}
