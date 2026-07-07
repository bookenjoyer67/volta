--[[
Volta bridge.lua — LuaJIT FFI glue to libvolta_core.so

This module:
  1. Declares all C function signatures via ffi.cdef
  2. Loads the shared library from the frontend directory
  3. Exposes bridge.rsvp (raw C function table) and bridge.ffi
     (the LuaJIT ffi module itself, for ffi.C.malloc/ffi.string etc.)

Why a separate module instead of embedding cdefs in book.lua?
  The require("ffi") call here loads LuaJIT's built-in FFI module
  on the first call.  If we named this file ffi.lua, it would shadow
  the built-in on subsequent requires — hence the rename to bridge.lua.
]]

local ffi = require("ffi")

-- cdef: all C declarations exposed to LuaJIT FFI
-- Lua comments (--) are NOT valid inside cdef strings — use /* */ or omit.
ffi.cdef [[
  void* malloc(size_t size);
  void  free(void* ptr);
  void* memcpy(void* dest, const void* src, size_t n);

  typedef struct DocEnum DocEnum;

  /* Document lifecycle */
  DocEnum*    rsvp_open(const char* path);
  void        rsvp_close(DocEnum* doc);

  /* Metadata */
  const char* rsvp_title(DocEnum* doc);
  uint32_t    rsvp_word_count(DocEnum* doc);

  /* Word access */
  const char* rsvp_word_at(DocEnum* doc, uint32_t i);
  uint32_t    rsvp_chapter_at(DocEnum* doc, uint32_t i);

  /* Chapter access */
  uint32_t    rsvp_chapter_count(DocEnum* doc);
  const char* rsvp_chapter_title(DocEnum* doc, uint32_t c);
  const char* rsvp_chapter_text(DocEnum* doc, uint32_t c);

  /* Page rendering (PDF only) */
  const char* rsvp_render_page(DocEnum* doc, uint32_t page, uint32_t dpi);

  /* Player state */
  void        rsvp_seek(DocEnum* doc, uint32_t i);
  void        rsvp_set_wpm(DocEnum* doc, uint32_t wpm);
  void        rsvp_play(DocEnum* doc);
  void        rsvp_pause(DocEnum* doc);
  bool        rsvp_is_playing(DocEnum* doc);
  uint32_t    rsvp_current_index(DocEnum* doc);
  uint32_t    rsvp_tick(DocEnum* doc, double dt_ms);

  /* Chapter start lookup */
  uint32_t    rsvp_chapter_start(DocEnum* doc, uint32_t chapter);
]]

-- Expose the raw LuaJIT ffi so book.lua can call ffi.C.malloc etc.
local M = { ffi = ffi }

-- Load the shared library from a known absolute path.
-- In production we'd resolve this relative to the game directory;
-- for development the hardcoded path is fine.
local lib_path = "/home/computing/volta/frontend/libvolta_core.so"

local ok, lib = pcall(function()
  return ffi.load(lib_path)
end)

if ok then
  M.rsvp = lib    -- bridge.rsvp.rsvp_open(...), bridge.rsvp.rsvp_seek(...) etc.
  M.loaded = true
else
  M.rsvp = nil
  M.loaded = false
  M.load_error = lib
end

return M
