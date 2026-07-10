--[[
Volta ui/menu.lua — library grid with cover images

Reads ~/.local/share/volta/library.json for book metadata and
cover paths.  Renders a card grid with cover images, titles,
authors, and progress bars.

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

-- Card dimensions
local CARD_W = 220
local CARD_H = 260
local CARD_GAP = 16
local MARGIN = 24
local COVER_H = 160

-- Loaded cover images, keyed by cover_path
M._covers = {}
M._default_cover = {}

function M:init()
  local home = os.getenv("HOME") or "/tmp"
  local library_path = home .. "/.local/share/volta/library.json"

  -- Load library entries
  local f = io.open(library_path, "r")
  if f then
    local raw = f:read("*a")
    f:close()
    if raw and raw ~= "" then
      local ok, data = pcall(json.decode, raw)
      if ok and type(data) == "table" then
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
        table.sort(temp, function(a, b)
          return (a.last_opened or 0) > (b.last_opened or 0)
        end)
        M.entries = temp
      end
    end
  end

  -- Pre-load cover images (GPU textures must be created in love.load)
  -- LOVE sandboxes love.graphics.newImage, so we load raw bytes
  -- via io.open and create ImageData manually.
  for _, entry in ipairs(M.entries) do
    if entry.cover_path and not M._covers[entry.cover_path] then
      local f = io.open(entry.cover_path, "rb")
      if f then
        local data = f:read("*a")
        f:close()
        -- newImageData(string) treats string as filename, not raw bytes.
        -- Need FileData → ImageData → Image chain.
        local fd = love.filesystem.newFileData(data, "cover.png")
        local ok, img = pcall(function()
          local id = love.image.newImageData(fd)
          return love.graphics.newImage(id)
        end)
        if ok and img then
          M._covers[entry.cover_path] = img
          print("Cover loaded: " .. entry.title)
        else
          print("Cover FAILED: " .. entry.cover_path .. " — " .. tostring(img))
        end
      else
        print("Cover file not readable: " .. entry.cover_path)
      end
    end
  end
end

-- — Draw ──

function M:_draw_card(x, y, entry, selected)
  local theme = config.theme.reader

  -- Card background
  local bg = selected and {0.12, 0.08, 0.16} or {0.04, 0.04, 0.06}
  love.graphics.setColor(unpack(bg))
  love.graphics.rectangle("fill", x, y, CARD_W, CARD_H, 6, 6)

  -- Card border
  local border = selected and theme.selection or {0.25, 0.25, 0.25}
  love.graphics.setColor(unpack(border))
  love.graphics.setLineWidth(selected and 2 or 1)
  love.graphics.rectangle("line", x, y, CARD_W, CARD_H, 6, 6)

  -- Cover image area
  local cx, cy, cw, ch = x + 8, y + 8, CARD_W - 16, COVER_H
  local img = M._covers[entry.cover_path]

  if img and type(img) == "userdata" then
    local iw, ih = img:getWidth(), img:getHeight()
    local scale = math.min(cw / iw, ch / ih)
    local sw, sh = iw * scale, ih * scale
    local dx = cx + (cw - sw) / 2
    local dy = cy + (ch - sh) / 2
    love.graphics.setColor(1, 1, 1)
    love.graphics.draw(img, dx, dy, 0, scale, scale)
  else
    -- Placeholder label
    love.graphics.setColor(0.15, 0.15, 0.2)
    love.graphics.rectangle("fill", cx, cy, cw, ch, 4, 4)

    local lbl = "?"
    if entry.format == "epub" then lbl = "EPUB"
    elseif entry.format == "pdf" then lbl = "PDF"
    elseif entry.format == "md" then lbl = "MD"
    end

    love.graphics.setColor(unpack(theme.heading))
    love.graphics.setFont(love.graphics.newFont(24))
    love.graphics.printf(lbl, cx, cy + ch / 2 - 14, cw, "center")
  end

  -- Title
  local ty = y + COVER_H + 18
  love.graphics.setColor(unpack(selected and theme.selection or theme.text))
  love.graphics.setFont(love.graphics.newFont(13))
  local title = entry.title
  if #title > 24 then title = title:sub(1, 23) .. "…" end
  love.graphics.printf(title, x + 8, ty, CARD_W - 16, "left")

  -- Author
  ty = ty + 18
  if entry.author ~= "" then
    love.graphics.setColor(0.5, 0.5, 0.5)
    local author = entry.author
    if #author > 24 then author = author:sub(1, 23) .. "…" end
    love.graphics.printf(author, x + 8, ty, CARD_W - 16, "left")
  end

  -- Progress bar
  ty = ty + 22
  local pct = 0
  if entry.chapter_count > 0 then
    pct = math.floor(entry.current_chapter / entry.chapter_count * 100)
  end
  local bw = CARD_W - 16
  local filled = math.floor(bw * pct / 100)
  love.graphics.setColor(0.2, 0.2, 0.2)
  love.graphics.rectangle("fill", x + 8, ty, bw, 6, 3, 3)
  if filled > 0 then
    love.graphics.setColor(unpack(theme.selection))
    love.graphics.rectangle("fill", x + 8, ty, filled, 6, 3, 3)
  end

  -- Chapter info
  ty = ty + 12
  love.graphics.setColor(0.5, 0.5, 0.5)
  love.graphics.setFont(love.graphics.newFont(11))
  local ch = ""
  if entry.chapter_count > 0 then
    ch = string.format("Ch %d/%d", entry.current_chapter + 1, entry.chapter_count)
  end
  love.graphics.printf(ch, x + 8, ty, CARD_W - 16, "left")
end

function M:draw()
  local w, h = love.graphics.getWidth(), love.graphics.getHeight()
  local theme = config.theme.reader

  love.graphics.clear(unpack(theme.bg))

  if #M.entries == 0 then
    love.graphics.setFont(love.graphics.newFont(18))
    love.graphics.setColor(unpack(theme.text))
    love.graphics.printf("No books yet.\n\nPress Ctrl+O to browse for a file.",
      0, h * 0.35, w, "center")
  else
    local cols = math.max(1, math.floor((w - MARGIN * 2 + CARD_GAP) / (CARD_W + CARD_GAP)))
    for i, entry in ipairs(M.entries) do
      local vi = i - 1
      local col = vi % cols
      local row = math.floor(vi / cols)
      local cx = MARGIN + col * (CARD_W + CARD_GAP)
      local cy = 50 + row * (CARD_H + CARD_GAP)
      if cy + CARD_H <= h - 50 then
        M:_draw_card(cx, cy, entry, i == M.selected_index)
      end
    end
  end

  -- Footer
  love.graphics.setFont(love.graphics.newFont(12))
  love.graphics.setColor(0.5, 0.5, 0.5)
  love.graphics.printf(
    "Enter = open  |  Ctrl+O = browse  |  Ctrl+S = save  |  Esc = quit",
    0, h - 40, w, "center")
end

-- ── Input ──

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
    entry.last_opened = os.time()
    require("reader.reader"):enter()
    set_mode("reader")
  end
end

function M:_browse_file()
  local handle = io.popen("zenity --file-selection --title='Open Book' 2>/dev/null")
  if not handle then return end
  local path = handle:read("*a")
  handle:close()
  if path and path ~= "" then
    path = path:gsub("%s+$", "")
    if path ~= "" and book:open(path) then
      require("reader.reader"):enter()
      set_mode("reader")
    end
  end
end

function M:mousepressed(x, y, button, istouch, presses)
  local w = love.graphics.getWidth()
  local cols = math.max(1, math.floor((w - MARGIN * 2 + CARD_GAP) / (CARD_W + CARD_GAP)))
  for i = 1, #M.entries do
    local vi = i - 1
    local col = vi % cols
    local row = math.floor(vi / cols)
    local cx = MARGIN + col * (CARD_W + CARD_GAP)
    local cy = 50 + row * (CARD_H + CARD_GAP)
    if x >= cx and x <= cx + CARD_W and y >= cy and y <= cy + CARD_H then
      M.selected_index = i
      M:_open_selected()
      return
    end
  end
end

return M
