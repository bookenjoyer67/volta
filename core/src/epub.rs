//! EPUB document backend.
//!
//! Uses the `rbook` crate's EpubReader API for sequential chapter
//! extraction.  Every word is assigned a chapter_index so the FFI
//! layer can answer "which chapter does word N belong to?" in O(1).
//!
//! HTML tags are stripped with a simple state machine (no full XML
//! parser — good enough for 99% of real-world EPUB content).

use crate::doc::Document;
use crate::types::{Chapter, Word, BookMetadata};
use std::ffi::CString;
use std::path::Path;

/// Parsed EPUB document.
///
/// All fields are `pub` so the FFI dispatcher can reach into the
/// pre-allocated CString vectors for pointer-stable word access.
pub struct EpubDoc {
    pub metadata: BookMetadata,
    pub words: Vec<Word>,
    /// Pre-built CStrings matching `words` — indices correspond 1:1.
    pub word_cstrings: Vec<CString>,
    pub chapters: Vec<Chapter>,
    /// Pre-built CStrings for chapter titles (per-chapter).
    pub chapter_title_cstrings: Vec<CString>,
    /// Pre-built CStrings for full chapter text (per-chapter).
    pub chapter_text_cstrings: Vec<CString>,
}

impl EpubDoc {
    /// Decode common HTML entities in a string.
    ///
    /// Handles named entities (&amp;, &lt;, &gt;, &quot;, &apos;, &nbsp;,
    /// &mdash;, &ndash;, &ldquo;, &rdquo;, &lsquo;, &rsquo;, &hellip;)
    /// and numeric entities (&#8211; → –).
    ///
    /// Runs after tag stripping, before whitespace collapsing.
    fn decode_entities(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        let bytes = text.as_bytes();

        while i < bytes.len() {
            if bytes[i] == b'&' {
                // Find the closing semicolon
                if let Some(end) = text[i..].find(';') {
                    let entity = &text[i + 1..i + end]; // skip '&' and ';'

                    let replacement: Option<&str> = match entity {
                        "amp" => Some("&"),
                        "lt" => Some("<"),
                        "gt" => Some(">"),
                        "quot" => Some("\""),
                        "apos" => Some("'"),
                        "nbsp" => Some(" "),
                        "mdash" => Some("\u{2014}"),  // —
                        "ndash" => Some("\u{2013}"),  // –
                        "ldquo" => Some("\u{201c}"),  // "
                        "rdquo" => Some("\u{201d}"),  // "
                        "lsquo" => Some("\u{2018}"),  // '
                        "rsquo" => Some("\u{2019}"),  // '
                        "hellip" => Some("\u{2026}"), // …
                        _ => None,
                    };

                    match replacement {
                        Some(s) => {
                            out.push_str(s);
                            i += end + 1; // skip past ';'
                            continue;
                        }
                        None => {
                            // Try numeric entity: &#NNNN;
                            if entity.starts_with('#') {
                                let num_str = &entity[1..]; // skip '#'
                                if let Ok(n) = num_str.parse::<u32>() {
                                    if let Some(c) = char::from_u32(n) {
                                        out.push(c);
                                        i += end + 1;
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Not an entity — copy character as-is
            out.push(text[i..].chars().next().unwrap());
            i += text[i..].chars().next().unwrap().len_utf8();
        }

        out
    }

    /// Open and fully ingest an EPUB file.
    ///
    /// This reads every spine entry sequentially via `rbook::EpubReader`,
    /// strips HTML, tokenizes into words, and pre-allocates CStrings.
    /// For a typical novel (~100K words) this takes < 100ms.
    pub fn open(path: &Path) -> Result<Self, String> {
        let epub =
            rbook::Epub::open(path).map_err(|e| format!("Failed to open EPUB: {}", e))?;

        // --- metadata ---
        let title = epub
            .metadata()
            .title()
            .map(|t| t.to_string())
            .unwrap_or_else(|| "Unknown Title".to_string());

        // rbook returns creators as an opaque iterator; grab the first.
        let author = epub
            .metadata()
            .creators()
            .next()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "Unknown Author".to_string());

        let metadata = BookMetadata { title, author };

        // --- content extraction ---
        // EpubReader walks the spine in reading order, yielding one
        // EpubReaderContent per spine entry.
        let mut reader = epub.reader();
        let mut words = Vec::new();
        let mut chapters = Vec::new();
        let mut ci: u32 = 0;

        while let Some(content_result) = reader.read_next() {
            let content = content_result
                .map_err(|e| format!("Failed to read chapter: {}", e))?;
            let raw_text = content.content();       // &str — the XHTML body
            let spine_entry = content.spine_entry(); // metadata for this entry

            // Chapter titles come from the spine idref (internal EPUB ID).
            // Most EPUBs use human-readable IDs like "chapter-1"; fall back
            // to a numbered label otherwise.
            let chapter_title = {
                let idref = spine_entry.idref();
                if idref.is_empty() {
                    format!("Chapter {}", ci + 1)
                } else {
                    idref.to_string()
                }
            };

            // --- HTML stripping ---
            // Simple character-level state machine: everything between
            // '<' and '>' is discarded.  Does NOT handle CDATA, comments,
            // or script/style blocks — assume EPUB content is clean XHTML.
            let mut clean_text = String::new();
            let mut in_tag = false;
            for ch in raw_text.chars() {
                match ch {
                    '<' => in_tag = true,
                    '>' => in_tag = false,
                    _ if !in_tag => {
                        clean_text.push(ch);
                    }
                    _ => {}
                }
            }

            // --- entity decoding ---
            // EPUBs encode special characters as HTML entities.
            // &#8211; → – (en dash), &mdash; → —, &ldquo; → ", etc.
            let clean_text = Self::decode_entities(&clean_text);

            // Collapse runs of whitespace into single spaces.
            let clean_text = clean_text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");

            // --- word tokenization ---
            // Each word is tagged with its chapter index so
            // rsvp_chapter_at can answer in O(1) without scanning.
            for word_str in clean_text.split_whitespace() {
                if !word_str.is_empty() {
                    words.push(Word::new(word_str.to_string(), ci));
                }
            }

            chapters.push(Chapter {
                title: chapter_title,
                text: clean_text,
            });

            ci += 1;
            // Progress log every 10 chapters for large books
            if ci % 10 == 0 {
                eprintln!("  Chapter {} ({} words)", ci, words.len());
            }
        }

        // --- pre-build CStrings for FFI ---
        // The Lua frontend receives `*const c_char` pointers into these
        // vectors.  As long as the EpubDoc lives, the pointers are valid.
        let word_cstrings: Vec<CString> =
            words.iter().map(|w| w.to_cstring()).collect();
        let chapter_title_cstrings: Vec<CString> = chapters
            .iter()
            .map(|c| CString::new(c.title.as_str()).unwrap_or_default())
            .collect();
        let chapter_text_cstrings: Vec<CString> = chapters
            .iter()
            .map(|c| CString::new(c.text.as_str()).unwrap_or_default())
            .collect();

        eprintln!(
            "EPUB loaded: {} words in {} chapters",
            words.len(),
            chapters.len()
        );

        Ok(EpubDoc {
            metadata,
            words,
            word_cstrings,
            chapters,
            chapter_title_cstrings,
            chapter_text_cstrings,
        })
    }
}

impl Document for EpubDoc {
    fn title(&self) -> &str {
        &self.metadata.title
    }

    fn word_count(&self) -> u32 {
        self.words.len() as u32
    }

    fn word_at(&self, i: u32) -> &Word {
        &self.words[i as usize]
    }

    fn chapter_count(&self) -> u32 {
        self.chapters.len() as u32
    }

    fn chapter_title(&self, i: u32) -> &str {
        &self.chapters[i as usize].title
    }

    fn chapter_text(&self, i: u32) -> &str {
        &self.chapters[i as usize].text
    }
}
