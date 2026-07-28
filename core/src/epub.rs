//! EPUB document backend.
//!
//! Uses the `rbook` crate's EpubReader API for sequential chapter
//! extraction.  Every word is assigned a chapter_index so the FFI
//! layer can answer "which chapter does word N belong to?" in O(1).
//!
//! HTML tags are stripped with a simple state machine (no full XML
//! parser — good enough for 99% of real-world EPUB content).
//! Inline images (<img> tags) are extracted, cached to disk, and
//! tracked by word_offset so renderers can interleave them.

use crate::doc::Document;
use crate::types::{Chapter, Word, BookMetadata, ChapterImage};
use sha2::{Digest, Sha256};
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Pre-built C-compatible image info for FFI (per-chapter vectors).
    /// Pointers into `chapter_image_path_cstrings`.
    pub chapter_image_c: Vec<Vec<crate::ChapterImageC>>,
    /// Storage for CString paths referenced by chapter_image_c.
    pub chapter_image_path_cstrings: Vec<Vec<CString>>,
}

/// Get the deterministic cache path for a content image.
fn image_cache_path(epub_path: &str, image_href: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let mut hasher = Sha256::new();
    hasher.update(epub_path.as_bytes());
    hasher.update(b"\x00");
    hasher.update(image_href.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let dir = PathBuf::from(format!("{}/.cache/volta/images", home));
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join(format!("{}.png", hash)))
}

/// Resolve a relative image src against a spine entry's base href.
///
/// Example: base="/EPUB/OEBPS/chapter1.xhtml", src="images/foo.png"
/// → "/EPUB/OEBPS/images/foo.png"
///
/// Percent-encodes special characters in the resolved path to match
/// rbook's manifest href format (+ → %2B, etc.).
fn resolve_image_src(base_href: &str, src: &str) -> String {
    // Find the last '/' in base_href to get the directory
    let dir = match base_href.rfind('/') {
        Some(pos) => &base_href[..pos],
        None => "",
    };

    // Handle relative path components
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for segment in src.split('/') {
        match segment {
            "." | "" => {} // skip empty and current-dir
            ".." => {
                // Don't pop the last directory (can't go above EPUB root)
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            _ => parts.push(segment),
        }
    }

    // Build the resolved path, percent-encoding special characters
    let raw = format!("/{}", parts.join("/"));
    percent_encode_path(&raw)
}

/// Percent-encode special characters in a URL path segment.
/// rbook stores manifest hrefs in percent-encoded form where
/// characters like + are encoded as %2B.
fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for ch in path.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '/' | '-' | '_' | '.' | '~' | '%' => {
                out.push(ch);
            }
            _ => {
                // Encode as UTF-8 bytes, then %XX per byte
                let mut buf = [0u8; 4];
                let encoded = ch.encode_utf8(&mut buf);
                for byte in encoded.bytes() {
                    out.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    out
}

impl EpubDoc {
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
                            if let Some(num_str) = entity.strip_prefix('#') {
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
    ///
    /// Inline <img> tags are extracted, cached to
    /// `~/.cache/volta/images/<sha256>.png`, and tracked by word_offset
    /// in each Chapter.
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
        let manifest = epub.manifest();
        let mut reader = epub.reader();
        let mut words = Vec::new();
        let mut chapters = Vec::new();
        let mut ci: u32 = 0;
        let epub_path_str = path.to_string_lossy().to_string();

        while let Some(content_result) = reader.read_next() {
            let content = content_result
                .map_err(|e| format!("Failed to read chapter: {}", e))?;
            // rbook's content() strips HTML — read raw XHTML for image extraction
            let raw_xhtml = content.manifest_entry().read_str()
                .map_err(|e| format!("Failed to read raw XHTML: {}", e))?;
            let raw_text: &str = &raw_xhtml; // raw XHTML with <img> tags intact
            let spine_entry = content.spine_entry(); // metadata for this entry

            // Get the base href of this spine entry for image path resolution
            let base_href = content.manifest_entry().href().to_string();

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

            // Strip HTML comments first (MS Office conditional comments
            // like <!--[if gte mso]>...<![endif]-->> carry metadata junk).
            let raw_text: std::borrow::Cow<str> = if raw_text.contains("<!--") {
                let mut stripped = String::with_capacity(raw_text.len());
                let mut rest = raw_text;
                while let Some(start) = rest.find("<!--") {
                    stripped.push_str(&rest[..start]);
                    rest = match rest[start..].find("-->") {
                        Some(end) => &rest[start + end + 3..],
                        None => "",
                    };
                }
                stripped.push_str(rest);
                stripped.into()
            } else {
                raw_text.into()
            };

            // --- HTML stripping + image detection ---
            // Character-level state machine: tag contents are captured so
            // block-level closing tags (</p>, </div>, headings, <br>, ...)
            // can emit paragraph breaks. <img> tags are recorded with their
            // clean_text position so word_offset is computed from actual
            // visible text, not raw HTML tokens.
            const BLOCK_TAGS: &[&str] = &[
                "/p", "/div", "/h1", "/h2", "/h3", "/h4", "/h5", "/h6", "/li",
                "/blockquote", "/section", "/article", "/tr", "br", "br/",
                "hr", "hr/",
            ];
            let mut clean_text = String::new();
            let mut in_tag = false;
            let mut tag_buf = String::new();
            // Content of these blocks is code, not prose — drop it entirely.
            let mut skip_block: Option<&'static str> = None;
            // Record <img> tag positions in clean_text for word_offset computation.
            let mut image_tags: Vec<(usize, String)> = Vec::new();

            for ch in raw_text.chars() {
                match ch {
                    '<' => {
                        in_tag = true;
                        tag_buf.clear();
                    }
                    '>' => {
                        in_tag = false;
                        let name = tag_buf.trim().to_ascii_lowercase();
                        let name = name.split_whitespace().next().unwrap_or("");
                        // Record <img> tag before any block-tag processing
                        if name == "img" && skip_block.is_none() {
                            image_tags.push((clean_text.len(), tag_buf.clone()));
                        }
                        match name {
                            "style" => skip_block = Some("/style"),
                            "script" => skip_block = Some("/script"),
                            _ => {}
                        }
                        if Some(name) == skip_block {
                            skip_block = None;
                            continue;
                        }
                        if skip_block.is_none() && BLOCK_TAGS.contains(&name) {
                            clean_text.push_str("\n\n");
                        }
                    }
                    _ if in_tag => {
                        tag_buf.push(ch);
                    }
                    _ => {
                        if skip_block.is_none() {
                            clean_text.push(ch);
                        }
                    }
                }
            }

            // --- entity decoding ---
            // EPUBs encode special characters as HTML entities.
            // &#8211; → – (en dash), &mdash; → —, &ldquo; → ", etc.
            let clean_text = Self::decode_entities(&clean_text);

            // Paragraph-aware whitespace normalization:
            // within a paragraph collapse all whitespace runs to a single
            // space; preserve exactly one blank line between paragraphs.
            let clean_text = clean_text
                .split("\n\n")
                .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            // --- compute word_offsets for images ---
            // Image tags were recorded during HTML stripping with their
            // clean_text position, giving an exact word_offset.
            let mut chapter_images: Vec<ChapterImage> = Vec::new();

            for (clean_pos, tag_content) in &image_tags {
                let tag_lower = tag_content.to_ascii_lowercase();
                if let Some(src_start) = tag_lower.find("src=") {
                    let rest = &tag_content[src_start + 4..];
                    let quote_char = rest.chars().next().unwrap_or('"');
                    if quote_char == '"' || quote_char == '\'' {
                        if let Some(src_end) = rest[1..].find(quote_char) {
                            let src = &rest[1..src_end + 1];
                            let resolved = resolve_image_src(&base_href, src);

                            // word_offset from clean_text position, not raw HTML
                            let prefix = &clean_text[..(*clean_pos).min(clean_text.len())];
                            let word_offset = prefix.split_whitespace().count();

                            // Extract and cache the image
                            if let Some(cache_path) =
                                image_cache_path(&epub_path_str, &resolved)
                            {
                                if !cache_path.exists() {
                                    if let Some(entry) = manifest.by_href(&resolved) {
                                        if let Ok(bytes) = entry.read_bytes() {
                                            if let Ok(img) = image::load_from_memory(&bytes) {
                                                let _ = img.save(&cache_path);
                                            }
                                        }
                                    }
                                }

                                // Get dimensions
                                let (w, h) = if cache_path.exists() {
                                    image::ImageReader::new(std::io::Cursor::new(
                                        &fs::read(&cache_path).unwrap_or_default(),
                                    ))
                                    .with_guessed_format()
                                    .ok()
                                    .and_then(|r| r.into_dimensions().ok())
                                    .unwrap_or((0, 0))
                                } else {
                                    (0, 0)
                                };

                                chapter_images.push(ChapterImage {
                                    word_offset,
                                    cached_path: cache_path.to_string_lossy().to_string(),
                                    width: w,
                                    height: h,
                                });
                            }
                        }
                    }
                }
            }

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
                images: chapter_images,
            });

            ci += 1;
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

        // --- pre-build C image data for FFI ---
        let mut chapter_image_c: Vec<Vec<crate::ChapterImageC>> = Vec::new();
        let mut chapter_image_path_cstrings: Vec<Vec<CString>> = Vec::new();
        for ch in &chapters {
            let mut c_images = Vec::new();
            let mut c_paths = Vec::new();
            for img in &ch.images {
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

        Ok(EpubDoc {
            metadata,
            words,
            word_cstrings,
            chapters,
            chapter_title_cstrings,
            chapter_text_cstrings,
            chapter_image_c,
            chapter_image_path_cstrings,
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
