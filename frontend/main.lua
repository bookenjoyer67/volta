--[[
Volta main.lua — LOVE game loop entry point

State machine with three modes:
  menu   — file open dialog, recent files, drag-and-drop
  reader — normal reading with reflowed text, scroll, chapters
  rsvp   — Rapid Serial Visual Presentation speed reading

Global functions set_mode() and get_mode() allow mode switching
from within UI modules (e.g. reader → rsvp when 'r' is pressed).

When launched with a file path argument (love frontend/ book.epub),
the app skips the menu and opens the book directly in reader mode.
]]

local config = require("config")
local input = require("input")
local book = require("book")
local progress = require("progress")

local mode = "menu"  -- menu | reader | rsvp

-- ── LOVE callbacks ────────────────────────────────────────────

function love.load(args)
  -- UTF-8 polyfill (LuaJIT doesn't ship utf8)
  utf8 = require("utf8")

  -- Force a stable save identity so recent.txt and progress.json
  -- always land in the same directory.
  love.filesystem.setIdentity("volta")
  -- Ensure save directory exists (setIdentity doesn't always create it)
  love.filesystem.createDirectory("")

  love.window.setTitle("Volta")
  love.window.setMode(
    config.window_width, config.window_height,
    { resizable = true, vsync = config.vsync }
  )
  love.graphics.setBackgroundColor(unpack(config.theme.reader.bg))
  love.keyboard.setKeyRepeat(true)

  -- Initialise keybinding runtime table
  input:init()

  -- Load saved reading progress
  progress:init()

  -- Initialise menu (recent files list)
  require("ui.menu"):init()

  -- Auto-open if a file path was passed on the command line
  if args[1] then
    book:open(args[1])
    if book:is_loaded() then
      mode = "reader"
      require("reader.reader"):enter()
    end
  end

  -- Load the active theme
  config:apply_theme_reader()
end

--- Called every frame.  Only RSVP mode needs frame timing.
function love.update(dt)
  if mode == "rsvp" then
    require("rsvp.rsvp"):update(dt)
  end
end

--- Render the current mode.
function love.draw()
  if mode == "menu" then
    require("ui.menu"):draw()
  elseif mode == "reader" then
    require("reader.reader"):draw()
  elseif mode == "rsvp" then
    require("rsvp.rsvp"):draw()
  end
end

--- Dispatch keypresses to the active mode's handler.
function love.keypressed(key, scancode, isrepeat)
  if mode == "menu" then
    require("ui.menu"):keypressed(key, scancode, isrepeat)
  elseif mode == "reader" then
    require("reader.reader"):keypressed(key, scancode, isrepeat)
  elseif mode == "rsvp" then
    require("rsvp.rsvp"):keypressed(key, scancode, isrepeat)
  end
end

--- Text input (only RSVP mode uses this for numeric WPM entry).
function love.textinput(t)
  if mode == "rsvp" then
    require("rsvp.rsvp"):textinput(t)
  end
end

function love.mousepressed(x, y, button, istouch, presses)
  if mode == "menu" then
    require("ui.menu"):mousepressed(x, y, button, istouch, presses)
  end
end

function love.wheelmoved(x, y)
  if mode == "reader" then
    require("reader.reader"):wheelmoved(x, y)
  end
end

function love.resize(w, h)
  config.window_width = w
  config.window_height = h
end

-- ── Drag-and-drop ─────────────────────────────────────────────

function love.filedropped(file)
  local path = file:getFilename()
  if path then
    book:open(path)
    if book:is_loaded() then
      mode = "reader"
      require("reader.reader"):enter()
    end
  end
end

-- ── Mode switching (called from UI modules) ───────────────────

function set_mode(new_mode)
  mode = new_mode
end

function get_mode()
  return mode
end

--- Save progress on quit.
function love.quit()
  -- Save reader position if a book is open
  if book:is_loaded() then
    local reader = require("reader.reader")
    local entry = {
      chapter = reader.current_chapter,
      scroll_y = reader.scroll_y,
      cursor_word = reader.cursor_word,
      word_index = book:current_index(),
      wpm = config.wpm,
    }
    progress:save(book.file_path, entry)
  end
  progress:flush()
end
