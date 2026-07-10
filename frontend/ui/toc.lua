--[[
Volta ui/toc.lua — Table of Contents overlay

Opened from reader mode with gt chord. Lists all chapter titles
in a centered overlay. j/k navigate, Enter jump, Esc cancel,
/ toggles filter. gg/G for top/bottom.
]]

local book = require("book")
local config = require("config")

local M = {}

-- State
M.chapters = {}        -- {{title, title, ...}, index = chapter_idx}
M.filtered = {}        -- indices into chapters matching filter
M.selected = 1         -- 1-indexed within filtered
M.scroll = 0           -- pixel offset
M.filter = ""
M.filter_active = false
M.source_chapter = 0
M.source_cursor = 0
M._gg_timer = 0        -- for gg chord detection
M._item_h = 28         -- row height
M._filter_h = 36       -- filter bar height

function M:enter(chapter, cursor)
  M.chapters = {}
  local count = book:chapter_count()
  for ch = 0, count - 1 do
    local title = book:chapter_title(ch)
    if title == "" then
      title = "Chapter " .. (ch + 1)
    end
    M.chapters[ch + 1] = title
  end

  M:_apply_filter()

  -- Select current chapter
  for i, ci in ipairs(M.filtered) do
    if ci - 1 == chapter then
      M.selected = i
      break
    end
  end

  M.scroll = 0
  M.filter = ""
  M.filter_active = false
  M.source_chapter = chapter
  M.source_cursor = cursor
  M._gg_timer = 0
end

function M:_apply_filter()
  M.filtered = {}
  local q = M.filter:lower()
  for i = 1, #M.chapters do
    if q == "" or M.chapters[i]:lower():find(q, 1, true) then
      table.insert(M.filtered, i)
    end
  end
  M.selected = math.min(M.selected, math.max(1, #M.filtered))
end

function M:draw()
  local w, h = love.graphics.getWidth(), love.graphics.getHeight()
  local theme = config.theme.reader

  -- Dimming backdrop
  love.graphics.setColor(0, 0, 0, 0.65)
  love.graphics.rectangle("fill", 0, 0, w, h)

  -- Overlay box: 65% x 72% centered
  local box_w = w * 0.65
  local box_h = h * 0.72
  local box_x = (w - box_w) / 2
  local box_y = (h - box_h) / 2

  -- Box background
  love.graphics.setColor(0.04, 0.04, 0.07, 0.95)
  love.graphics.rectangle("fill", box_x, box_y, box_w, box_h, 8, 8)

  -- Border
  love.graphics.setColor(unpack(theme.selection))
  love.graphics.setLineWidth(2)
  love.graphics.rectangle("line", box_x, box_y, box_w, box_h, 8, 8)

  -- Title bar
  local title = string.format("Table of Contents  —  %d/%d chapters",
    #M.filtered, #M.chapters)
  if M.filter ~= "" then
    title = title .. string.format(" (filter: \"%s\")", M.filter)
  end
  love.graphics.setFont(love.graphics.newFont(15))
  love.graphics.setColor(unpack(theme.heading))
  love.graphics.print(title, box_x + 12, box_y + 8)

  -- Separator
  love.graphics.setColor(0.15, 0.15, 0.23)
  love.graphics.rectangle("fill", box_x + 8, box_y + 32, box_w - 16, 1)

  -- List area
  local list_top = box_y + 40
  local list_bottom = box_y + box_h - (M.filter_active and M._filter_h + 36 or 28)
  local visible_h = list_bottom - list_top
  local visible_count = math.floor(visible_h / M._item_h)
  local max_scroll = math.max(0, (#M.filtered - visible_count) * M._item_h)
  M.scroll = math.min(M.scroll, max_scroll)

  -- Ensure selected is visible
  local sel_y = (M.selected - 1) * M._item_h
  if sel_y < M.scroll then
    M.scroll = sel_y
  elseif sel_y > M.scroll + visible_h - M._item_h then
    M.scroll = sel_y - visible_h + M._item_h
  end

  if #M.filtered == 0 then
    love.graphics.setFont(love.graphics.newFont(16))
    love.graphics.setColor(0.5, 0.5, 0.5)
    love.graphics.print("(no matching chapters)", box_x + 20, list_top + 20)
  else
    love.graphics.setFont(love.graphics.newFont(14))
    for vi = 0, visible_count - 1 do
      local fi = math.floor(M.scroll / M._item_h) + vi + 1
      if fi > #M.filtered then break end
      local ci = M.filtered[fi]
      local is_sel = (fi == M.selected)
      local is_current = (ci - 1 == M.source_chapter)

      local prefix = is_sel and "\u{25B6}" or " "
      local text = string.format("%s %3d. %s", prefix, ci, M.chapters[ci])

      -- Truncate
      local font = love.graphics.newFont(14)
      if font:getWidth(text) > box_w - 40 then
        while font:getWidth(text .. "\u{2026}") > box_w - 40 and #text > 10 do
          text = text:sub(1, -2)
        end
        text = text .. "\u{2026}"
      end

      local row_y = list_top + vi * M._item_h

      -- Highlight background for selected row
      if is_sel then
        love.graphics.setColor(0.12, 0.08, 0.18)
        love.graphics.rectangle("fill", box_x + 8, row_y, box_w - 16, M._item_h, 3, 3)
        love.graphics.setColor(unpack(theme.selection))
      elseif is_current then
        love.graphics.setColor(0.39, 0.39, 0.63)
      else
        love.graphics.setColor(unpack(theme.text))
      end

      love.graphics.print(text, box_x + 20, row_y + 6)
    end
  end

  -- Filter bar
  if M.filter_active then
    local fy = box_y + box_h - M._filter_h - 12
    love.graphics.setColor(0.06, 0.06, 0.1)
    love.graphics.rectangle("fill", box_x + 12, fy, box_w - 24, M._filter_h, 4, 4)
    love.graphics.setColor(unpack(theme.selection))
    love.graphics.setFont(love.graphics.newFont(16))
    love.graphics.print("/" .. M.filter, box_x + 18, fy + 8)
  end

  -- Footer
  local footer = " j/k: move  Enter: jump  /: filter  gg/G: top/bottom  Esc: cancel "
  love.graphics.setFont(love.graphics.newFont(11))
  love.graphics.setColor(unpack(theme.hud))
  local fw = love.graphics.newFont(11):getWidth(footer)
  love.graphics.print(footer, box_x + (box_w - fw) / 2, box_y + box_h - 18)
end

function M:keypressed(key)
  -- Filter mode
  if M.filter_active then
    if key == "escape" then
      M.filter_active = false
    elseif key == "backspace" then
      M.filter = M.filter:sub(1, -2)
      M:_apply_filter()
    elseif key == "return" then
      M.filter_active = false
    end
    return
  end

  -- Clear gg timer on non-g keys
  if key ~= "g" then
    M._gg_timer = 0
  end

  if key == "escape" then
    set_mode("reader")
  elseif key == "return" then
    M:_select()
  elseif key == "j" or key == "down" then
    M.selected = math.min(M.selected + 1, #M.filtered)
  elseif key == "k" or key == "up" then
    M.selected = math.max(1, M.selected - 1)
  elseif key == "/" then
    M.filter_active = not M.filter_active
    if not M.filter_active then
      M.filter = ""
      M:_apply_filter()
    end
  elseif key == "g" then
    local now = love.timer.getTime()
    if M._gg_timer > 0 and (now - M._gg_timer) < 0.3 then
      M.selected = 1
      M.scroll = 0
      M._gg_timer = 0
    else
      M._gg_timer = now
    end
  elseif key == "G" or key == "g" and love.keyboard.isDown("lshift") then
    M.selected = #M.filtered
  end
end

function M:textinput(t)
  if M.filter_active and #t == 1 then
    M.filter = M.filter .. t
    M:_apply_filter()
  end
end

function M:_select()
  if #M.filtered == 0 then return end
  local ci = M.filtered[M.selected] - 1  -- 0-indexed chapter

  -- Jump to selected chapter in reader
  local reader = require("reader.reader")
  reader.current_chapter = ci
  reader.scroll_y = 0
  reader.cursor_word = 0
  reader:_reflow()
  set_mode("reader")
end

return M
