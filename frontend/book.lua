--[[
Volta book.lua — document state management and FFI wrapper

This module is the single owner of the DocEnum pointer obtained
from rsvp_open.  All other modules (reader, rsvp, menu) call
through book.lua's methods — never touch bridge.rsvp directly.

The pointer model:
  - book:open(path)  → allocates DocEnum via rsvp_open
  - book:close()     → frees it via rsvp_close
  - All getters      → borrow the pointer (read-only, Rust side)
  - Player controls  → mutable borrow (seek/play/pause/tick)

Because DocEnum holds pre-allocated CStrings (see Rust types.rs),
word/text/title pointers returned by rsvp_word_at etc. are valid
until rsvp_close — no copying across the FFI boundary per call.
]]

local bridge = require("bridge")
local ffi = bridge.ffi  -- raw LuaJIT ffi (for ffi.C.malloc, ffi.string, etc.)

local M = {}

M.doc = nil       -- opaque DocEnum* from Rust
M.file_path = nil -- absolute path of the open file

--- Open a document by file path.
-- Returns true on success, false on failure (with a messagebox).
-- The path is normalized to absolute for consistent save/load keys.
function M:open(path)
  self:close()

  -- Normalize to absolute path for save/load consistency
  if path:sub(1, 1) == "~" then
    local home = os.getenv("HOME") or "/tmp"
    path = home .. path:sub(2)
  elseif path:sub(1, 1) ~= "/" then
    -- Prepend CWD for relative paths
    local cwd = io.popen("pwd"):read("*a"):gsub("%s+$", "") or ""
    path = cwd .. "/" .. path
  end

  -- Allocate a null-terminated C string for the path.
  local c_str = ffi.C.malloc(#path + 1)
  ffi.copy(c_str, path)
  local doc_ptr = bridge.rsvp.rsvp_open(c_str)
  ffi.C.free(c_str)

  if doc_ptr == nil then
    love.window.showMessageBox(
      "Error", "Failed to open: " .. path, "error")
    return false
  end

  self.doc = doc_ptr
  self.file_path = path

  print("Opened:", ffi.string(bridge.rsvp.rsvp_title(doc_ptr)))
  print("Words:", bridge.rsvp.rsvp_word_count(doc_ptr))

  -- Restore saved progress for this book
  local progress = require("progress")
  local saved = progress:load(path)
  if saved then
    if saved.word_index and saved.word_index > 0 then
      self:seek(saved.word_index)
    end
    if saved.wpm then
      self:set_wpm(saved.wpm)
    end
    self._saved = saved
  end

  return true
end

--- Close the current document (safe to call when nothing is open).
function M:close()
  if self.doc then
    bridge.rsvp.rsvp_close(self.doc)
    self.doc = nil
    self.file_path = nil
  end
end

--- Is a document currently loaded?
function M:is_loaded()
  return self.doc ~= nil
end

-- ── Read-only accessors ──────────────────────────────────────

function M:title()
  if not self.doc then return "" end
  return ffi.string(bridge.rsvp.rsvp_title(self.doc))
end

function M:word_count()
  if not self.doc then return 0 end
  return bridge.rsvp.rsvp_word_count(self.doc)
end

function M:word_at(i)
  if not self.doc then return "" end
  local ptr = bridge.rsvp.rsvp_word_at(self.doc, i)
  if ptr == nil then return "" end
  return ffi.string(ptr)
end

function M:chapter_at(i)
  if not self.doc then return 0 end
  return bridge.rsvp.rsvp_chapter_at(self.doc, i)
end

function M:chapter_count()
  if not self.doc then return 0 end
  return bridge.rsvp.rsvp_chapter_count(self.doc)
end

function M:chapter_title(i)
  if not self.doc then return "" end
  local ptr = bridge.rsvp.rsvp_chapter_title(self.doc, i)
  if ptr == nil then return "" end
  return ffi.string(ptr)
end

function M:chapter_text(i)
  if not self.doc then return "" end
  local ptr = bridge.rsvp.rsvp_chapter_text(self.doc, i)
  if ptr == nil then return "" end
  return ffi.string(ptr)
end

-- ── RSVP player controls ─────────────────────────────────────

function M:seek(i)
  bridge.rsvp.rsvp_seek(self.doc, i)
end

function M:play()
  bridge.rsvp.rsvp_play(self.doc)
end

function M:pause()
  bridge.rsvp.rsvp_pause(self.doc)
end

function M:is_playing()
  return bridge.rsvp.rsvp_is_playing(self.doc)
end

function M:current_index()
  return bridge.rsvp.rsvp_current_index(self.doc)
end

function M:tick(dt_ms)
  return bridge.rsvp.rsvp_tick(self.doc, dt_ms)
end

function M:set_wpm(wpm)
  bridge.rsvp.rsvp_set_wpm(self.doc, wpm)
end

--- Find the first word index of a chapter (used when entering
-- RSVP mode from reader mode to start at the right chapter).
function M:chapter_start(chapter)
  if not self.doc then return 0 end
  return tonumber(
    bridge.rsvp.rsvp_chapter_start(self.doc, chapter)
  ) or 0
end

return M
