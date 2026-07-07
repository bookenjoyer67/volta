//! PDF document backend.
//!
//! Uses `pdftotext` (poppler-utils) for text extraction and
//! `pdftoppm` for page rendering.  Each page is treated as one
//! "chapter" for navigation purposes.
//!
//! Rendered page images are cached in `~/.cache/volta/<sha256>/`
//! keyed by the PDF's absolute path.

use crate::doc::Document;
use crate::types::Word;
use sha2::{Sha256, Digest};
use std::ffi::CString;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Parsed PDF document with pre-extracted text.
///
/// All fields `pub` so the FFI dispatcher can reach CString vectors
/// directly — same pattern as EpubDoc.
pub struct PdfDoc {
    pub file_path: String,
    /// Total pages (as reported by counting form-feeds).
    pub page_count: u32,
    pub words: Vec<Word>,
    pub word_cstrings: Vec<CString>,
    pub chapter_titles: Vec<String>,
    pub chapter_title_cstrings: Vec<CString>,
    pub chapter_texts: Vec<String>,
    /// `~/.cache/volta/<sha256(file_path)>/` — stores rendered page PNGs.
    pub cache_dir: PathBuf,
}

impl PdfDoc {
    /// Open a PDF, extract all text via pdftotext, and tokenize.
    ///
    /// Requires `pdftotext` on PATH (poppler-utils package).
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        let file_path = path.to_string_lossy().to_string();

        // Count pages by probing pdftotext with increasing -f/-l flags.
        // pdftotext exits non-zero when asked for a page beyond the last.
        let page_count = Self::count_pages(&file_path)?;

        // Extract all text in one pass with -layout for positional fidelity.
        let output = Command::new("pdftotext")
            .args(&["-layout", &file_path, "-"])
            .output()
            .map_err(|e| format!("Failed to run pdftotext: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "pdftotext failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let full_text = String::from_utf8_lossy(&output.stdout).to_string();

        // pdftotext separates pages with form feed (U+000C).
        let pages: Vec<&str> = full_text.split('\u{000c}').collect();

        let mut words = Vec::new();
        let mut chapter_texts = Vec::new();

        for (page_idx, page_text) in pages.iter().enumerate() {
            let trimmed = page_text.trim();
            if trimmed.is_empty() {
                chapter_texts.push(String::new());
                continue;
            }

            // Tokenize this page's text into words
            let page_words: Vec<&str> = trimmed.split_whitespace().collect();
            for w in page_words {
                if !w.is_empty() {
                    words.push(Word::new(w.to_string(), page_idx as u32));
                }
            }

            chapter_texts.push(trimmed.to_string());
        }

        // Page labels: "Page 1", "Page 2", ...
        let chapter_titles: Vec<String> = (1..=pages.len())
            .map(|n| format!("Page {}", n))
            .collect();

        // Cache dir: ~/.cache/volta/<sha256 of absolute path>/
        let cache_dir = Self::cache_dir(&file_path)?;
        fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to create cache dir: {}", e))?;

        // Pre-build CStrings for FFI (same pattern as EpubDoc)
        let word_cstrings: Vec<CString> =
            words.iter().map(|w| w.to_cstring()).collect();
        let chapter_title_cstrings: Vec<CString> = chapter_titles
            .iter()
            .map(|t| CString::new(t.as_str()).unwrap_or_default())
            .collect();

        eprintln!(
            "PDF loaded: {} pages, {} words",
            chapter_texts.len(),
            words.len()
        );

        Ok(PdfDoc {
            file_path,
            page_count,
            words,
            word_cstrings,
            chapter_titles,
            chapter_title_cstrings,
            chapter_texts,
            cache_dir,
        })
    }

    /// Count pages by binary-searching pdftotext page range.
    ///
    /// Tries pages 1, 2, 3, ... until pdftotext fails, then returns
    /// the last successful page number.  Caps at 10,000.
    fn count_pages(file_path: &str) -> Result<u32, String> {
        for n in 1u32..10000 {
            let status = Command::new("pdftotext")
                .args(&[
                    "-f", &n.to_string(),
                    "-l", &n.to_string(),
                    file_path, "-",
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map_err(|e| format!("Failed to run pdftotext: {}", e))?;

            if !status.success() {
                return Ok(n - 1);
            }
        }
        Ok(10000) // safety cap
    }

    /// Deterministic cache path: `~/.cache/volta/<sha256>/`.
    fn cache_dir(file_path: &str) -> Result<PathBuf, String> {
        let mut hasher = Sha256::new();
        hasher.update(file_path.as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        let home =
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        Ok(PathBuf::from(format!("{}/.cache/volta/{}", home, hash)))
    }

    /// Render a single PDF page to PNG using pdftoppm.
    ///
    /// Results are cached on disk.  Returns the absolute path to
    /// the PNG file, or None if rendering fails.
    ///
    /// Requires `pdftoppm` on PATH (also from poppler-utils).
    pub fn render_page(&self, page: u32, dpi: u32) -> Option<String> {
        let page = page.max(1).min(self.page_count.max(1));
        let prefix = format!("page_{:04}", page);
        let expected_path =
            self.cache_dir.join(format!("{}.png", prefix));

        // Cache hit — skip re-render
        if expected_path.exists() {
            return Some(expected_path.to_string_lossy().to_string());
        }

        // -singlefile: output one PNG, not page-NN.png
        let output = Command::new("pdftoppm")
            .args(&[
                "-f", &page.to_string(),
                "-l", &page.to_string(),
                "-r", &dpi.to_string(),
                "-png",
                "-singlefile",
                &self.file_path,
            ])
            .arg(self.cache_dir.join(&prefix))
            .output()
            .ok()?;

        if output.status.success() && expected_path.exists() {
            Some(expected_path.to_string_lossy().to_string())
        } else {
            eprintln!(
                "pdftoppm failed for page {}: {}",
                page,
                String::from_utf8_lossy(&output.stderr)
            );
            None
        }
    }
}

impl Document for PdfDoc {
    fn title(&self) -> &str {
        // PDFs don't reliably have embedded titles; use the file path.
        &self.file_path
    }

    fn word_count(&self) -> u32 {
        self.words.len() as u32
    }

    fn word_at(&self, i: u32) -> &Word {
        &self.words[i as usize]
    }

    fn chapter_count(&self) -> u32 {
        self.chapter_texts.len() as u32
    }

    fn chapter_title(&self, i: u32) -> &str {
        &self.chapter_titles[i as usize]
    }

    fn chapter_text(&self, i: u32) -> &str {
        &self.chapter_texts[i as usize]
    }
}
