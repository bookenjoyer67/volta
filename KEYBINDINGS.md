# Volta Keybindings

## Reader Mode

| Key | Action |
|-----|--------|
| `↓` / `↑` | Move cursor down/up one line |
| `←` / `→` | Move cursor left/right one word |
| `j` / `k` | Scroll down/up 3 lines |
| `Ctrl+d` / `Ctrl+u` | Half-page down/up (vim-style) |
| `Ctrl+f` / `Ctrl+b` | Page down/up (vim-style) |
| `Space` / `Backspace` | Page down/up |
| `gg` | Jump to top of chapter (double-tap g) |
| `G` | Jump to bottom of chapter (shift+g) |
| `n` | Next chapter |
| `p` | Previous chapter |
| `r` | Enter RSVP speed reading **at cursor position** |
| `Ctrl+S` | **Manual save progress** |
| `Esc` | Back to menu |
| `Mouse wheel` | Scroll text |

## RSVP Mode

| Key | Action |
|-----|--------|
| `Space` | Play / Pause |
| `h` / `←` | Seek back 10 words |
| `l` / `→` | Seek forward 10 words |
| `k` / `↓` | Seek back 100 words |
| `j` / `↑` | Seek forward 100 words |
| `=` | Increase WPM (+25) |
| `-` | Decrease WPM (-25) |
| `s` | Toggle stats overlay |
| `Ctrl+S` | **Manual save progress** |
| `Esc` | Exit RSVP, return to reader |

## Menu

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate items |
| `Enter` | Open selected (first item = continue reading) |
| `Ctrl+O` | Open file picker |
| `Esc` | Quit |
| `Drag & drop` | Open EPUB/PDF file |

## Global

| Key | Action |
|-----|--------|
| `/` | Toggle help overlay |

## Customization

Keybindings are in `frontend/input.lua`. Edit the `defaults` table to remap.
