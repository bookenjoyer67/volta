//! Markdown document backend.
//!
//! Reads .md files as plain text. Splits on `##` or `#` headings
//! into chapters so long docs are navigable. Headings become chapter
//! titles. Text between headings is the chapter body.

use crate::doc::Document;
use crate::types::{ChapterImage, Word};
use sha2::{Digest, Sha256};
use std::ffi::CString;
use std::fs;
use std::path::Path;

pub struct MdDoc {
    pub file_path: String,
    pub words: Vec<Word>,
    pub word_cstrings: Vec<CString>,
    pub chapter_titles: Vec<String>,
    pub chapter_title_cstrings: Vec<CString>,
    pub chapter_texts: Vec<String>,
    pub chapter_text_cstrings: Vec<CString>,
    /// Per-chapter inline images (C FFI representation).
    pub chapter_image_c: Vec<Vec<crate::ChapterImageC>>,
    pub chapter_image_path_cstrings: Vec<Vec<CString>>,
    /// Per-chapter inline images (Rust representation for TUI use).
    pub chapter_images: Vec<Vec<crate::types::ChapterImage>>,
}

impl MdDoc {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file_path = path.to_string_lossy().to_string();
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        // Split on markdown headings (## or # at start of line).
        // Preserve the heading line as the chapter title.
        let mut chapters: Vec<(String, String)> = Vec::new(); // (title, text)
        let mut current_title = String::new();
        let mut current_text = String::new();
        let mut first = true;

        for line in raw.lines() {
            let trimmed = line.trim();
            if (trimmed.starts_with("## ") || trimmed.starts_with("# "))
                && !trimmed.starts_with("###")
            {
                // Save previous chapter
                if !first || !current_text.trim().is_empty() {
                    let title = if current_title.is_empty() {
                        Self::filename_title(&file_path)
                    } else {
                        current_title.clone()
                    };
                    chapters.push((title, current_text.clone()));
                }
                current_title = trimmed
                    .trim_start_matches('#')
                    .trim()
                    .to_string();
                current_text.clear();
                first = false;
            } else {
                if !current_text.is_empty() {
                    current_text.push('\n');
                }
                current_text.push_str(line);
            }
        }

        // Last chapter
        if !current_text.trim().is_empty() || !current_title.is_empty() {
            let title = if current_title.is_empty() {
                Self::filename_title(&file_path)
            } else {
                current_title
            };
            chapters.push((title, current_text));
        }

        // If no headings found, treat whole file as one chapter
        if chapters.is_empty() {
            chapters.push((Self::filename_title(&file_path), raw));
        }

        // Tokenize into words, extract images per chapter
        let mut words = Vec::new();
        let mut chapter_texts = Vec::new();
        let mut all_chapter_images: Vec<Vec<ChapterImage>> = Vec::new();
        let base_dir = Path::new(&file_path).parent().map(|p| p.to_path_buf());

        for (ci, (_title, text)) in chapters.iter().enumerate() {
            // ── Extract markdown images before tokenizing ──
            let mut chapter_images: Vec<ChapterImage> = Vec::new();
            let mut cleaned = String::new();
            let mut remaining = text.as_str();
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());

            while let Some(bang_idx) = remaining.find("![") {
                // Text before the image
                cleaned.push_str(&remaining[..bang_idx]);

                // Find the closing parenthesis
                if let Some(open_paren) = remaining[bang_idx..].find('(') {
                    let paren_start = bang_idx + open_paren;
                    if let Some(close_paren) = remaining[paren_start..].find(')') {
                        let path_str = &remaining[paren_start + 1..paren_start + close_paren];

                        // Compute word_offset from cleaned text so far
                        let word_offset = cleaned.split_whitespace().count();

                        // Try to load the image
                        if !path_str.is_empty() && !path_str.starts_with("http") {
                            let img_path = if path_str.starts_with('/') {
                                Path::new(path_str).to_path_buf()
                            } else if let Some(ref base) = base_dir {
                                base.join(path_str)
                            } else {
                                Path::new(path_str).to_path_buf()
                            };

                            // Cache the image
                            if img_path.exists() {
                                let mut hasher = Sha256::new();
                                hasher.update(img_path.to_string_lossy().as_bytes());
                                let hash = format!("{:x}", hasher.finalize());
                                let cache_dir = format!("{}/.cache/volta/images", home);
                                let _ = fs::create_dir_all(&cache_dir);
                                let cache_path = format!("{}/{}.png", cache_dir, hash);

                                if !Path::new(&cache_path).exists() {
                                    let _ = fs::copy(&img_path, &cache_path);
                                }

                                let (w, h) = if Path::new(&cache_path).exists() {
                                    image::ImageReader::new(
                                        std::io::Cursor::new(&fs::read(&cache_path).unwrap_or_default()))
                                        .with_guessed_format().ok()
                                        .and_then(|r| r.into_dimensions().ok())
                                        .unwrap_or((0, 0))
                                } else {
                                    (0, 0)
                                };

                                chapter_images.push(ChapterImage {
                                    word_offset,
                                    cached_path: cache_path,
                                    width: w,
                                    height: h,
                                });
                            }
                        }

                        // Skip past the entire ![...](...) syntax
                        remaining = &remaining[paren_start + close_paren + 1..];
                        continue;
                    }
                }
                // Malformed — treat ![ as literal text
                cleaned.push_str("![");
                remaining = &remaining[bang_idx + 2..];
            }
            // Remaining text after last image
            cleaned.push_str(remaining);

            all_chapter_images.push(chapter_images);
            // Paragraph-aware normalize: collapse single newlines
            // (CommonMark soft breaks) to spaces within a paragraph,
            // collapse 3+ newlines to one blank line between paragraphs.
            let trimmed = cleaned
                .trim()
                .split("\n\n")
                .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            for w in trimmed.split_whitespace() {
                words.push(Word::new(w.to_string(), ci as u32));
            }
            chapter_texts.push(trimmed);
        }

        let chapter_titles: Vec<String> =
            chapters.into_iter().map(|(t, _)| t).collect();

        let word_cstrings: Vec<CString> =
            words.iter().map(|w| w.to_cstring()).collect();
        let chapter_title_cstrings: Vec<CString> = chapter_titles
            .iter()
            .map(|t| CString::new(t.as_str()).unwrap_or_default())
            .collect();
        let chapter_text_cstrings: Vec<CString> = chapter_texts
            .iter()
            .map(|t| CString::new(t.as_str()).unwrap_or_default())
            .collect();

        // --- pre-build C image data for FFI ---
        let mut chapter_image_c: Vec<Vec<crate::ChapterImageC>> = Vec::new();
        let mut chapter_image_path_cstrings: Vec<Vec<CString>> = Vec::new();
        for images in &all_chapter_images {
            let mut c_images = Vec::new();
            let mut c_paths = Vec::new();
            for img in images {
                c_paths.push(CString::new(img.cached_path.as_str()).unwrap_or_default());
                let c_ptr = c_paths.last().unwrap().as_ptr();
                c_images.push(crate::ChapterImageC {
                    word_offset: img.word_offset as u32,
                    cached_path: c_ptr,
                    width: img.width,
                    height: img.height,
                });
            }
            chapter_image_c.push(c_images);
            chapter_image_path_cstrings.push(c_paths);
        }

        Ok(MdDoc {
            file_path,
            words,
            word_cstrings,
            chapter_titles,
            chapter_title_cstrings,
            chapter_text_cstrings,
            chapter_texts,
            chapter_image_c,
            chapter_image_path_cstrings,
            chapter_images: all_chapter_images,
        })
    }

    fn filename_title(file_path: &str) -> String {
        Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    }
}

impl Document for MdDoc {
    fn title(&self) -> &str {
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
