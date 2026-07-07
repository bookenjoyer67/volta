--[[
Volta config.lua — application settings and theme management

Holds window dimensions, default WPM, the active theme, and font
resolution.  Themes are loaded on demand from frontend/themes/.

Font resolution maps family names (mono/sans/serif) to system
TTF paths and falls back to LOVE's default font if a path is
missing or fails to load.
]]

local M = {}

-- Window
M.window_width = 1024
M.window_height = 768
M.vsync = true

-- RSVP defaults
M.wpm = 300

-- Active theme name (must match a file in frontend/themes/)
M.theme_name = "neon"

-- Font family → system TTF path.
-- Edit these for your distro if fonts live elsewhere.
local FONT_MAP = {
  mono  = "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
  sans  = "/usr/share/fonts/TTF/DejaVuSans.ttf",
  serif = "/usr/share/fonts/TTF/DejaVuSerif.ttf",
}

--- Resolve a font family name to a LOVE Font object.
-- @param family "mono" | "sans" | "serif"
-- @param size   point size
-- @return love.Font (never nil — falls back to default)
function M:resolve_font(family, size)
  local path = FONT_MAP[family]
  if path then
    local ok, f = pcall(love.graphics.newFont, path, size)
    if ok then return f end
  end
  return love.graphics.newFont(size)  -- fallback: LOVE built-in
end

-- Default theme tables (used when no theme file is loaded, or as
-- a fallback before apply_theme_reader runs).
M.theme = {
  reader = {
    bg = {0, 0, 0}, text = {1, 1, 1}, heading = {1, 1, 1},
    link = {0.5, 0.5, 1}, selection = {0.3, 0.3, 0.5},
    font = "mono", font_size = 18,
  },
  rsvp = {
    bg = {0, 0, 0}, word = {1, 1, 1}, word_fade = {0.4, 0.4, 0.4},
    hud = {0.5, 0.5, 0.5}, progress = {0.5, 0.5, 1},
    font = "mono", font_size = 48,
  },
}

--- Load the named theme file and apply it to the reader background.
function M:apply_theme_reader()
  local ok, t = pcall(require, "themes." .. M.theme_name)
  if ok and t then M.theme = t end
  local r = M.theme.reader
  love.graphics.setBackgroundColor(unpack(r.bg))
end

--- Cycle to the next or previous theme in the built-in list.
-- @param direction 1 = next, -1 = previous
function M:cycle_theme(direction)
  local themes = {
    "daylight", "sepia", "night", "dusk",
    "forest", "ocean", "neon", "amber",
  }
  local idx = 1
  for i, name in ipairs(themes) do
    if name == M.theme_name then idx = i; break end
  end
  idx = idx + (direction or 1)
  if idx > #themes then idx = 1
  elseif idx < 1 then idx = #themes end
  M.theme_name = themes[idx]
  M:apply_theme_reader()
end

return M
