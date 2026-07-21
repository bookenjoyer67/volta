# Volta Keybindings

## Library (Home Screen)

| Key | Action |
|-----|--------|
| `↑` / `↓` | Move up/down one row of cards |
| `←` / `→` | Move left/right one card |
| `Enter` | Open selected book |
| `Ctrl+O` | Browse for file |
| `Esc` | Quit |

## Reader Mode

| Key | Action |
|-----|--------|
| `↓` / `↑` | Move cursor down/up one line |
| `←` / `→` | Move cursor left/right one word |
| `j` | Scroll down 3 lines |
| `k` | Scroll up 3 lines |
| `Ctrl+d` | Half-page down |
| `Ctrl+u` | Half-page up |
| `Ctrl+f` | Full page down |
| `Ctrl+b` | Full page up |
| `gg` | Jump to top of chapter (tap `g` twice quickly) |
| `G` | Jump to bottom of chapter (Shift+G) |
| `n` | Next chapter *(or next search match when search is active)* |
| `p` | Previous chapter |
| `N` | Previous search match *(when search is active)* |
|| `r` | Enter RSVP speed reading **at cursor position** |
|| `t` | Cycle to next theme |
|| `T` | Cycle to previous theme |
|| `gt` | Open Table of Contents overlay (tap `g` then `t` quickly) |
|| `Ctrl+o` | Jump back to previous position (undo TOC/search jump) |
|| `/` | Start search — type your query, Enter to execute, Esc to cancel |
|| `Ctrl+S` | **Save progress** (does not auto-save!) |
|| `<` / `>` | Narrow / widen page margins |
|| `{` / `}` | Narrow / widen max column width (text centers when set; narrow past the floor to turn off) |
|| `Esc` | Clear search / Return to library |

### Paragraphs

Paragraph breaks are preserved from the source (EPUB block tags, Markdown blank lines). The first line of every paragraph is indented; continuation lines are flush left. PDF text has no paragraph structure, so PDFs render as before.

### Scrollbar

A vertical scrollbar on the right edge shows your position within the current chapter.
A small dot next to it shows your global position in the book.

## Table of Contents

Open with `gt` in reader mode. A centered overlay lists all chapter titles.

|| Key | Action |
||-----|--------|
|| `j` / `↓` | Move selection down |
|| `k` / `↑` | Move selection up |
|| `Enter` | Jump to selected chapter |
|| `Esc` | Cancel — return to reader at previous position |
|| `/` | Toggle filter — type to narrow chapters by title |
|| `gg` | Jump to top of list |
|| `G` | Jump to bottom of list |

Filtered chapters are highlighted. Press `/` again to clear the filter.

## Jump-Back Stack

Every search jump and TOC jump pushes your previous position onto a stack.

|| Key | Action |
||-----|--------|
|| `Ctrl+o` | Pop the stack and return to your previous position |

The stack holds up to 20 positions. Duplicates are not pushed.

## RSVP Mode

| Key | Action |
|-----|--------|
| `Space` | Play / Pause |
| `h` / `←` | Seek back 10 words |
| `l` / `→` | Seek forward 10 words |
| `k` / `↑` | Seek back 100 words |
| `j` / `↓` | Seek forward 100 words |
| `=` | Increase WPM (+25) |
| `-` | Decrease WPM (-25) |
| `s` | Toggle stats overlay |
| `t` | Cycle to next theme |
| `T` | Cycle to previous theme |
| `Ctrl+S` | **Save progress** |
| `Esc` | Return to reader |

## Search

| Key | Action |
|-----|--------|
| `/` | Enter search mode — type query, bottom bar shows input |
| `Enter` | Execute search — jumps to first match |
| `n` | Next match |
| `N` | Previous match |
| `Esc` | Clear search and all highlights |

Search is case-insensitive and scans every chapter in the book. Matches appear in gold text. The cursor word (hot pink) takes priority when it overlaps a match.

## Customization

Keybindings are in `frontend/input.lua`. Edit the `defaults` table to remap.
