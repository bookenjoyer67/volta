--[[
Volta ui/menu.lua — library grid with cover images

Reads ~/.local/share/volta/library.json for book metadata and
cover paths.  Renders a scrollable card grid with cover images,
titles, authors, and progress bars.

Arrow keys navigate, Enter opens, Ctrl+O browses for files.
]]

local config = require("config")
local book = require("book")
local json = require("json")

local M = {}

-- Library entries: {path, title, author, format, chapter_count,
--   current_chapter, current_word, last_opened, cover_path}
M.entries = {}
M.selected_index = 1
M.scroll_offset = 0

-- Card dimensions
local CARD_W = 220
local CARD_H = 260
local CARD_GAP = 16
local MARGIN = 24
local COVER_H = 160

-- Loaded cover images, keyed by cover_path
M._covers = {}
M._default_cover = nil

local library_path

function M:init()
  local home = os.getenv("HOME") or "/tmp"
  library_path = home .. "/.local/share/volta/library.json"

  -- Load library entries
  local f = io.open(library_path, "r")
  if f then
    local raw = f:read("*a")
    f:close()
    if raw and raw ~= "" then
      local ok, data = pcall(json.decode, raw)
      if ok and type(data) == "table" then
        -- Convert map to ordered list (most recent first)
        -- The JSON order is insertion order, so iterate pairs
        local temp = {}
        for path, entry in pairs(data) do
          table.insert(temp, {
            path = path,
            title = entry.title or path:match("([^/]+)%.[^%.]+$") or path,
            author = entry.author or "",
            format = entry.format or "",
            chapter_count = entry.chapter_count or 0,
            current_chapter = entry.current_chapter or 0,
            current_word = entry.current_word or 0,
            last_opened = entry.last_opened or 0,
            cover_path = entry.cover_path,
          })
        end
        -- Sort by last_opened descending
        table.sort(temp, function(a, b)
          return (a.last_opened or 0) > (b.last_opened or 0)
        end)
        M.entries = temp
      end
    end
  end
end

--- Load a cover image, with caching and fallback.
function M:_get_cover(entry)
  -- Return cached image
  if M._covers[entry.cover_path] then
    return M._covers[entry.cover_path]
  end

  -- Try loading from cover_path
  if entry.cover_path then
    local ok, img = pcall(love.graphics.newImage, entry.cover_path)
    if ok and img then
      M._covers[entry.cover_path] = img
      return img
    end
  end

  -- Return format-based default
  if not M._default_cover then
    -- Create a simple colored rectangle as default cover
    M._default_cover = {}
  end
  return M._default_cover
end

--- Draw a single book card.
function M:_draw_card(x, y, entry, is_selected)
  local theme = config.theme.reader

  -- Card background
  local bg = is_selected and {0.12, 0.08, 0.16} or {0.04, 0.04, 0.06}
  love.graphics.setColor(unpack(bg))
  love.graphics.rectangle("fill", x, y, CARD_W, CARD_H, 6, 6)

  -- Card border
  local border = is_selected and theme.selection or {0.25, 0.25, 0.25}
  love.graphics.setColor(unpack(border))
  love.graphics.setLineWidth(is_selected and 2 or 1)
  love.graphics.rectangle("line", x, y, CARD_W, CARD_H, 6, 6)

  -- Cover image area
  local cover_x = x + 8
  local cover_y = y + 8
  local cover_w = CARD_W - 16
  local cover_h = COVER_H

  local img = M:_get_cover(entry)
  if type(img) == "userdata" then
    -- Actual image — draw scaled to fit
    local iw, ih = img:getWidth(), img:getHeight()
    local scale = math.min(cover_w / iw, cover_h / ih)
    local sw, sh = iw * scale, ih * scale
    local cx = cover_x + (cover_w - sw) / 2
    local cy = cover_y + (cover_h - sh) / 2
    love.graphics.setColor(1, 1, 1)
    love.graphics.draw(img, cx, cy, 0, scale, scale)
  else
    -- No cover — draw format label
    love.graphics.setColor(0.15, 0.15, 0.2)
    love.graphics.rectangle("fill", cover_x, cover_y, cover_w, cover_h, 4, 4)

    local icon = "?"
    if entry.format == "epub" then icon = "EPUB"
    elseif entry.format == "pdf" then icon = "PDF"
    elseif entry.format == "md" then icon = "MD"
    end

    love.graphics.setColor(unpack(theme.heading))
    local font = love.graphics.newFont(24)
    love.graphics.setFont(font)
    love.graphics.printf(icon, cover_x, cover_y + cover_h / 2 - 14, cover_w, "center")
  end

  -- Title
  local text_y = y + COVER_H + 16
  love.graphics.setColor(unpack(is_selected and theme.selection or theme.text))
  local font = love.graphics.newFont(13)
  love.graphics.setFont(font)
  local title = entry.title
  if #title > 24 then title = title:sub(1, 23) .. "…" end
  love.graphics.printf(title, x + 8, text_y, CARD_W - 16, "left")

  -- Author
  text_y = text_y + 18
  love.graphics.setColor(0.5, 0.5, 0.5)
  local author = entry.author
  if author ~= "" then
    if #author > 24 then author = author:sub(1, 23) .. "…" end
    love.graphics.printf(author, x + 8, text_y, CARD_W - 16, "left")
  end

  -- Progress bar
  text_y = text_y + 22
  local pct = 0
  if entry.chapter_count > 0 then
    pct = math.floor(entry.current_chapter / entry.chapter_count * 100)
  end
  local bar_w = CARD_W - 16
  local filled = math.floor(bar_w * pct / 100)

  love.graphics.setColor(0.2, 0.2, 0.2)
  love.graphics.rectangle("fill", x + 8, text_y, bar_w, 6, 3, 3)
  if filled > 0 then
    love.graphics.setColor(unpack(theme.selection))
    love.graphics.rectangle("fill", x + 8, text_y, filled, 6, 3, 3)
  end

  -- Chapter info
  text_y = text_y + 12
  love.graphics.setColor(0.5, 0.5, 0.5)
  local ch_info = ""
  if entry.chapter_count > 0 then
    ch_info = string.format("Ch %d/%d", entry.current_chapter + 1, entry.chapter_count)
  end
  local info_font = love.graphics.newFont(11)
  love.graphics.setFont(info_font)
  love.graphics.printf(ch_info, x + 8, text_y, CARD_W - 16, "left")
end

function M:draw()
  local w, h = love.graphics.getWidth(), love.graphics.getHeight()
  local theme = config.theme.reader

  love.graphics.clear(unpack(theme.bg))

  if #M.entries == 0 then
    -- Empty state
    love.graphics.setFont(love.graphics.newFont(18))
    love.graphics.setColor(unpack(theme.text))
    love.graphics.printf("No books yet.\n\nPress Ctrl+O to browse for a file.",
      0, h * 0.35, w, "center")
    love.graphics.setFont(love.graphics.newFont(12))
    love.graphics.setColor(0.5, 0.5, 0.5)
    love.graphics.printf(
      "Ctrl+O = browse  |  Ctrl+S = save  |  Esc = quit",
      0, h - 40, w, "center")
    return
  end

  -- Calculate grid
  local cols = math.max(1, math.floor((w - MARGIN * 2 + CARD_GAP) / (CARD_W + CARD_GAP)))
  local rows_per_page = math.max(1, math.floor((h - 80) / (CARD_H + CARD_GAP)))

  -- Draw cards
  local items_drawn = 0
  for i = 0, #M.entries - 1 do
    local idx = i + 1
    -- Skip before scroll
    if idx <= M.scroll_offset * cols then
      -- nada
    else
      local visible_idx = idx - M.scroll_offset * cols - 1
      local col = visible_idx % cols
      local row = math.floor(visible_idx / cols)
      local card_x = MARGIN + col * (CARD_W + CARD_GAP)
      local card_y = 50 + row * (CARD_H + CARD_GAP)

      if card_y + CARD_H > h - 50 then
        -- Off-screen: stop rendering
      else
        local entry = M.entries[idx]
        local is_selected = (idx == M.selected_index)
        M:_draw_card(card_x, card_y, entry, is_selected)
        items_drawn = items_drawn + 1
      end
    end
  end

  -- Instructions footer
  love.graphics.setFont(love.graphics.newFont(12))
  love.graphics.setColor(0.5, 0.5, 0.5)
  love.graphics.printf(
    "Enter = open  |  Ctrl+O = browse  |  Ctrl+S = save  |  Esc = quit",
    0, h - 40, w, "center")
end

function M:keypressed(key, scancode, isrepeat)
  local w = love.graphics.getWidth()
  local cols = math.max(1, math.floor((w - MARGIN * 2 + CARD_GAP) / (CARD_W + CARD_GAP)))

  if key == "escape" then
    love.event.quit()
  elseif key == "up" then
    M.selected_index = math.max(1, M.selected_index - cols)
  elseif key == "down" then
    M.selected_index = math.min(#M.entries, M.selected_index + cols)
  elseif key == "left" then
    M.selected_index = math.max(1, M.selected_index - 1)
  elseif key == "right" then
    M.selected_index = math.min(#M.entries, M.selected_index + 1)
  elseif key == "return" then
    M:_open_selected()
  elseif key == "o"
    and (love.keyboard.isDown("lctrl") or love.keyboard.isDown("rctrl")) then
    M:_browse_file()
  end
end

function M:_open_selected()
  local entry = M.entries[M.selected_index]
  if not entry then return end

  local ok = book:open(entry.path)
  if ok then
    -- Update library entry's last_opened
    entry.last_opened = os.time()
    require("reader.reader"):enter()
    set_mode("reader")
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
        -- Add to library (the Rust side will handle persistence)
        require("reader.reader"):enter()
        set_mode("reader")
      end
    end
  end
end

function M:mousepressed(x, y, button, istouch, presses)
  local w = love.graphics.getWidth()
  local cols = math.max(1, math.floor((w - MARGIN * 2 + CARD_GAP) / (CARD_W + CARD_GAP)))

  for i = 1, #M.entries do
    local visible_idx = i - M.scroll_offset * cols - 1
    if visible_idx >= 0 then
      local col = visible_idx % cols
      local row = math.floor(visible_idx / cols)
      local card_x = MARGIN + col * (CARD_W + CARD_GAP)
      local card_y = 50 + row * (CARD_H + CARD_GAP)

      if x >= card_x and x <= card_x + CARD_W
        and y >= card_y and y <= card_y + CARD_H then
        M.selected_index = i
        M:_open_selected()
        return
      end
    end
  end
end

return M
