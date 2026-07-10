//! Volta core library — FFI bridge between Rust and Lua.
//!
//! ## Architecture
//!
//! This crate compiles to a C dynamic library (`libvolta_core.so`).
//! The Lua frontend loads it via LuaJIT FFI and calls the 20
//! `rsvp_*` functions declared below.
//!
//! Internally, the library uses a sum-type dispatcher (`DocEnum`)
//! that wraps either an EpubDoc or PdfDoc alongside a PlayerState.
//! All FFI functions null-check the opaque pointer, then delegate
//! through the `Document` trait or `PlayerState` methods.
//!
//! ## Safety
//!
//! The Lua side passes a raw `DocEnum*` obtained from `rsvp_open`.
//! This pointer is heap-allocated (`Box::into_raw`) and must be
//! freed by `rsvp_close`.  The CString vectors inside each doc
//! variant provide stable `*const c_char` pointers for the lifetime
//! of the DocEnum — no copying across the FFI boundary.

// Module visibility: epub & types are `pub` for the test binary;
// pdf stays private (only used through the DocEnum dispatcher).
pub mod cover;
pub mod doc;
pub mod epub;
pub mod library;
pub mod md;
pub mod pdf;
pub mod player;
pub mod types;

use doc::Document;
use epub::EpubDoc;
use md::MdDoc;
use pdf::PdfDoc;
use player::PlayerState;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;

// ────────────────────────────────────────────────────────────────
// DocEnum — format dispatcher
// ────────────────────────────────────────────────────────────────

/// A tagged union over EPUB and PDF, each paired with its own
/// PlayerState.  This lets the FFI surface be format-agnostic:
/// every `rsvp_*` function just matches on the variant and calls
/// through the `Document` trait or `PlayerState`.
pub enum DocEnum {
    Epub(EpubDoc, PlayerState),
    Md(MdDoc, PlayerState),
    Pdf(PdfDoc, PlayerState),
}

impl DocEnum {
    /// Borrow the document trait object for read-only access.
    pub fn doc(&self) -> &dyn Document {
        match self {
            DocEnum::Epub(d, _) => d,
            DocEnum::Md(d, _) => d,
            DocEnum::Pdf(d, _) => d,
        }
    }

    /// Borrow the player (read-only).
    pub fn player(&self) -> &PlayerState {
        match self {
            DocEnum::Epub(_, p) => p,
            DocEnum::Md(_, p) => p,
            DocEnum::Pdf(_, p) => p,
        }
    }

    /// Borrow the player mutably (for seek/play/pause/tick).
    pub fn player_mut(&mut self) -> &mut PlayerState {
        match self {
            DocEnum::Epub(_, p) => p,
            DocEnum::Md(_, p) => p,
            DocEnum::Pdf(_, p) => p,
        }
    }

    /// Return a stable C pointer to word `i`.
    ///
    /// The pointer lives inside the pre-allocated CString vectors
    /// on EpubDoc/PdfDoc and is valid until rsvp_close.
    fn word_cstring_ptr(&self, i: u32) -> *const c_char {
        match self {
            DocEnum::Epub(d, _) => d.word_cstrings[i as usize].as_ptr(),
            DocEnum::Md(d, _) => d.word_cstrings[i as usize].as_ptr(),
            DocEnum::Pdf(d, _) => d.word_cstrings[i as usize].as_ptr(),
        }
    }

    /// Stable C pointer to chapter title `i`.
    fn chapter_title_cstring_ptr(&self, i: u32) -> *const c_char {
        match self {
            DocEnum::Epub(d, _) => d.chapter_title_cstrings[i as usize].as_ptr(),
            DocEnum::Md(d, _) => d.chapter_title_cstrings[i as usize].as_ptr(),
            DocEnum::Pdf(d, _) => d.chapter_title_cstrings[i as usize].as_ptr(),
        }
    }

    /// Stable C pointer to chapter text `i`.
    ///
    /// Note: PdfDoc does not pre-build chapter_text_cstrings — it
    /// allocates one on-demand here.  This leaks the CString, but
    /// the pointer lives for the DocEnum lifetime (freed on close).
    fn chapter_text_cstring_ptr(&self, i: u32) -> *const c_char {
        match self {
            DocEnum::Epub(d, _) => d.chapter_text_cstrings[i as usize].as_ptr(),
            DocEnum::Md(d, _) => d.chapter_text_cstrings[i as usize].as_ptr(),
            DocEnum::Pdf(d, _) => {
                // Build on demand — acceptable because this is called
                // infrequently (once per chapter switch, not per frame).
                CString::new(d.chapter_text(i).as_bytes())
                    .unwrap_or_default()
                    .into_raw()
            }
        }
    }

    /// Build a CString for the document title.
    fn title_cstring(&self) -> CString {
        CString::new(self.doc().title().as_bytes())
            .unwrap_or_default()
    }

    /// Find the first word index belonging to chapter `chapter`.
    ///
    /// Linear scan — O(n) in word count.  For a 100K-word book this
    /// is ~100µs.  Called once when entering RSVP mode, not per frame.
    pub fn chapter_start(&self, chapter: u32) -> u32 {
        let count = self.doc().word_count();
        for i in 0..count {
            if self.doc().word_at(i).chapter_index == chapter {
                return i;
            }
        }
        count // fallback: past-the-end
    }
}

// ────────────────────────────────────────────────────────────────
// FFI exports — called from Lua via LuaJIT FFI
// ────────────────────────────────────────────────────────────────

/// Open an EPUB or PDF file.
///
/// Returns a heap-allocated `DocEnum*` (or NULL on failure).
/// The returned pointer is opaque to Lua; it must be freed with
/// `rsvp_close`.
///
/// The initial PlayerState is created with WPM=300, paused.
///
/// Helper: add a book to the library and extract cover.
fn add_to_library(doc: &dyn Document, path: &Path, format: &str) {
    use crate::library::{Library, LibraryEntry};
    let mut library = Library::load();
    let path_s = path.to_string_lossy();
    let cover_path = crate::cover::extract_cover(&path_s, format);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    library.upsert(
        &path_s,
        LibraryEntry {
            title: doc.title().to_string(),
            author: String::new(),
            format: format.to_string(),
            chapter_count: doc.chapter_count(),
            current_chapter: 0,
            current_word: 0,
            last_opened: now,
            added: now,
            cover_path,
        },
    );
    library.save();
}

#[no_mangle]
pub extern "C" fn rsvp_open(path: *const c_char) -> *mut DocEnum {
    // Convert C string → Rust &str, return NULL on invalid UTF-8.
    let path_str = unsafe {
        match CStr::from_ptr(path).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return std::ptr::null_mut(),
        }
    };

    let path = Path::new(&path_str);

    // Dispatch on file extension (case-insensitive).
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Note: `?` cannot be used inside `extern "C" fn` because the
    // return type is a raw pointer, not Result.  We match explicitly.
    let result: Result<DocEnum, String> = match ext.as_str() {
        "epub" => match EpubDoc::open(path) {
            Ok(doc) => {
                let total = doc.word_count() as usize;
                add_to_library(&doc, path, "epub");
                Ok(DocEnum::Epub(doc, PlayerState::new(total, 300)))
            }
            Err(e) => Err(format!("Failed to open EPUB: {}", e)),
        },
        "pdf" => match PdfDoc::open(path) {
            Ok(doc) => {
                let total = doc.word_count() as usize;
                add_to_library(&doc, path, "pdf");
                Ok(DocEnum::Pdf(doc, PlayerState::new(total, 300)))
            }
            Err(e) => Err(format!("Failed to open PDF: {}", e)),
        },
        "md" => match MdDoc::open(path) {
            Ok(doc) => {
                let total = doc.word_count() as usize;
                add_to_library(&doc, path, "md");
                Ok(DocEnum::Md(doc, PlayerState::new(total, 300)))
            }
            Err(e) => Err(format!("Failed to open MD: {}", e)),
        },
        _ => Err(format!("Unsupported format: .{}", ext)),
    };

    match result {
        Ok(doc) => Box::into_raw(Box::new(doc)),
        Err(e) => {
            eprintln!("rsvp_open error: {}", e);
            std::ptr::null_mut()
        }
    }
}

/// Free a document and its associated PlayerState.
///
/// After this call the pointer is invalid.  All previously returned
/// `*const c_char` pointers (word/text/title) are also invalid.
#[no_mangle]
pub extern "C" fn rsvp_close(doc: *mut DocEnum) {
    if doc.is_null() {
        return;
    }
    unsafe {
        let _ = Box::from_raw(doc); // Drop runs EpubDoc/PdfDoc destructors
    }
}

/// Document title.  Caller does NOT own the returned pointer;
/// it is valid until `rsvp_close`.
#[no_mangle]
pub extern "C" fn rsvp_title(doc: *mut DocEnum) -> *const c_char {
    if doc.is_null() {
        return std::ptr::null();
    }
    let doc = unsafe { &*doc };
    doc.title_cstring().into_raw()
}

/// Total extracted words.
#[no_mangle]
pub extern "C" fn rsvp_word_count(doc: *mut DocEnum) -> u32 {
    if doc.is_null() {
        return 0;
    }
    let doc = unsafe { &*doc };
    doc.doc().word_count()
}

/// Word at index `i`.  Returns NULL if `i` is out of bounds.
/// The returned pointer is valid until `rsvp_close`.
#[no_mangle]
pub extern "C" fn rsvp_word_at(
    doc: *mut DocEnum,
    i: u32,
) -> *const c_char {
    if doc.is_null() {
        return std::ptr::null();
    }
    let doc = unsafe { &*doc };
    let count = doc.doc().word_count();
    if count == 0 || i >= count {
        return std::ptr::null();
    }
    doc.word_cstring_ptr(i)
}

/// Chapter index for word `i` (0-based).
#[no_mangle]
pub extern "C" fn rsvp_chapter_at(doc: *mut DocEnum, i: u32) -> u32 {
    if doc.is_null() {
        return 0;
    }
    let doc = unsafe { &*doc };
    let count = doc.doc().word_count();
    if count == 0 || i >= count {
        return 0;
    }
    doc.doc().word_at(i).chapter_index
}

/// Total number of chapters.
#[no_mangle]
pub extern "C" fn rsvp_chapter_count(doc: *mut DocEnum) -> u32 {
    if doc.is_null() {
        return 0;
    }
    let doc = unsafe { &*doc };
    doc.doc().chapter_count()
}

/// Chapter title for chapter `i` (0-based).  Returns NULL if
/// `i` is out of bounds.
#[no_mangle]
pub extern "C" fn rsvp_chapter_title(
    doc: *mut DocEnum,
    i: u32,
) -> *const c_char {
    if doc.is_null() {
        return std::ptr::null();
    }
    let doc = unsafe { &*doc };
    if i >= doc.doc().chapter_count() {
        return std::ptr::null();
    }
    doc.chapter_title_cstring_ptr(i)
}

/// Full plain-text content of chapter `i`.  Returns NULL if
/// `i` is out of bounds.
#[no_mangle]
pub extern "C" fn rsvp_chapter_text(
    doc: *mut DocEnum,
    i: u32,
) -> *const c_char {
    if doc.is_null() {
        return std::ptr::null();
    }
    let doc = unsafe { &*doc };
    if i >= doc.doc().chapter_count() {
        return std::ptr::null();
    }
    doc.chapter_text_cstring_ptr(i)
}

/// Render a PDF page to PNG (EPUB returns NULL).
///
/// Result is a C string containing the absolute path to the
/// cached PNG file, or NULL if rendering failed.
#[no_mangle]
pub extern "C" fn rsvp_render_page(
    doc: *mut DocEnum,
    page: u32,
    dpi: u32,
) -> *const c_char {
    if doc.is_null() {
        return std::ptr::null();
    }
    let doc = unsafe { &mut *doc };
    match doc {
        DocEnum::Pdf(pdf_doc, _) => {
            match pdf_doc.render_page(page, dpi) {
                Some(path) => {
                    CString::new(path).unwrap_or_default().into_raw()
                }
                None => std::ptr::null(),
            }
        }
        DocEnum::Md(_, _) => std::ptr::null(),
        DocEnum::Epub(_, _) => std::ptr::null(),
    }
}

// ────────────────────────────────────────────────────────────────
// Player controls — all delegate to PlayerState methods
// ────────────────────────────────────────────────────────────────

/// Jump to word index `i` (clamped to [0, word_count-1]).
#[no_mangle]
pub extern "C" fn rsvp_seek(doc: *mut DocEnum, i: u32) {
    if doc.is_null() {
        return;
    }
    let doc = unsafe { &mut *doc };
    doc.player_mut().seek(i);
}

/// Set reading speed in words per minute (clamped to 50–2000).
#[no_mangle]
pub extern "C" fn rsvp_set_wpm(doc: *mut DocEnum, wpm: u32) {
    if doc.is_null() {
        return;
    }
    let doc = unsafe { &mut *doc };
    doc.player_mut().set_wpm(wpm);
}

/// Start or resume RSVP playback.
#[no_mangle]
pub extern "C" fn rsvp_play(doc: *mut DocEnum) {
    if doc.is_null() {
        return;
    }
    let doc = unsafe { &mut *doc };
    doc.player_mut().play();
}

/// Pause RSVP playback (position preserved).
#[no_mangle]
pub extern "C" fn rsvp_pause(doc: *mut DocEnum) {
    if doc.is_null() {
        return;
    }
    let doc = unsafe { &mut *doc };
    doc.player_mut().pause();
}

/// Is the player currently advancing?
#[no_mangle]
pub extern "C" fn rsvp_is_playing(doc: *mut DocEnum) -> bool {
    if doc.is_null() {
        return false;
    }
    let doc = unsafe { &*doc };
    doc.player().is_playing()
}

/// Current word index (0-based).
#[no_mangle]
pub extern "C" fn rsvp_current_index(doc: *mut DocEnum) -> u32 {
    if doc.is_null() {
        return 0;
    }
    let doc = unsafe { &*doc };
    doc.player().current()
}

/// Advance the player by `dt_ms` milliseconds.
///
/// Called every frame from `love.update(dt)` when RSVP mode
/// is active.  Returns the new word index.
#[no_mangle]
pub extern "C" fn rsvp_tick(
    doc: *mut DocEnum,
    dt_ms: f64,
) -> u32 {
    if doc.is_null() {
        return 0;
    }
    let doc = unsafe { &mut *doc };
    doc.player_mut().tick(dt_ms)
}

/// Find the first word index belonging to a chapter.
///
/// Used when entering RSVP mode from reader mode — seeks the
/// player to the start of the chapter the user was reading.
#[no_mangle]
pub extern "C" fn rsvp_chapter_start(
    doc: *const DocEnum,
    chapter: u32,
) -> u32 {
    if doc.is_null() {
        return 0;
    }
    let doc = unsafe { &*doc };
    doc.chapter_start(chapter)
}
