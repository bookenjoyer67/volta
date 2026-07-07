# Volta — Architecture

## What It Is

A desktop ebook reader for Linux that can read EPUB and PDF files.
The normal reading mode shows flowing pages of text. The RSVP mode
plays words one at a time at adjustable speed. RSVP is a feature,
not the whole app.

Core in Rust, GUI in Lua (LÖVE, loaded via LuaJIT FFI).

---

## Name

**Volta** — Italian for "turn" or "time." Also the electrical unit for
potential difference (flow). A volta is also the structural turn in a
sonnet. Three meanings, one word: the flow through text, the turning
of pages, the moment of understanding. Short, searchable, no naming
collisions.

---

## Architecture Diagram

```
┌────────────────────────────────────────────────────────────────────┐
│  LÖVE (Lua)                                                        │
│                                                                     │
│  ┌──────┐ ┌────────┐ ┌─────────┐ ┌─────────┐ ┌────────┐          │
│  │ menu │ │ reader │ │  rsvp   │ │ library │ │ config │          │
│  │ .lua │ │ .lua   │ │  .lua   │ │ .lua    │ │ .lua   │          │
│  └──────┘ └────────┘ └─────────┘ └─────────┘ └────────┘          │
│       │         │          │          │                            │
│  ┌────┴─────────┴──────────┴──────────┴────────────────────┐      │
│  │                    FFI bridge (ffi.lua)                  │      │
│  └────────────────────────┬─────────────────────────────────┘      │
│                           │ C ABI call                            │
└───────────────────────────┼─────────────────────────────────────────┘
                            │
┌───────────────────────────┼─────────────────────────────────────────┐
│  librsvp_core.so (Rust)  │                                          │
│                           ▼                                          │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │                    Document Trait                             │  │
│  │  + title(), total_words(), word_at(i), chapter_at(i),        │  │
│  │    render_page(n, scale) → Image, page_count()               │  │
│  └──────┬────────────────────────────┬──────────────────────────┘  │
│         │                            │                             │
│  ┌──────▼──────┐           ┌─────────▼──────────┐                 │
│  │ EpubDoc     │           │ PdfDoc              │                 │
│  │ rbook crate │           │ poppler (pdftoppm)  │                 │
│  │ reflow text │           │ page images         │                 │
│  └─────────────┘           └────────────────────┘                 │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  PlayerState                                                   │  │
│  │  current_index, wpm, is_playing, accumulator (ms)             │  │
│  │  tick(dt_ms) → advances position                               │  │
│  │  seek(n), set_wpm(wpm), play(), pause()                       │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  Cache (page images)                                          │  │
│  │  ~/.cache/volta/<doc_hash>/page_001.png                       │  │
│  │  LRU eviction, rendered at requested DPI                      │  │
│  └──────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Two Reading Modes

### 1. Normal Reader (reflowed text / page images)

| Format  | How it renders                          |
|---------|-----------------------------------------|
| EPUB    | Extract chapters via rbook, reflow text in LÖVE with configurable font, size, margins, line height. Dark/light themes. Chapter breaks. |
| PDF     | Render each page as PNG via pdftoppm (poppler). Display full-page images. Zoom, scroll, page-by-page navigation. Cache in ~/.cache/volta/. |

Normal reader features:
- Next/previous page (arrow keys)
- Chapter navigation (EPUB: real chapters; PDF: inferred from page ranges)
- Word highlighting / selection (future)
- Adjustable font size, theme, margins (EPUB)
- Zoom level (PDF)
- Progress bar with chapter markers
- Open file dialog

### 2. RSVP Reader

A separate view mode. Enter from any position in the normal reader,
exit back to the same position.

- Extracts flat word list from the loaded document (works for both EPUB and PDF)
- Words displayed one at a time, centered, large font
- ORP (Optimal Recognition Point) — fixation letter highlighted within each word
- Space = play/pause, Arrows = seek, Up/Down = WPM adjust
- WPM range: 100–1000
- Returns to normal reader at the same word position on exit

---

## Themes

We cannot render publisher CSS, so themes are the identity of the app.
Every visual parameter of both reading modes is driven by the active
theme. 8 built-in themes ship with Volta. Users can drop custom themes
into `~/.config/volta/themes/`.

### What a Theme Controls

```
Reader mode:
  font_family       — "Serif", "Sans", "Mono", or any system font name
  font_size         — base size in points (16 default)
  line_height       — multiplier (1.6 default)
  paragraph_spacing — extra space after paragraphs (12px default)
  margins           — { top, bottom, left, right } in px
  justify           — left-align or justify (boolean)
  background        — hex color
  text              — hex color
  heading_color     — hex color
  link_color        — hex color
  selection_color   — hex color (highlighted text)

RSVP mode:
  font_family       — font for word display
  font_size         — word display size (48 default)
  background        — hex color
  text              — word color
  orp_color         — optimal recognition point highlight color
  orp_position      — fraction into word (0.35 default)
```

### Theme File Format

Lua files returning a table. Stored in `~/.config/volta/themes/<name>.lua`
or bundled in `frontend/themes/`. Loaded via sandboxed `load()`.

```lua
-- ~/.config/volta/themes/sepia.lua
return {
  name = "Sepia",
  description = "Warm cream background, soft brown text — easy on the eyes",
  author = "Volta",

  reader = {
    font_family       = "Serif",
    font_size         = 18,
    line_height       = 1.6,
    paragraph_spacing = 12,
    margins           = { top = 80, bottom = 80, left = 60, right = 60 },
    justify           = true,
    background        = "#F5F0E8",
    text              = "#3D2B1F",
    heading_color     = "#5C3A1E",
    link_color        = "#8B4513",
    selection_color   = "#D4A574",
  },

  rsvp = {
    font_family   = "Mono",
    font_size     = 52,
    background    = "#1A1A2E",
    text          = "#E0E0E0",
    orp_color     = "#FF6B6B",
    orp_position  = 0.35,
  },
}
```

### Built-in Themes

| Theme         | Reader bg  | Reader text | Vibe                    |
|---------------|------------|-------------|-------------------------|
| **Daylight**  | `#FFFFFF`  | `#1A1A1A`   | Clean white, serif      |
| **Sepia**     | `#F5F0E8`  | `#3D2B1F`   | Warm cream, brown text  |
| **Night**     | `#1E1E24`  | `#C8C8C8`   | Dark gray, soft white   |
| **Dusk**      | `#1A1A2E`  | `#E8D5B7`   | Deep blue-gray, warm    |
| **Paper**     | `#F8F6F0`  | `#2C2C2C`   | Off-white, book feel    |
| **Terminal**  | `#0D0D0D`  | `#00FF41`   | Green-on-black, retro   |
| **High Contrast** | `#000000` | `#FFFFFF` | Accessibility-first     |
| **Neon**      | `#101417`  | `#00E5FF`   | Rainbow + hot pink accents, cyberpunk |

### Theme Loading Order

1. Default theme (Daylight) shipped in `frontend/themes/daylight.lua`
2. User themes scanned from `~/.config/volta/themes/*.lua` (overrides any
   built-in with the same name)
3. Active theme saved in `~/.local/share/volta/config.json`
4. Theme can be changed at runtime via a menu (no restart needed)

### Custom Theme Import

- User drops a `.lua` file into `~/.config/volta/themes/`
- Or uses an "Import Theme" option in the settings menu that copies a
  file into that directory
- Theme is instantly available in the theme picker on next open

---

## Data Flow

### Opening a document

```
Lua: file selected → call rsvp_open(path)
  │
  ▼
Rust: detect format by extension
  ├── .epub → EpubDoc: rbook → parse chapters → extract words → Vec<Word>
  └── .pdf  → PdfDoc:  poppler → extract metadata + page count
                         For text: pdftotext → extract words per page
                         For rendering: pdftoppm → page images (on demand, cached)
  │
  ▼
Rust: return opaque Document* handle
  │
  ▼
Lua: store handle, query title, word_count, page_count
      for EPUB: start on page 1, reflow first chapter's text
      for PDF:  render page 1 as image, display
```

### Normal reading (EPUB — reflow)

```
Lua requests text for chapter at index N
  → Rust returns chapter text as C string
  → Lua reflows text to fit window width (word-wrap)
  → Renders with love.graphics.print (or printf)
  → On key press: scroll within chapter, or advance to next chapter
```

### Normal reading (PDF — page images)

```
Lua requests page N at scale S
  → Rust checks cache in ~/.cache/volta/<hash>/page_NNNN.png
  → If cached: return image path as C string
  → If not cached: run pdftoppm -f N -l N -r 200 -png file.pdf cache_path
      → Wait for completion → return path
  → Lua: love.graphics.draw(image, ...)
  → Pre-cache next 2 pages in background thread
```

### RSVP mode (both formats)

```
Lua enters RSVP mode at word index I
  → Rust loads full word array (already done during open)
  → Lua timer loop: love.update(dt) → rsvp_tick(handle, dt_ms)
  → Rust: accumulate ms, when ms_per_word exceeded, advance index
  → Lua: rsvp_word_text(handle, current_index) → render centered
  → On exit: rsvp_current_index(handle) → Lua stores for return
```

---

## FFI Surface (C ABI, extern "C")

```c
// Document lifecycle
Document* rsvp_open(const char* path);           // Open EPUB or PDF
void      rsvp_close(Document* doc);              // Free everything

// Metadata
const char* rsvp_title(Document* doc);            // Book title
uint32_t    rsvp_word_count(Document* doc);       // Total words
uint32_t    rsvp_page_count(Document* doc);       // Total pages (PDF) or estimated

// Word access (used by RSVP mode + reflow)
const char* rsvp_word_at(Document* doc, uint32_t i);  // Word text
uint32_t    rsvp_chapter_at(Document* doc, uint32_t i); // Chapter index for word i

// Chapter access (used by normal reader EPUB mode)
uint32_t    rsvp_chapter_count(Document* doc);
const char* rsvp_chapter_title(Document* doc, uint32_t c);
const char* rsvp_chapter_text(Document* doc, uint32_t c);  // Full chapter text for reflow

// Page rendering (used by normal reader PDF mode)
// Returns path to cached PNG, or NULL on error
const char* rsvp_render_page(Document* doc, uint32_t page, uint32_t dpi);

// Player state (used by RSVP mode)
void     rsvp_seek(Document* doc, uint32_t i);
void     rsvp_set_wpm(Document* doc, uint32_t wpm);
void     rsvp_play(Document* doc);
void     rsvp_pause(Document* doc);
bool     rsvp_is_playing(Document* doc);
uint32_t rsvp_current_index(Document* doc);
uint32_t rsvp_tick(Document* doc, double dt_ms);  // Returns new current_index
```

---

## Project Structure

```
volta/
├── ARCHITECTURE.md       — this file
├── README.md             — build, usage, dependencies
├── Cargo.toml            — workspace root
│
├── core/                 — Rust shared library
│   ├── Cargo.toml        — [lib] crate-type = ["cdylib", "staticlib"]
│   │                     — deps: rbook, lopdf, libc
│   └── src/
│       ├── lib.rs        — FFI exports, dispatch to Document trait
│       ├── doc.rs        — Document trait definition
│       ├── epub.rs       — EpubDoc: rbook-based implementation
│       ├── pdf.rs        — PdfDoc: poppler subprocess-based, page cache
│       ├── player.rs     — PlayerState: tick, WPM, accumulator, seek
│       └── types.rs      — shared structs: Word, Chapter, BookMetadata
│
├── frontend/             — LÖVE application
│   ├── main.lua          — love.load/update/draw, global dispatch
│   ├── ffi.lua           — ffi.cdef + ffi.load, all Rust bindings
│   ├── config.lua        — defaults: wpm, font, theme, keybinds
│   ├── input.lua         — keybind dispatch (reader + RSVP contexts)
│   ├── book.lua          — open/bookkeeping state
│   │
│   ├── reader/           — Normal reader mode
│   │   ├── reader.lua    — mode dispatch (epub vs pdf render)
│   │   ├── epub_view.lua — reflow text, chapter navigation
│   │   ├── pdf_view.lua  — page image display, zoom, pan
│   │   └── page.lua      — pagination, progress
│   │
│   ├── rsvp/             — RSVP reader mode
│   │   ├── rsvp.lua      — mode entry/exit, timer loop
│   │   ├── display.lua   — word rendering, ORP highlight
│   │   └── stats.lua     — live WPM, progress %, time remaining
│   │
│   ├── ui/               — Shared UI components
│   │   ├── menu.lua      — file open dialog, welcome screen, recent files
│   │   ├── theme.lua     — theme loader, switcher, preview
│   │   ├── progress.lua  — progress bar with chapter markers
│   │   └── help.lua      — keybind reference overlay
│   │
│   ├── themes/           — Built-in themes
│   │   ├── daylight.lua
│   │   ├── sepia.lua
│   │   ├── night.lua
│   │   ├── dusk.lua
│   │   ├── paper.lua
│   │   ├── terminal.lua
│   │   ├── high_contrast.lua
│   │   └── neon.lua
│   │
│   └── fonts/            — bundled fonts
│       └── JetBrainsMono-Regular.ttf  (or similar)
│
├── build.sh              — cargo build --release + copy .so to frontend/
└── run.sh                — build.sh && love frontend/
```

---

## Dependencies

| Layer   | Dependency    | Purpose                          |
|---------|---------------|----------------------------------|
| Rust    | rbook         | EPUB 2/3 parsing, text extraction |
| Rust    | libc          | C string helpers for FFI         |
| Rust    | sha2          | Content hashing for cache keys   |
| System  | poppler-utils | pdftoppm, pdftotext              |
| System  | love (>=11)   | LÖVE game engine                 |
| System  | Rust toolchain| rustup, cargo, etc.              |

Poppler is the only system-level dependency beyond LÖVE and Rust.
It's in every distro's package manager (poppler-utils on Debian/Arch,
poppler on Fedora).

---

## Edge Cases

- **DRM EPUB**: Show clear error. rbook returns empty/no text.
- **Scanned PDF (image-only)**: pdftotext returns nothing. RSVP mode
  shows "No extractable text." Normal reader still shows page images.
- **Very large PDF (1000+ pages)**: Lazy page rendering. Only render
  requested pages. Cache in background. Max cache = 500MB.
- **EPUB with CSS layout**: rbook gives us access to the raw XHTML.
  We strip HTML tags and reflow. CSS styling is ignored for simplicity
  (reader-mode style, like Firefox Reader View).
- **Mixed RTL/LTR text**: UTF-8 throughout. LÖVE handles bidirectional
  text via HarfBuzz (love 11.5+). No special handling needed.
- **Resume reading**: Store last position + hash in
  `~/.local/share/volta/sessions.json`. Reopen jumps to where you left off.
- **Rsync-dotfile-safe**: All state in `~/.local/share/volta/`,
  cache in `~/.cache/volta/`. Nothing in the repo dir.
- **Theme has missing fields**: theme.lua merges user theme against
  the Daylight default. Missing fields fall through to defaults instead
  of causing errors.
- **Malformed user theme**: Sandboxed load() catches Lua errors. Show
  an error message and fall back to the previous theme. Never crash.
- **Font not found on system**: LÖVE's love.graphics.setNewFont returns
  nil for nonexistent fonts. Fall back to bundled JetBrainsMono (for
  RSVP) or a system sans-serif (for reader mode).
- **Theme file with malicious code**: Sandboxed load() runs in a
  restricted environment — no io, no os, no FFI access. Worst case:
  infinite loop (user closes window).

---

## Limitations (Honest)

- EPUB CSS / styling is not rendered. This is a reader-mode experience
  (every book gets the same clean font/theme regardless of publisher CSS).
- PDF text extraction quality depends on the PDF. Machine-generated PDFs
  (from LaTeX, Word) are perfect. Scanned PDFs yield no text.
- LÖVE renders via OpenGL. On very old hardware or software renderers,
  it may be slow. Modern GPUs handle 2D text effortlessly.
- RSVP is a single-word display mode. You cannot skim, scan, or
  re-read a sentence visually. That's inherent to RSVP as a method.
- LÖVE does not have native file dialogs. The file picker will be a
  simple custom UI or use Zenity/KDialog as a fallback.
