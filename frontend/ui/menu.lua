--[[
Volta ui/menu.lua — welcome screen

Shows:
  - "Continue reading" at the top if the last book has saved progress
  - Recent files list (up to 10)
  - Instructions footer

Enter opens the selected item.  Ctrl+O spawns a native file picker.
]]

local config = require("config")
local book = require("book")
local progress = require("progress")

local M = {}

M.recent_files = {}
M.selected_index = 1
M._last_book = nil
M._has_continue = false

-- Save paths (consistent with progress.lua)
local home = os.getenv("HOME") or "/tmp"
M._save_dir = home .. "/.local/share/volta"
M._recent_file = M._save_dir .. "/recent.txt"

function M:init()
  -- Ensure save directory
  os.execute("mkdir -p '" .. M._save_dir:gsub("'", "'\\''") .. "'")

  -- Load recent files list
  local f = io.open(M._recent_file, "r")
  if f then
    local content = f:read("*a")
    f:close()
    if content then
      for line in content:gmatch("[^\n]+") do
        if line ~= "" then
          table.insert(M.recent_files, line)
        end
      end
    end
  end

  -- Load saved progress to find the "continue reading" book
  M:_refresh_continue()
end

--- Called on every menu draw — refreshes "continue" state.
function M:_refresh_continue()
  M._last_book = nil
  M._has_continue = false
  if #M.recent_files == 0 then return end

  local last_path = M.recent_files[1]
  local saved = progress:load(last_path)
  if not saved or not saved.word_index then return end

  -- Get title from the progress entry or path
  local title = last_path:match("([^/]+)%.[^%.]+$") or last_path
  local total_words = saved.word_index -- we need the doc loaded for full count
  -- Use a placeholder — we'll show chapter info if available, else just path

  M._last_book = {
    path = last_path,
    title = title,
    chapter = saved.chapter,
    word_index = saved.word_index,
  }
  M._has_continue = true
end

function M:_save_recent()
  local text = table.concat(M.recent_files, "\n")
  local f = io.open(M._recent_file, "w")
  if f then f:write(text); f:close() end
end

function M:_add_recent(path)
  for i, f in ipairs(M.recent_files) do
    if f == path then
      table.remove(M.recent_files, i)
      break
    end
  end
  table.insert(M.recent_files, 1, path)
  if #M.recent_files > 20 then
    table.remove(M.recent_files)
  end
  M:_save_recent()
end

function M:draw()
  local w, h = love.graphics.getWidth(), love.graphics.getHeight()
  local theme = config.theme.reader

  love.graphics.clear(unpack(theme.bg))

  -- Title
  local title_font = love.graphics.newFont(48)
  love.graphics.setFont(title_font)
  love.graphics.setColor(unpack(theme.heading))
  love.graphics.printf("Volta", 0, h * 0.12, w, "center")

  -- Subtitle
  love.graphics.setFont(love.graphics.newFont(16))
  love.graphics.setColor(unpack(theme.text))
  love.graphics.printf("EPUB & PDF reader with RSVP speed reading",
    0, h * 0.19, w, "center")

  local y = h * 0.30

  -- "Continue reading" — always index 1, pre-selected
  if M._has_continue then
    love.graphics.setFont(love.graphics.newFont(11))
    love.graphics.setColor(unpack(theme.heading))
    love.graphics.print("Continue reading:", 40, y - 8)

    love.graphics.setFont(love.graphics.newFont(16))
    local color = (M.selected_index == 1) and theme.selection or theme.link
    love.graphics.setColor(unpack(color))
    local label = "> " .. M._last_book.title
    if M._last_book.chapter then
      label = label .. "  (Ch. " .. (M._last_book.chapter + 1) .. ")"
    end
    love.graphics.print(label, 60, y + 10)
    y = y + 45
  end

  -- Recent files header
  love.graphics.setFont(love.graphics.newFont(14))
  love.graphics.setColor(unpack(theme.heading))
  love.graphics.print("Recent:", 40, y)
  y = y + 25

  -- Recent files list
  love.graphics.setFont(love.graphics.newFont(14))
  local offset = M._has_continue and 2 or 1  -- skip index 1 if "continue" exists
  for i, path in ipairs(M.recent_files) do
    local display_idx = i + offset - 1
    local filename = path:match("([^/]+)$") or path
    local color = (display_idx == M.selected_index) and theme.selection or theme.text
    love.graphics.setColor(unpack(color))

    -- Check if this file has saved progress
    local saved = progress:load(path)
    local extra = ""
    if saved and saved.chapter then
      extra = string.format("  (Ch. %d)", saved.chapter + 1)
    end

    if M._has_continue and i == 1 then
      love.graphics.print(string.format("  %d. %s%s", i + 1, filename, extra), 60, y)
    else
      love.graphics.print(string.format("  %d. %s%s", display_idx, filename, extra), 60, y)
    end
    y = y + 22
    if display_idx >= 10 then break end
  end

  -- Instructions
  love.graphics.setFont(love.graphics.newFont(12))
  love.graphics.setColor(0.5, 0.5, 0.5)
  love.graphics.printf(
    "Enter = open  |  Ctrl+O = browse  |  Ctrl+S = save  |  Esc = quit",
    0, h - 40, w, "center")
end

function M:keypressed(key, scancode, isrepeat)
  local max_idx = math.min(10, #M.recent_files + (M._has_continue and 1 or 0))

  if key == "escape" then
    love.event.quit()
  elseif key == "up" then
    M.selected_index = math.max(1, M.selected_index - 1)
  elseif key == "down" then
    M.selected_index = math.min(max_idx, M.selected_index + 1)
  elseif key == "return" then
    M:_open_selected()
  elseif key == "o"
    and (love.keyboard.isDown("lctrl") or love.keyboard.isDown("rctrl")) then
    M:_browse_file()
  end
end

--- Open whichever item is currently selected.
function M:_open_selected()
  local path

  if M._has_continue and M.selected_index == 1 then
    path = M._last_book.path
  else
    local offset = M._has_continue and 2 or 1
    local idx = M.selected_index - offset + 1
    path = M.recent_files[idx]
  end

  if path then
    local ok = book:open(path)
    if ok then
      M:_add_recent(path)
      require("reader.reader"):enter()
      set_mode("reader")
    end
  end
end

function M:_browse_file()
  local cmd = "zenity --file-selection --title='Open Book' 2>/dev/null"
  local handle = io.popen(cmd)
  if not handle then
    love.window.showMessageBox("Error",
      "Could not open file picker. Install zenity.", "error")
    return
  end

  local path = handle:read("*a")
  handle:close()

  if path and path ~= "" then
    path = path:gsub("%s+$", "")
    if path ~= "" then
      local ok = book:open(path)
      if ok then
        M:_add_recent(path)
        require("reader.reader"):enter()
        set_mode("reader")
      end
    end
  end
end

function M:mousepressed(x, y, button, istouch, presses)
  local item_h = 22
  local start_y = love.graphics.getHeight() * 0.30
  if M._has_continue then
    -- "Continue reading" has bigger hitbox (45px)
    if y >= start_y and y < start_y + 45 then
      M.selected_index = 1
      M:_open_selected()
      return
    end
    start_y = start_y + 45
  end
  start_y = start_y + 25 -- "Recent:" header

  for i = 1, math.min(10, #M.recent_files) do
    local iy = start_y + (i - 1) * item_h
    if y >= iy and y < iy + item_h then
      local display_idx = M._has_continue and (i + 1) or i
      M.selected_index = display_idx
      M:_open_selected()
      return
    end
  end
end

return M
