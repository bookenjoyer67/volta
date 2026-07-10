//! Cover image extraction and caching.
//!
//! EPUB: reads cover from manifest via rbook.
//! PDF: renders page 1 as a small PNG via pdftoppm.
//! MD: no cover.
//!
//! Thumbnails are cached in `~/.cache/volta/covers/<sha256>.png`.
//! Returns the absolute path to the cached thumbnail, or None.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Extract and cache a cover thumbnail for the given file.
/// Returns the path to the cached PNG, or None if no cover could be extracted.
pub fn extract_cover(file_path: &str, format: &str) -> Option<String> {
    let cache_path = cover_cache_path(file_path)?;
    if cache_path.exists() {
        return Some(cache_path.to_string_lossy().to_string());
    }

    let result = match format {
        "epub" => extract_epub_cover(file_path, &cache_path),
        "pdf" => extract_pdf_cover(file_path, &cache_path),
        _ => None,
    };

    result.map(|_| cache_path.to_string_lossy().to_string())
}

/// Get the deterministic cache path for a cover thumbnail.
fn cover_cache_path(file_path: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let mut hasher = Sha256::new();
    hasher.update(file_path.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let dir = PathBuf::from(format!("{}/.cache/volta/covers", home));
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join(format!("{}.png", hash)))
}

/// Extract cover from EPUB via rbook manifest.
fn extract_epub_cover(file_path: &str, cache_path: &Path) -> Option<()> {
    let epub = rbook::Epub::open(file_path).ok()?;
    let cover_entry = epub.manifest().cover_image()?;
    let bytes = cover_entry.read_bytes().ok()?;

    // Resize to thumbnail using the image crate
    let img = image::load_from_memory(&bytes).ok()?;
    let thumb = img.thumbnail(120, 160); // ~6x4 terminal cells at 2:1 ratio
    thumb.save(cache_path).ok()?;
    Some(())
}

/// Render PDF page 1 as a thumbnail via pdftoppm.
fn extract_pdf_cover(file_path: &str, cache_path: &Path) -> Option<()> {
    let prefix = cache_path.with_extension(""); // strip .png
    let output = Command::new("pdftoppm")
        .args(&[
            "-f", "1",
            "-l", "1",
            "-r", "30",
            "-png",
            "-scale-to", "200",
            "-singlefile",
            file_path,
        ])
        .arg(&prefix)
        .output()
        .ok()?;

    if output.status.success() && cache_path.exists() {
        Some(())
    } else {
        None
    }
}

/// Detect if we're running in a Kitty-compatible terminal.
pub fn is_kitty() -> bool {
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("TERM")
            .map(|t| t.contains("kitty"))
            .unwrap_or(false)
}

/// Clear all kitty images from the terminal.
pub fn kitty_clear_all() {
    if !is_kitty() {
        return;
    }
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(b"\x1b_Ga=d\x1b\\");
    let _ = handle.flush();
}

/// Display a PNG image at a specific terminal position using Kitty graphics protocol.
/// Requires kitty terminal.  Does nothing if not in kitty.
///
/// `row` and `col` are 1-based terminal positions.
/// `width` and `height` are in terminal cells.
pub fn kitty_display_image(
    path: &str,
    row: u16,
    col: u16,
    width_cells: u16,
    height_cells: u16,
) {
    if !is_kitty() {
        return;
    }

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(_) => return,
    };

    // Kitty graphics protocol
    // \x1b_Ga=T,f=100,s=<w>,v=<h>,c=<cols>,r=<rows>;<base64>\x1b\\
    // We need image dimensions for s= and v=.  Read from PNG header.
    let (img_w, img_h) = match image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
    {
        Some(dims) => dims,
        None => return,
    };

    use std::io::Write;

    // We need base64 encoding.  Use a simple inline implementation
    // since we don't want to add a base64 dependency.
    let b64 = base64_encode(&bytes);

    // Move cursor to position, then emit kitty image
    let cmd = format!(
        "\x1b[{};{}H\x1b_Ga=T,f=100,s={},v={},c={},r={};{}\x1b\\\\",
        row + 1,
        col + 1,
        img_w,
        img_h,
        width_cells,
        height_cells,
        b64
    );

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = handle.write_all(cmd.as_bytes());
    let _ = handle.flush();
}

/// Simple base64 encoder (no external dependency).
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((triple >> 6) & 0x3F) as usize] as char
        } else {
            b'=' as char
        });
        out.push(if chunk.len() > 2 {
            CHARS[(triple & 0x3F) as usize] as char
        } else {
            b'=' as char
        });
    }

    out
}
