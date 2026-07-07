//! Core data types shared across the Volta engine.
//!
//! Everything flows through these three structs:
//!   Word    — one token with chapter context
//!   Chapter — title + full text for a book section
//!   BookMetadata — title and author

use std::ffi::CString;

/// A single token extracted from a document.
///
/// `chapter_index` is a zero-based index into the chapter list.
/// The FFI layer pre-allocates CStrings matching each Word so
/// the Lua side can read pointers without copying.
#[derive(Debug, Clone)]
pub struct Word {
    pub text: String,
    pub chapter_index: u32,
}

/// One section of a book — a chapter, a front-matter page, etc.
///
/// `text` is the full plain-text content after HTML stripping
/// and whitespace normalization.
#[derive(Debug, Clone)]
pub struct Chapter {
    pub title: String,
    pub text: String,
}

/// Minimal book-level metadata extracted at open time.
#[derive(Debug, Clone)]
pub struct BookMetadata {
    pub title: String,
    pub author: String,
}

impl Word {
    /// Construct a word tied to a specific chapter.
    pub fn new(text: String, chapter_index: u32) -> Self {
        Word { text, chapter_index }
    }

    /// Allocate a CString for FFI export.
    ///
    /// These are pre-built when the document is opened so that
    /// `rsvp_word_at` can return a stable `*const c_char` pointer
    /// into the CString's internal buffer.
    pub fn to_cstring(&self) -> CString {
        CString::new(self.text.as_str())
            .unwrap_or_else(|_| CString::new("").unwrap())
    }
}
