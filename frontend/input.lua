--[[
Volta input.lua — keybinding map

All key names are LOVE key constants (the `key` argument passed
to love.keypressed).  Edit the `defaults` table to remap any
binding; no other files need to change.

See KEYBINDINGS.md in the project root for the human-readable
reference.
]]

local M = {}

M.defaults = {
  -- Reader mode
  reader_scroll_down = "j",
  reader_scroll_up = "k",
  reader_page_down = "space",
  reader_page_up = "backspace",
  reader_next_chapter = "n",
  reader_prev_chapter = "p",
  reader_toggle_rsvp = "r",
  reader_escape = "escape",
  reader_zoom_in = "kp+",
  reader_zoom_out = "kp-",

  -- Vim-style page navigation
  reader_half_page_down = "d",
  reader_half_page_up = "u",
  reader_full_page_down = "f",
  reader_full_page_up = "b",
  reader_chapter_top = "g",
  reader_chapter_bottom = "g",

  -- Theme cycling
  reader_cycle_theme = "t",
  reader_cycle_theme_rev = "T",

  -- Cursor movement (arrow keys)
  reader_cursor_up = "up",
  reader_cursor_down = "down",
  reader_cursor_left = "left",
  reader_cursor_right = "right",

  -- RSVP mode
  rsvp_play_pause = "space",
  rsvp_seek_back_10 = "left",
  rsvp_seek_forward_10 = "right",
  rsvp_seek_back_100 = "down",
  rsvp_seek_forward_100 = "up",
  rsvp_speed_up = "=",
  rsvp_speed_down = "-",
  rsvp_exit = "escape",
  rsvp_toggle_stats = "s",
  rsvp_font_up = "]",
  rsvp_font_down = "[",

  -- Vim-style hjkl seeking
  rsvp_seek_back_10_vim = "h",
  rsvp_seek_forward_10_vim = "l",
  rsvp_seek_back_100_vim = "k",
  rsvp_seek_forward_100_vim = "j",

  -- Global
  toggle_help = "/",
}

--- Initialise the runtime binding table (copied from defaults).
-- Called once at startup so that in the future we can add
-- user-config overrides without touching this file.
function M:init()
  self.bindings = {}
  for k, v in pairs(M.defaults) do
    self.bindings[k] = v
  end
end

--- Look up the current key for an action.
-- Falls back to the default if the runtime table is uninitialised.
function M:get(action)
  return self.bindings[action] or M.defaults[action]
end

return M
