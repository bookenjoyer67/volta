//! Volta TUI — terminal ebook reader with RSVP speed reading.
//!
//! Usage: volta-tui [file.epub]

use std::collections::HashMap;
use std::path::Path;
use volta_core::doc::Document;
use volta_core::epub::EpubDoc;
use volta_core::player::PlayerState;
use volta_core::DocEnum;

mod tui;

/// Saved progress entry shape (matches progress.json).
#[derive(serde::Deserialize, Default)]
struct SavedProgress {
    chapter: Option<usize>,
    cursor_word: Option<usize>,
}

fn load_progress(path: &str) -> Option<SavedProgress> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let progress_file = format!("{}/.local/share/volta/progress.json", home);
    eprintln!("[volta] Loading progress from {} for key {}", progress_file, path);
    let mut data: HashMap<String, SavedProgress> =
        std::fs::read_to_string(&progress_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
    eprintln!("[volta] Loaded {} entries. Keys: {:?}", data.len(), data.keys().collect::<Vec<_>>());
    let result = data.remove(path);
    eprintln!("[volta] Found entry for '{}': {:?}", path, result.as_ref().map(|s| (s.chapter, s.cursor_word)));
    result
}

fn open_epub(path: &Path) -> DocEnum {
    let epub = EpubDoc::open(path).unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });
    let total = epub.word_count() as usize;
    DocEnum::Epub(epub, PlayerState::new(total, 300))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() >= 2 && (args[1] == "--help" || args[1] == "-h") {
        print_help();
        return;
    }

    // No file argument → start in menu mode
    if args.len() < 2 {
        let app = tui::App::new_menu();
        if let Err(e) = tui::run(app) {
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // File argument → open directly
    let path = Path::new(&args[1]);
    let path_str = path.to_string_lossy().to_string();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let doc = match ext.as_str() {
        "epub" => open_epub(path),
        "pdf" => {
            eprintln!("PDF support in TUI mode coming soon. Use --gui for PDF.");
            std::process::exit(1);
        }
        _ => {
            eprintln!("Unsupported format: .{}", ext);
            eprintln!("Supported: .epub");
            std::process::exit(1);
        }
    };

    // Restore saved position
    let saved = load_progress(&path_str);
    let (start_chapter, start_cursor) = saved
        .as_ref()
        .map(|s| (s.chapter.unwrap_or(0), s.cursor_word.unwrap_or(0)))
        .unwrap_or((0, 0));

    let mut app = tui::App::new(doc, path_str.clone());
    app.set_position(start_chapter, start_cursor);
    tui::menu::MenuState::add_recent(&path_str);

    if let Err(e) = tui::run(app) {
        eprintln!("TUI error: {}", e);
        std::process::exit(1);
    }
}

fn print_help() {
    eprintln!("Volta TUI — terminal ebook reader with RSVP speed reading");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("  volta-tui                  Open menu");
    eprintln!("  volta-tui <file.epub>     Open EPUB in reader mode");
    eprintln!();
    eprintln!("MENU:");
    eprintln!("  ↑/↓        Navigate items");
    eprintln!("  Enter      Open selected");
    eprintln!("  Ctrl+O     Browse files");
    eprintln!("  Esc        Quit");
    eprintln!();
    eprintln!("READER:");
    eprintln!("  arrows     Move cursor word-by-word / line-by-line");
    eprintln!("  j/k        Scroll down/up 3 lines");
    eprintln!("  Ctrl+d/u   Half-page down/up");
    eprintln!("  Ctrl+f/b   Full page down/up");
    eprintln!("  gg/G       Jump to chapter top/bottom");
    eprintln!("  n/p        Next/previous chapter");
    eprintln!("  r          Enter RSVP speed reading at cursor");
    eprintln!("  Esc/q      Quit");
    eprintln!();
    eprintln!("RSVP:");
    eprintln!("  Space      Play / Pause");
    eprintln!("  h/l        Seek back/forward 10 words");
    eprintln!("  k/j        Seek back/forward 100 words");
    eprintln!("  arrows     Same as hjkl");
    eprintln!("  =/-        Increase/decrease WPM");
    eprintln!("  s          Toggle stats overlay");
    eprintln!("  Esc        Exit RSVP, return to reader");
}
