<p align="center">
  <img src="https://img.shields.io/badge/Rust-000?logo=rust&logoColor=fff" alt="Rust">
  <img src="https://img.shields.io/badge/Lua-2C2D72?logo=lua&logoColor=fff" alt="Lua">
  <img src="https://img.shields.io/badge/LÖVE-ff69b4?logo=love&logoColor=fff" alt="LÖVE">
  <img src="https://img.shields.io/badge/license-MIT-brightgreen" alt="License">
</p>

<h1 align="center">
  <code>⚡ Volta</code>
</h1>

<h3 align="center"><em>turn the page. turn the mind.</em></h3>

<p align="center">
  Desktop ebook reader with RSVP speed reading.
  <br>
  Normal reading <b>or</b> one-word-at-a-time flow.
  <br>
  Terminal <b>or</b> GUI. EPUB, PDF, <b>or</b> Markdown.
</p>

---

## Features

| | |
|---|---|
| **Library grid** | Home screen with book covers, titles, authors, progress bars. Arrow key navigation. |
| **Cover images** | EPUB covers extracted automatically. PDF page-1 thumbnails via pdftoppm. Cached in `~/.cache/volta/covers/`. |
| **Dual-mode** | TUI (terminal) when launched from shell, GUI (LÖVE) when launched from desktop |
| **RSVP** | Rapid Serial Visual Presentation — words flash one at a time at configurable speed |
| **Progress saving** | Press `Ctrl+S` to save your position. Resume where you left off. Does NOT auto-save. |
| **8 built-in themes** | Neon, Sepia, Night, Dusk, Daylight, Forest, Ocean, Amber — cycle with `t`/`T` |
| **Full-text search** | Press `/` to search across all chapters. `n` for next match, `N` for previous. Matches highlighted in gold. |
| **Keyboard-driven** | Arrow keys move cursor. `j`/`k` scroll, `n`/`p` chapters, `gg`/`G` top/bottom. |
| **EPUB + PDF + MD** | rbook for EPUB, poppler for PDF, plain-text for Markdown |
| **Cursor-based RSVP entry** | Place cursor on any word in reader mode, press `r` — RSVP starts from that exact position |

## Install

### Dependencies

```
arch:     sudo pacman -S love rust poppler zenity
debian:   sudo apt install love rustc cargo poppler-utils zenity
fedora:   sudo dnf install love rust cargo poppler-utils zenity
```

### Build

```bash
git clone https://github.com/bookenjoyer67/volta.git
cd volta
./build.sh
```

`build.sh` compiles the Rust core (shared library + TUI binary) and copies `libvolta_core.so` into `frontend/`.

### Install launcher

```bash
sudo ln -sf "$(pwd)/volta" /usr/local/bin/volta
```

The `volta` launcher auto-detects whether you're in a terminal (→ TUI) or launched from a desktop icon (→ GUI). It auto-rebuilds when source files change.

## Usage

```bash
volta                      # Open library grid
volta book.epub            # Open EPUB directly in reader mode
volta document.pdf         # PDF via pdftotext (TUI) or page images (GUI)
volta notes.md             # Markdown, split on headings
volta --gui book.epub      # Force GUI mode even from terminal
```

### Desktop entry

```bash
cp volta.desktop ~/.local/share/applications/
```

Then launch Volta from your app launcher. GUI mode, no terminal window.

## Modes

### 🏠 Library

Card grid showing all your books. Covers, titles, authors, progress bars. Arrow keys navigate, Enter opens, `Ctrl+O` browses for new files, `Esc` quits.

| Key | Action |
|-----|--------|
| `↑` `↓` `←` `→` | Navigate cards |
| `Enter` | Open selected book |
| `Ctrl+O` | Browse for file |
| `Esc` | Quit |

### 📖 Reader Mode

Flowing text. Scroll, navigate chapters, place cursor anywhere. Press `r` to drop into RSVP at cursor position.

| Key | Action |
|-----|--------|
| `↓` `↑` `←` `→` | Move cursor word by word or line by line |
| `j` `k` | Scroll down/up 3 lines |
| `Ctrl+d` `Ctrl+u` | Half-page down/up |
| `Ctrl+f` `Ctrl+b` | Full page down/up |
| `gg` | Jump to top of chapter (tap `g` twice) |
| `G` | Jump to bottom of chapter |
| `n` `p` | Next/previous chapter (when no search active) |
| `r` | Enter RSVP at cursor |
| `t` / `T` | Next / previous theme |
| `/` | Search across all chapters (type query, Enter to find) |
| `n` / `N` | Next/previous search match (when search is active) |
| `Esc` | Return to library |
| `Ctrl+s` | Save progress |

### ⚡ RSVP Mode

One word at a time, centered, adjustable speed. Pure reading flow.

| Key | Action |
|-----|--------|
| `Space` | Play / Pause |
| `h` `l` | Seek ±10 words |
| `k` `j` | Seek ±100 words |
| `=` `-` | WPM ±25 |
| `s` | Toggle stats overlay |
| `t` / `T` | Next / previous theme |
| `Esc` | Return to reader |

Full keybindings: [KEYBINDINGS.md](KEYBINDINGS.md)

## Architecture

```
volta/
├── core/              Rust — EPUB/PDF/MD parsing, RSVP engine, TUI (ratatui)
│   ├── src/main.rs    TUI binary entrypoint
│   ├── src/lib.rs     FFI exports for LÖVE bridge
│   ├── src/library.rs Library persistence (library.json)
│   ├── src/cover.rs   Cover extraction (EPUB manifest, PDF thumbnail)
│   ├── src/md.rs      Markdown document backend
│   └── src/tui/       Menu, reader, RSVP views
├── frontend/          Lua — LÖVE GUI, FFI bridge to Rust core
│   ├── main.lua       love.load/draw/update dispatch
│   ├── bridge.lua     LuaJIT FFI → libvolta_core.so
│   ├── ui/menu.lua    Library grid with cover images
│   ├── reader.lua     Normal reading mode
│   ├── rsvp.lua       RSVP display + timer
│   └── themes/        Built-in color themes
├── build.sh           cargo build + copy .so
├── run.sh             build + launch LÖVE
├── volta              Dual-mode launcher script
└── volta.desktop      Desktop entry
```

Deep dive: [ARCHITECTURE.md](ARCHITECTURE.md)

## Stack

| Layer | Tech |
|-------|------|
| Core engine | Rust (`rbook`, `sha2`, `serde_json`, `image`) |
| TUI | `ratatui` + `crossterm` |
| GUI | LÖVE 11.x (LuaJIT) |
| FFI bridge | LuaJIT FFI → C ABI from Rust `cdylib` |

## License

[MIT](LICENSE)

---

<p align="center">
  <sub>Built for people who actually read.</sub>
</p>
