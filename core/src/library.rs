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
