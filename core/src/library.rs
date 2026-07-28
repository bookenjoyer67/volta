//! Library — persistent book metadata and reading progress.
//!
//! Stored in `~/.local/share/volta/library.json`.  Each entry is
//! keyed by absolute file path.  Entries are ordered by most recent
//! first (the order they appear in the JSON map — serde preserves
//! insertion order in the output, and we re-insert on update).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub title: String,
    pub author: String,
    pub format: String,    // "epub", "pdf", "md"
    pub chapter_count: u32,
    pub current_chapter: u32,
    pub current_word: usize,
    pub last_opened: u64,  // unix timestamp
    pub added: u64,
    /// Optional path to cached cover thumbnail (~/.cache/volta/covers/<hash>.png)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_path: Option<String>,
}

pub struct Library {
    /// Ordered by most recent first.  Keyed by absolute path.
    map: HashMap<String, LibraryEntry>,
    /// Ordered list of paths (most recent first).
    order: Vec<String>,
    file_path: PathBuf,
}

impl Library {
    pub fn load() -> Self {
        let file_path = library_path();
        let raw = fs::read_to_string(&file_path).unwrap_or_default();
        let map: HashMap<String, LibraryEntry> =
            serde_json::from_str(&raw).unwrap_or_default();

        // Reconstruct order from the JSON map (serde_json preserves
        // insertion order when deserializing into a BTreeMap-style map,
        // but HashMap doesn't — so we read the raw JSON array of keys).
        let order: Vec<String> = if let Ok(v) =
            serde_json::from_str::<serde_json::Value>(&raw)
        {
            v.as_object()
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        Library {
            map,
            order,
            file_path,
        }
    }

    pub fn entries(&self) -> Vec<(&str, &LibraryEntry)> {
        self.order
            .iter()
            .filter_map(|path| self.map.get(path).map(|e| (path.as_str(), e)))
            .collect()
    }

    pub fn get(&self, path: &str) -> Option<&LibraryEntry> {
        self.map.get(path)
    }

    /// Add or update an entry.  Moves it to the front (most recent).
    pub fn upsert(&mut self, path: &str, entry: LibraryEntry) {
        // Remove from order if already present
        self.order.retain(|p| p != path);
        self.order.insert(0, path.to_string());
        self.map.insert(path.to_string(), entry);
    }

    /// Update reading progress for a book.  Does NOT change order.
    pub fn update_progress(&mut self, path: &str, chapter: u32, word: usize) {
        if let Some(entry) = self.map.get_mut(path) {
            entry.current_chapter = chapter;
            entry.current_word = word;
            entry.last_opened = now_secs();
        }
    }

    /// Touch last_opened and move to front (called on open).
    pub fn touch(&mut self, path: &str) {
        if self.map.contains_key(path) {
            self.order.retain(|p| p != path);
            self.order.insert(0, path.to_string());
        }
        if let Some(entry) = self.map.get_mut(path) {
            entry.last_opened = now_secs();
        }
    }

    /// Remove an entry from the library.
    pub fn remove(&mut self, path: &str) {
        self.order.retain(|p| p != path);
        self.map.remove(path);
    }

    pub fn save(&self) {
        // Rebuild serialization map in order
        let ordered: serde_json::Map<String, serde_json::Value> = self
            .order
            .iter()
            .filter_map(|path| {
                self.map.get(path).map(|entry| {
                    (
                        path.clone(),
                        serde_json::to_value(entry).unwrap_or_default(),
                    )
                })
            })
            .collect();

        let json = serde_json::Value::Object(ordered);
        if let Some(parent) = self.file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(
            &self.file_path,
            serde_json::to_string_pretty(&json).unwrap_or_default(),
        );
    }

    /// Create a library backed by a specific file (for testing).
    /// Unlike `load()`, this doesn't read from `~/.local/share/volta/`.
    pub fn with_path(file_path: PathBuf) -> Self {
        let raw = fs::read_to_string(&file_path).unwrap_or_default();
        let map: HashMap<String, LibraryEntry> =
            serde_json::from_str(&raw).unwrap_or_default();
        let order: Vec<String> = if let Ok(v) =
            serde_json::from_str::<serde_json::Value>(&raw)
        {
            v.as_object()
                .map(|obj| obj.keys().cloned().collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Library {
            map,
            order,
            file_path,
        }
    }
}

fn library_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{}/.local/share/volta/library.json", home))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_entry(title: &str) -> LibraryEntry {
        LibraryEntry {
            title: title.to_string(),
            author: "Test Author".to_string(),
            format: "epub".to_string(),
            chapter_count: 10,
            current_chapter: 0,
            current_word: 0,
            last_opened: 0,
            added: 0,
            cover_path: None,
        }
    }

    fn temp_library(name: &str) -> Library {
        let dir = std::env::temp_dir().join("volta_tests");
        fs::create_dir_all(&dir).ok();
        let path = dir.join(format!("library_{}.json", name));
        let _ = fs::remove_file(&path);
        Library::with_path(path)
    }

    #[test]
    fn upsert_adds_entry() {
        let mut lib = temp_library("upsert_adds");
        lib.upsert("/books/a.epub", test_entry("Book A"));
        assert!(lib.get("/books/a.epub").is_some());
    }

    #[test]
    fn upsert_moves_to_front() {
        let mut lib = temp_library("upsert_order");
        lib.upsert("/books/a.epub", test_entry("A"));
        lib.upsert("/books/b.epub", test_entry("B"));
        let entries = lib.entries();
        assert_eq!(entries[0].0, "/books/b.epub");
        // Re-upsert A → moves to front
        lib.upsert("/books/a.epub", test_entry("A"));
        let entries = lib.entries();
        assert_eq!(entries[0].0, "/books/a.epub");
    }

    #[test]
    fn remove_deletes_entry() {
        let mut lib = temp_library("remove");
        lib.upsert("/books/a.epub", test_entry("A"));
        lib.remove("/books/a.epub");
        assert!(lib.get("/books/a.epub").is_none());
        assert!(lib.entries().is_empty());
    }

    #[test]
    fn update_progress_preserves_order() {
        let mut lib = temp_library("progress_order");
        lib.upsert("/books/a.epub", test_entry("A"));
        lib.upsert("/books/b.epub", test_entry("B"));
        lib.update_progress("/books/a.epub", 5, 100);
        let entries = lib.entries();
        assert_eq!(entries[0].0, "/books/b.epub");
        assert_eq!(entries[1].0, "/books/a.epub");
    }

    #[test]
    fn touch_moves_to_front() {
        let mut lib = temp_library("touch");
        lib.upsert("/books/a.epub", test_entry("A"));
        lib.upsert("/books/b.epub", test_entry("B"));
        lib.touch("/books/a.epub");
        let entries = lib.entries();
        assert_eq!(entries[0].0, "/books/a.epub");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut lib = temp_library("roundtrip");
        lib.upsert("/books/a.epub", test_entry("A"));
        lib.save();

        let lib2 = Library::with_path(lib.file_path.clone());
        let entries = lib2.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "/books/a.epub");
        assert_eq!(entries[0].1.title, "A");
    }
}
