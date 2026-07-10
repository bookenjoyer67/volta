//! Markdown document backend.
//!
//! Reads .md files as plain text. Splits on `##` or `#` headings
//! into chapters so long docs are navigable. Headings become chapter
//! titles. Text between headings is the chapter body.

use crate::doc::Document;
use crate::types::Word;
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

        // Tokenize into words
        let mut words = Vec::new();
        let mut chapter_texts = Vec::new();

        for (ci, (_title, text)) in chapters.iter().enumerate() {
            let trimmed = text.trim().to_string();
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

        eprintln!(
            "MD loaded: {} chapters, {} words",
            chapter_texts.len(),
            words.len()
        );

        Ok(MdDoc {
            file_path,
            words,
            word_cstrings,
            chapter_titles,
            chapter_title_cstrings,
            chapter_text_cstrings,
            chapter_texts,
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
