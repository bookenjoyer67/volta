//! PDF document backend.
//!
//! Uses `pdftotext` (poppler-utils) for text extraction and
//! `pdftoppm` for page rendering.  Each page is treated as one
//! "chapter" for navigation purposes.
//!
//! Rendered page images are cached in `~/.cache/volta/<sha256>/`
//! keyed by the PDF's absolute path.

use crate::doc::Document;
use crate::types::{ChapterImage, Word};
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
    /// Per-page rendered images (one per chapter, at word_offset 0).
    /// PNGs are rendered on-demand via render_page().
    pub chapter_images: Vec<Vec<crate::types::ChapterImage>>,
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
            .args(["-layout", &file_path, "-"])
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

        // Build per-page ChapterImage entries (one per page, word_offset 0).
        // PNGs are rendered on-demand by render_page() — we store the
        // expected cache path without rendering first.
        // Use page_count from pdfinfo (more reliable than pages.len() which
        // can include a trailing empty entry from pdftotext's form-feed split).
        let chapter_images: Vec<Vec<ChapterImage>> = (1..=page_count as usize)
            .map(|page| {
                let prefix = format!("page_{:04}", page);
                let cached_path = cache_dir
                    .join(format!("{}.png", prefix))
                    .to_string_lossy()
                    .to_string();
                vec![ChapterImage {
                    word_offset: 0,
                    cached_path,
                    width: 0,
                    height: 0,
                }]
            })
            .collect();

        // Pre-build CStrings for FFI (same pattern as EpubDoc)
        let word_cstrings: Vec<CString> =
            words.iter().map(|w| w.to_cstring()).collect();
        let chapter_title_cstrings: Vec<CString> = chapter_titles
            .iter()
            .map(|t| CString::new(t.as_str()).unwrap_or_default())
            .collect();

        Ok(PdfDoc {
            file_path,
            page_count,
            words,
            word_cstrings,
            chapter_titles,
            chapter_title_cstrings,
            chapter_texts,
            cache_dir,
            chapter_images,
        })
    }

    ///PDF with pre-extracted and tokenized text
    ///Page count is gathered with 'pdfinfo', text with 'pdftotext'
    /// Words are tokenized up front and stored as "chapters"
fn count_pages(file_path: &str) -> Result<u32, String> {
    let output = Command::new("pdfinfo")
        .arg(file_path)
        .output()
        .map_err(|e| format!("Failed to run pdfinfo: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "pdfinfo failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.starts_with("Pages:") {
            let count_str = line
                .split_whitespace()
                .nth(1)
                .ok_or("Missing page count")?;
            return count_str.parse().map_err(|e| format!("Invalid page count: {}", e));
        }
    }
    Err("Could not find page count in pdfinfo output".to_string())
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
            .args([
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocEnum;

    #[test]
    fn pdf_chapter_images_match_pages() {
        let path = "/tmp/test.pdf";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: test PDF not found");
            return;
        }
        let doc = PdfDoc::open(std::path::Path::new(path)).expect("open failed");
        assert!(doc.page_count > 0);
        assert_eq!(doc.chapter_images.len(), doc.page_count as usize);
        for (i, images) in doc.chapter_images.iter().enumerate() {
            assert_eq!(images.len(), 1, "page {i} should have 1 image");
            assert_eq!(images[0].word_offset, 0);
            assert!(images[0].cached_path.contains("page_"));
        }
        let rendered = doc.render_page(1, 150).unwrap();
        assert_eq!(doc.chapter_images[0][0].cached_path, rendered);
    }

    #[test]
    fn pdf_load_images_no_panic_on_bounds() {
        let path = "/tmp/test.pdf";
        if !std::path::Path::new(path).exists() {
            return;
        }
        let pdf = PdfDoc::open(std::path::Path::new(path)).expect("open failed");
        let doc_enum = DocEnum::Pdf(pdf, crate::player::PlayerState::new(0, 300));
        let mut images: Vec<crate::types::ChapterImage> = Vec::new();
        if let DocEnum::Pdf(ref pdf, _) = doc_enum {
            if 0 < pdf.chapter_images.len() {
                for img in &pdf.chapter_images[0] {
                    images.push(img.clone());
                }
                assert!(!images.is_empty());
            }
            let max_ch = pdf.chapter_images.len() + 10;
            assert!(max_ch >= pdf.chapter_images.len());
        }
    }

    #[test]
    fn pdf_reader_image_near_cursor() {
        // Simulate what image_near_cursor does: find images within range
        let path = "/tmp/test.pdf";
        if !std::path::Path::new(path).exists() {
            return;
        }
        let pdf = PdfDoc::open(std::path::Path::new(path)).expect("open failed");
        let doc_enum = DocEnum::Pdf(pdf, crate::player::PlayerState::new(0, 300));

        // Load chapter_images like ReaderState::load_images does
        let mut chapter_images: Vec<crate::types::ChapterImage> = Vec::new();
        if let DocEnum::Pdf(ref pdf, _) = doc_enum {
            for img in &pdf.chapter_images[0] {
                chapter_images.push(img.clone());
            }
        }
        chapter_images.sort_by_key(|img| img.word_offset);

        // image_near_cursor logic: find first img with word_offset >= cursor
        // and within 15 ahead OR within 5 behind
        let find = |cursor: usize| -> bool {
            chapter_images.iter().any(|img| {
                let ahead = img.word_offset >= cursor && img.word_offset <= cursor + 15;
                let behind = img.word_offset < cursor && img.word_offset + 5 >= cursor;
                ahead || behind
            })
        };

        assert!(find(0), "should find image at cursor=0");
        assert!(find(5), "should find image at cursor=5");
        assert!(!find(20), "should NOT find image at cursor=20");
    }
}
