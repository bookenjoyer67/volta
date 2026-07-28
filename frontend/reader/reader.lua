--[[
Volta reader.lua — normal reading mode with reflowed text

Renders the current chapter as word-wrapped lines.  Tracks a
`line_word_offsets` array and a `cursor_word` for vim-style
word-level navigation with arrow keys.

Navigation: arrows move cursor word-by-word / line-by-line,
j/k scroll pages, Ctrl+d/u/f/b for vim-style page scrolling,
space/backspace for pages, n/p for chapters, mouse wheel to scroll.
]]

local book = require("book")
local config = require("config")
local input_mod = require("input")

local M = {}

M.scroll_y = 0
M.current_chapter = 0
M.line_height = 0
M.font_size = 18
M.margin = 60
M.max_col_width = 0   -- px; 0 = fill available width
M._origin_x = 60      -- computed in _reflow (margin + centering offset)
M._header_h = 52  -- header bar height, used in scroll calculations
M.wrapped_lines = {}

-- line_word_offsets[i] = word index (within the chapter) that
-- line i starts at.  Used to calculate RSVP entry point.
M.line_word_offsets = {}

-- Word cursor (vim-style): word index within current chapter
M.cursor_word = 0
M._gg_timer = 0  -- time of last 'g' press for gg detection
M._gt_timer = 0  -- time of last 'g' press for gt chord detection
M._line_word_x = {}  -- _line_word_x[line_i] = {word1_x, word2_x, ...} per-word x-offsets
M._flash = {0, ""}  -- {seconds, message} for status-bar flash

-- Search state
M.search_query = ""
M.search_active = false    -- typing in search bar
M.search_matches = {}      -- {{chapter=ch, word_offset=wo}, ...}
M.search_idx = 0
M.has_matches = false      -- matches exist, n/N navigate them
M.jump_stack = {}          -- {{chapter, cursor_word}, ...} for Ctrl+o back
M._needs_reflow = false    -- reflow needed on next draw

-- Inline image state
M._images = {}            -- { {word_offset, love_image, w, h, orig_w, orig_h}, ... }
M._image_y_offsets = {}   -- line_idx -> extra_y_pixels (cumulative image heights before this line)

-- Selection state
M.selection_anchor = nil  -- nil = no selection, number = anchor word index
M.visual_line_mode = false
M._mouse_dragging = false  -- true while LMB is held for selection

function M:init()
  M.font_size = config.theme.reader.font_size or 18
  M.font = config:resolve_font(config.theme.reader.font, M.font_size)
  M.line_height = M.font:getHeight() * 1.4
end

function M:enter()
  M:init()

  -- Restore saved position if available
  local saved = book._saved
  if saved and saved.current_chapter then
    M.current_chapter = math.min(saved.current_chapter, book:chapter_count() - 1)
    M.scroll_y = saved.scroll_y or 0
    book._saved = nil  -- consume so we don't re-restore on re-entry
  else
    M.scroll_y = 0
    -- Skip tiny chapters (cover pages, title pages — under 50 chars)
    M.current_chapter = 0
    while M.current_chapter < book:chapter_count() - 1 do
      local text = book:chapter_text(M.current_chapter)
      if #text > 50 then break end
      M.current_chapter = M.current_chapter + 1
    end
  end

  M:_reflow()

  -- Restore cursor position from saved progress, or start at top
  if saved and saved.current_word then
    M.cursor_word = math.min(saved.current_word,
      M.line_word_offsets[#M.line_word_offsets] or 0)
    M:_scroll_to_cursor()
  else
    M.cursor_word = M.line_word_offsets[1] or 0
  end

  -- Load inline images for this chapter
  M:_load_images()
end

--- Reflow chapter text into wrapped lines and track word offsets.
-- Called after chapter changes or window resize.
function M:_reflow()
  M.line_word_offsets = {}
  M._line_word_x = {}
  if not book:is_loaded() then return end

  local text = book:chapter_text(M.current_chapter)
  if text == "" then
    M.wrapped_lines = {"(empty chapter)"}
    M.line_word_offsets = {0}
    M._line_word_x = {{0}}
    return
  end

  local avail = love.graphics.getWidth() - M.margin * 2
  local max_width = (M.max_col_width > 0)
    and math.min(avail, M.max_col_width) or avail
  M._origin_x = M.margin + math.max(0, (avail - max_width) / 2)
  local space_w = M.font:getWidth(" ")
  local indent = "    "
  local indent_w = M.font:getWidth(indent)
  M.wrapped_lines = {}
  M.line_word_offsets = {}
  M._line_word_x = {}

  local word_idx = 0  -- counter through the chapter's words

  -- Paragraph-aware wrap: split on blank lines, indent each
  -- paragraph's first line.  Word indices never count the indent,
  -- so search offsets and RSVP entry points are unaffected.
  for paragraph in (text .. "\n\n"):gmatch("(.-)\n\n") do
    local current_line = ""
    local line_start_word = word_idx
    local current_x = indent_w  -- first line starts after the indent
    local line_word_x = {}
    local first_line = true

    for word in paragraph:gmatch("%S+") do
      local word_w = M.font:getWidth(word)
      local test = current_line == "" and word or current_line .. " " .. word
      local capacity = first_line and (max_width - indent_w) or max_width

      if M.font:getWidth(test) > capacity and current_line ~= "" then
        -- Line is full — commit it with x-offsets
        table.insert(M.wrapped_lines,
          first_line and (indent .. current_line) or current_line)
        table.insert(M.line_word_offsets, line_start_word)
        table.insert(M._line_word_x, line_word_x)
        current_line = word
        line_start_word = word_idx
        current_x = word_w + space_w
        line_word_x = {0}
        first_line = false
      else
        table.insert(line_word_x, current_x)
        current_line = test
        current_x = current_x + word_w + space_w
      end

      word_idx = word_idx + 1
    end

    -- Commit the paragraph's last line
    if current_line ~= "" then
      table.insert(M.wrapped_lines,
        first_line and (indent .. current_line) or current_line)
      table.insert(M.line_word_offsets, line_start_word)
      table.insert(M._line_word_x, line_word_x)
    end
  end

  -- Clamp cursor to valid range for this chapter
  M.cursor_word = math.min(M.cursor_word,
    M.line_word_offsets[#M.line_word_offsets] or 0)
end

--- Get the word index (within the current chapter) of the first
-- visible line, based on current scroll position.
function M:visible_word_offset()
  local visible_lines = math.floor(
    (love.graphics.getHeight() - M._header_h) / math.max(1, M.line_height)
  )
  local first_line = math.floor(
    M.scroll_y / math.max(1, M.line_height)
  )
  first_line = math.min(first_line, math.max(1, #M.line_word_offsets) - 1)
  return M.line_word_offsets[first_line + 1] or 0  -- Lua 1-indexed
end

--- Return the 1-based line index containing `word_idx`, or 1 if not found.
function M:_line_for_word(word_idx)
  for i = #M.line_word_offsets, 1, -1 do
    if M.line_word_offsets[i] <= word_idx then
      return i
    end
  end
  return 1
end

--- Auto-scroll so the line containing cursor_word is visible.
function M:_scroll_to_cursor()
  local line = M:_line_for_word(M.cursor_word)
  local line_y = (line - 1) * M.line_height
  local h = love.graphics.getHeight()
  local visible_top = M.scroll_y
  local visible_bottom = M.scroll_y + h - M._header_h

  if line_y < visible_top then
    M.scroll_y = math.max(0, line_y - M.line_height)
  elseif line_y + M.line_height > visible_bottom then
    M.scroll_y = line_y + M.line_height - h + M._header_h + M.line_height
  end
end

--- Load inline images for the current chapter from the FFI bridge.
function M:_load_images()
  M._images = {}
  M._image_y_offsets = {}

  if not book:is_loaded() then return end
  local count = book:chapter_image_count(M.current_chapter)
  print(string.format("[volta] _load_images: chapter=%d count=%d", M.current_chapter, count))
  if count == 0 then return end

  local avail = love.graphics.getWidth() - M.margin * 2
  local max_width = (M.max_col_width > 0)
    and math.min(avail, M.max_col_width) or avail

  for i = 0, count - 1 do
    local info = book:chapter_image_at(M.current_chapter, i)
    if info and info.path then
      print(string.format("[volta]   image %d: path=%s wo=%d w=%d h=%d", i, info.path, info.word_offset, info.width, info.height))
      local f = io.open(info.path, "r")
      if f then
        f:close()
        local ok, img = pcall(love.graphics.newImage, info.path)
        if ok and img then
          local iw, ih = img:getWidth(), img:getHeight()
          local scale = math.min(max_width / math.max(1, iw), 400 / math.max(1, ih), 1.0)
          local dw = iw * scale
          local dh = ih * scale
          print(string.format("[volta]   -> loaded %dx%d, scaled to %dx%d", iw, ih, dw, dh))
          table.insert(M._images, {
            word_offset = info.word_offset,
            img = img,
            w = dw,
            h = dh,
            orig_w = iw,
            orig_h = ih,
          })
        else
          print(string.format("[volta]   -> FAILED to load image: %s", ok and "pcall ok but nil img" or tostring(img)))
        end
      else
        print(string.format("[volta]   -> file not found: %s", info.path))
      end
    else
      print(string.format("[volta]   image %d: nil info or nil path", i))
    end
  end

  -- Sort by word_offset
  table.sort(M._images, function(a, b) return a.word_offset < b.word_offset end)

  -- Build y-offset map: for each image, find the line index
  -- where it should appear, and compute cumulative extra height
  M:_build_image_offsets()
end

--- Compute per-line vertical offsets from inline images.
-- Each image before a line pushes that line and all following lines down.
function M:_build_image_offsets()
  M._image_y_offsets = {}
  if #M._images == 0 then return end

  local line_count = #M.wrapped_lines

  -- First pass: assign each image to the line containing its word_offset
  for _, img in ipairs(M._images) do
    local wo = img.word_offset
    local best_line = 1
    for li = 1, line_count do
      local start_wo = M.line_word_offsets[li] or 0
      if start_wo <= wo then
        best_line = li
      else
        break
      end
    end
    -- Image appears before this line's first word
    img._line = best_line
  end

  -- Second pass: compute cumulative y-offset per line
  -- Sort images by line index
  table.sort(M._images, function(a, b) return a._line < b._line end)

  local cumulative_h = 0
  local img_idx = 1

  for line_i = 1, line_count do
    while img_idx <= #M._images and M._images[img_idx]._line == line_i do
      local img = M._images[img_idx]
      cumulative_h = cumulative_h + img.h + M.line_height * 0.5
      img_idx = img_idx + 1
    end
    M._image_y_offsets[line_i] = cumulative_h
  end
end

--- Push current position onto jump-back stack.
function M:_push_jump()
  local pos = {M.current_chapter, M.cursor_word}
  local top = M.jump_stack[#M.jump_stack]
  if not top or top[1] ~= pos[1] or top[2] ~= pos[2] then
    table.insert(M.jump_stack, pos)
    if #M.jump_stack > 20 then
      table.remove(M.jump_stack, 1)
    end
  end
end

--- Execute case-insensitive search across all chapters.
-- Populates M.search_matches with {chapter=ch, word_offset=wo}.
function M:_execute_search()
  M.search_matches = {}
  M.search_idx = 0
  local query = M.search_query:lower()
  if query == "" then return end

  local total_chapters = book:chapter_count()
  for ch = 0, total_chapters - 1 do
    local text = book:chapter_text(ch):lower()
    local start = 1
    while true do
      local found = text:find(query, start, true)
      if not found then break end
      -- Count words before this character position
      local prefix = book:chapter_text(ch):sub(1, found - 1)
      local word_offset = 0
      for _ in prefix:gmatch("%S+") do
        word_offset = word_offset + 1
      end
      table.insert(M.search_matches, {chapter = ch, word_offset = word_offset})
      start = found + #query
    end
  end

  M.has_matches = #M.search_matches > 0
  if M.has_matches then
    M.search_idx = 1
    M:_jump_to_match()
  end
end

--- Jump to the match at M.search_idx.
function M:_jump_to_match()
  if #M.search_matches == 0 then return end
  M:_push_jump()
  local idx = math.min(M.search_idx, #M.search_matches)
  local m = M.search_matches[idx]
  M.current_chapter = m.chapter
  M.cursor_word = m.word_offset
  M.scroll_y = 0
  M:_on_chapter_change()
  M:_scroll_to_cursor()
end

--- Search next match (wraps around).
function M:_search_next()
  if #M.search_matches == 0 then return end
  M.search_idx = M.search_idx + 1
  if M.search_idx > #M.search_matches then M.search_idx = 1 end
  M:_jump_to_match()
end

--- Search previous match (wraps around).
function M:_search_prev()
  if #M.search_matches == 0 then return end
  M.search_idx = M.search_idx - 1
  if M.search_idx < 1 then M.search_idx = #M.search_matches end
  M:_jump_to_match()
end

--- Build a set of match word offsets in the current chapter (for highlighting).
function M:_match_set()
  local s = {}
  for _, m in ipairs(M.search_matches) do
    if m.chapter == M.current_chapter then
      s[m.word_offset] = true
    end
  end
  return s
end

function M:draw()
  if M._needs_reflow then
    M:_reflow()
    M._needs_reflow = false
  end
  if not book:is_loaded() then return end
  if not M.font then M:init() end

  local w, h = love.graphics.getWidth(), love.graphics.getHeight()
  local theme = config.theme.reader

  love.graphics.clear(unpack(theme.bg))
  love.graphics.setFont(M.font)

  -- ── Header bar ──
  local total_chapters = book:chapter_count()

  -- Header background (slightly darker than page)
  love.graphics.setColor(theme.bg[1]*0.85, theme.bg[2]*0.85, theme.bg[3]*0.85, 0.95)
  love.graphics.rectangle("fill", 0, 0, w, M._header_h)
  -- Bottom border
  love.graphics.setColor(theme.heading[1]*0.3, theme.heading[2]*0.3, theme.heading[3]*0.3)
  love.graphics.rectangle("fill", 0, M._header_h - 1, w, 1)

  -- Format icon
  local icon = "📘"
  if book.file_path then
    local ext = book.file_path:match("%.([^.]+)$"):lower() or ""
    if ext == "epub" then icon = "📖"
    elseif ext == "pdf" then icon = "📄"
    elseif ext == "md" then icon = "📝" end
  end

  -- Title line: icon + book title | Ch X/Y | progress %
  local title_text = icon .. " " .. book:title()
  local title_font = love.graphics.newFont(14)
  love.graphics.setFont(title_font)
  love.graphics.setColor(unpack(theme.heading))
  love.graphics.print(title_text, 10, 6)
  -- Title dimensions for truncation
  if title_font:getWidth(title_text) > w - 200 then
    while title_font:getWidth(title_text .. "…") > w - 200 and #title_text > 10 do
      title_text = title_text:sub(1, -2)
    end
    title_text = title_text .. "…"
    love.graphics.print(title_text, 10, 6)
  end

  -- Chapter + progress on right
  local ch_text = string.format("Ch %d/%d", M.current_chapter + 1, total_chapters)
  local pct = 0
  if total_chapters > 0 then
    pct = math.floor((M.current_chapter + 1) / total_chapters * 100)
  end
  local right_text = string.format("%s  ▏ %d%%", ch_text, pct)
  love.graphics.setFont(love.graphics.newFont(12))
  love.graphics.setColor(theme.heading[1]*0.5, theme.heading[2]*0.5, theme.heading[3]*0.5)
  love.graphics.print(right_text, w - love.graphics.newFont(12):getWidth(right_text) - 10, 8)

  -- Chapter title line
  local ch_title = book:chapter_title(M.current_chapter)
  if ch_title and ch_title ~= "" then
    love.graphics.setFont(love.graphics.newFont(12))
    love.graphics.setColor(theme.heading[1]*0.7, theme.heading[2]*0.7, theme.heading[3]*0.7)
    local ch_display = ch_title
    if love.graphics.newFont(12):getWidth(ch_display) > w - 20 then
      while love.graphics.newFont(12):getWidth(ch_display .. "…") > w - 20 and #ch_display > 10 do
        ch_display = ch_display:sub(1, -2)
      end
      ch_display = ch_display .. "…"
    end
    love.graphics.print(ch_display, 14, 30)
  end

  -- Text content
  local match_set = M:_match_set()
  local y = M._header_h - M.scroll_y
  local max_text_width = (M.max_col_width > 0)
    and M.max_col_width or (w - M.margin * 2)
  local img_idx = 1

  for i, line in ipairs(M.wrapped_lines) do
    -- Apply image vertical offset for this line
    local img_offset = M._image_y_offsets[i] or 0
    local ly = y + (i - 1) * M.line_height + img_offset

    -- Render images that appear before this line
    while img_idx <= #M._images and M._images[img_idx]._line == i do
      local img_data = M._images[img_idx]
      local img_y = ly - img_data.h - M.line_height * 0.5
      local img_x = M._origin_x + (max_text_width - img_data.w) / 2
      if img_y + img_data.h > 0 and img_y < h then
        love.graphics.setColor(1, 1, 1)
        love.graphics.draw(img_data.img, img_x, img_y, 0, img_data.w / img_data.orig_w, img_data.h / img_data.orig_h)
      end
      img_idx = img_idx + 1
    end

    if ly + M.line_height > 0 and ly < h then
      -- Selection highlighting pass (drawn behind text)
      if M.selection_anchor then
        local sel_start = math.min(M.selection_anchor, M.cursor_word)
        local sel_end = math.max(M.selection_anchor, M.cursor_word)
        local first_word = M.line_word_offsets[i] or 0
        local word_count = M:_words_in_line(i)
        local line_end = first_word + word_count - 1
        -- Check if this line overlaps the selection range
        if first_word <= sel_end and line_end >= sel_start then
          local lead = line:match("^%s*")
          local x = M._origin_x + M.font:getWidth(lead)
          local wi = 0
          for word in line:gmatch("%S+") do
            local global_idx = first_word + wi
            wi = wi + 1
            if global_idx >= sel_start and global_idx <= sel_end then
              local word_w = M.font:getWidth(word)
              love.graphics.setColor(0.2, 0.25, 0.45, 0.7)
              love.graphics.rectangle("fill", x - 1, ly, word_w + 2, M.line_height)
            end
            x = x + M.font:getWidth(word) + M.font:getWidth(" ")
          end
        end
      end

      if M.has_matches then
        -- Word-by-word rendering with match highlighting
        local lead = line:match("^%s*")
        local x = M._origin_x + M.font:getWidth(lead)
        local first_word = M.line_word_offsets[i] or 0
        local word_idx = 0
        for word in line:gmatch("%S+") do
          local global_idx = first_word + word_idx
          local is_match = match_set[global_idx]
          local is_cursor = (global_idx == M.cursor_word)
          word_idx = word_idx + 1

          if is_cursor then
            local cursor_color = theme.cursor or theme.selection or {0.3, 0.5, 1, 0.35}
            love.graphics.setColor(unpack(cursor_color))
            love.graphics.rectangle("fill", x - 2, ly, M.font:getWidth(word) + 4, M.line_height)
            love.graphics.setColor(unpack(theme.text))
          elseif is_match then
            love.graphics.setColor(1, 0.78, 0.2)
            love.graphics.rectangle("fill", x - 1, ly, M.font:getWidth(word) + 2, M.line_height)
            love.graphics.setColor(0, 0, 0)
          else
            love.graphics.setColor(unpack(theme.text))
          end
          love.graphics.print(word, x, ly)
          x = x + M.font:getWidth(word) + M.font:getWidth(" ")
        end
      else
        love.graphics.setColor(unpack(theme.text))
        love.graphics.print(line, M._origin_x, ly)
      end
    end
  end

  -- Cursor word highlight
  local cursor_line_idx = M:_line_for_word(M.cursor_word)
  local cursor_y = M._header_h + (cursor_line_idx - 1) * M.line_height - M.scroll_y
  if cursor_y >= M._header_h and cursor_y < h
      and M._line_word_x[cursor_line_idx] then
    local word_offset = M.cursor_word - M.line_word_offsets[cursor_line_idx] + 1
    local word_x = M._line_word_x[cursor_line_idx][word_offset]
    if word_x then
      -- Extract the cursor word text from the line to measure its width
      local line_text = M.wrapped_lines[cursor_line_idx]
      local count = 0
      local cursor_word_text = ""
      for w in line_text:gmatch("%S+") do
        count = count + 1
        if count == word_offset then
          cursor_word_text = w
          break
        end
      end
      if cursor_word_text ~= "" then
        local word_width = M.font:getWidth(cursor_word_text)
        local cursor_color = theme.cursor or {1, 0.41, 0.71, 0.35}
        love.graphics.setColor(unpack(cursor_color))
        love.graphics.rectangle("fill",
          M._origin_x + word_x - 2, cursor_y,
          word_width + 4, M.line_height)
      end
    end
  end

  -- Scrollbar on right edge
  local sb_w = 6
  local sb_x = w - sb_w - 4
  local sb_h = h - M._header_h - 4
  if sb_h > 0 and w > 200 then
    local max_scroll = math.max(1, #M.wrapped_lines * M.line_height - sb_h)
    local thumb_h = math.max(20, sb_h * sb_h / math.max(1, (#M.wrapped_lines * M.line_height)))
    local thumb_y = M._header_h + (M.scroll_y / max_scroll) * (sb_h - thumb_h)

    -- Track
    love.graphics.setColor(0.08, 0.08, 0.12)
    love.graphics.rectangle("fill", sb_x, M._header_h, sb_w, sb_h, 3, 3)

    -- Thumb
    love.graphics.setColor(unpack(theme.selection))
    love.graphics.rectangle("fill", sb_x, thumb_y, sb_w, thumb_h, 3, 3)

    -- Global position dot on left of scrollbar
    if book:chapter_count() > 0 then
      local global_pct = (M.current_chapter + M.scroll_y / math.max(1, max_scroll + M.scroll_y)) / book:chapter_count()
      local dot_y = M._header_h + global_pct * sb_h
      love.graphics.setColor(theme.selection[1], theme.selection[2], theme.selection[3], 0.5)
      love.graphics.rectangle("fill", sb_x - 6, dot_y - 2, 4, 4, 1, 1)
    end
  end

  -- Position / search indicator
  if M.search_active then
    -- Search input bar at bottom
    love.graphics.setColor(0, 0, 0, 0.85)
    love.graphics.rectangle("fill", 0, h - 32, w, 32)
    love.graphics.setColor(unpack(theme.selection))
    local prompt = "/" .. M.search_query
    love.graphics.setFont(love.graphics.newFont(16))
    love.graphics.print(prompt, 10, h - 28)
  elseif M.has_matches then
    love.graphics.setColor(0.5, 0.5, 0.5)
    love.graphics.setFont(love.graphics.newFont(12))
    love.graphics.print(
      string.format("Match %d/%d", M.search_idx, #M.search_matches),
      w - 150, h - 25)
  else
    local visible_lines = math.floor((h - M._header_h) / math.max(1, M.line_height))
    local pages = math.max(1,
      math.ceil(#M.wrapped_lines / math.max(1, visible_lines)))
    local current_page = math.min(pages,
      math.floor(M.scroll_y / math.max(1, visible_lines * M.line_height)) + 1)
    love.graphics.setColor(0.5, 0.5, 0.5)
    love.graphics.print(
      string.format("Page %d/%d", current_page, pages), w - 120, h - 25)
  end

  -- Status bar (theme name + key hints)
  love.graphics.setColor(0.4, 0.4, 0.4)
  love.graphics.setFont(love.graphics.newFont(11))
  local status_left = config.theme_name .. "  |  / search  |  r RSVP  |  t/T theme  |  Ctrl+S save"
  love.graphics.print(status_left, 10, h - 16)

  -- Status-bar flash (Saved / Yanked / etc.)
  if M._flash[1] > 0 then
    local dt = math.min(love.timer.getDelta(), 0.05)
    M._flash[1] = M._flash[1] - dt
    local alpha = math.min(1, M._flash[1])
    love.graphics.setColor(0, 1, 0.5, alpha)
    love.graphics.print(M._flash[2], 10, h - 25)
  end
end

--- Called after chapter changes: reflow text and reload inline images.
function M:_on_chapter_change()
  M:_reflow()
  M:_load_images()
end

function M:keypressed(key, scancode, isrepeat)
  local kb = input_mod

  -- ── Search input mode (captures all keystrokes) ──
  if M.search_active then
    if key == "escape" then
      M.search_active = false
      M.search_query = ""
    elseif key == "backspace" then
      M.search_query = M.search_query:sub(1, -2)
    elseif key == "return" then
      M.search_active = false
      M:_execute_search()
    end
    return  -- block all other keys during search input
  end

  -- Reset gg/gt timer on any key except 'g'
  if key ~= kb:get("reader_chapter_top") then
    M._gg_timer = 0
    M._gt_timer = 0
  end

  -- ── Toggle search ──

  if key == kb:get("reader_toggle_search") then
    M.search_active = true
    M.search_query = ""
    return
  end

  -- ── Yank (copy): y when visual mode active ──

  if key == "y" and M.selection_anchor then
    M:_yank_selection()
    return
  end

  -- ── Visual mode: v = char-wise, V = line-wise ──

  if key == "v" then
    if M.selection_anchor then
      M.selection_anchor = nil
      M.visual_line_mode = false
    else
      M.selection_anchor = M.cursor_word
      M.visual_line_mode = false
    end
    return
  end

  if key == "V" then
    if M.selection_anchor then
      M.selection_anchor = nil
      M.visual_line_mode = false
    else
      local line = M:_line_for_word(M.cursor_word)
      local line_start = M.line_word_offsets[line]
      local line_end = line_start + (M:_words_in_line(line) - 1)
      M.selection_anchor = line_start
      M.cursor_word = line_end
      M.visual_line_mode = true
    end
    return
  end

  -- ── Cursor movement (arrow keys) ──

  if key == kb:get("reader_cursor_up") then
    local cur_line = M:_line_for_word(M.cursor_word)
    if cur_line > 1 then
      local prev_line = cur_line - 1
      local first_word = M.line_word_offsets[prev_line]
      local offset = M.cursor_word - M.line_word_offsets[cur_line]
      M.cursor_word = first_word + offset
      if prev_line < #M.line_word_offsets then
        M.cursor_word = math.min(M.cursor_word,
          M.line_word_offsets[prev_line + 1] - 1)
      end
    end
    M:_scroll_to_cursor()

  elseif key == kb:get("reader_cursor_down") then
    local cur_line = M:_line_for_word(M.cursor_word)
    if cur_line < #M.line_word_offsets then
      local next_line = cur_line + 1
      local first_word = M.line_word_offsets[next_line]
      local offset = M.cursor_word - M.line_word_offsets[cur_line]
      M.cursor_word = first_word + offset
      if next_line < #M.line_word_offsets then
        M.cursor_word = math.min(M.cursor_word,
          M.line_word_offsets[next_line + 1] - 1)
      end
    end
    M:_scroll_to_cursor()

  elseif key == kb:get("reader_cursor_left") then
    M.cursor_word = math.max(0, M.cursor_word - 1)
    M:_scroll_to_cursor()

  elseif key == kb:get("reader_cursor_right") then
    local max_word = M.line_word_offsets[#M.line_word_offsets] or 0
    M.cursor_word = math.min(M.cursor_word + 1, max_word)
    M:_scroll_to_cursor()

  -- ── Scroll (j/k) ──

  elseif key == kb:get("reader_scroll_down") then
    M.scroll_y = M.scroll_y + M.line_height * 3
    M.cursor_word = M.line_word_offsets[
      math.min(#M.line_word_offsets,
        math.floor(M.scroll_y / math.max(1, M.line_height)) + 1)
    ] or M.cursor_word

  elseif key == kb:get("reader_scroll_up") then
    M.scroll_y = math.max(0, M.scroll_y - M.line_height * 3)
    M.cursor_word = M.line_word_offsets[
      math.min(#M.line_word_offsets,
        math.floor(M.scroll_y / math.max(1, M.line_height)) + 1)
    ] or M.cursor_word

  -- ── Vim Ctrl+d / Ctrl+u (half-page) ──

  elseif key == kb:get("reader_half_page_down")
      and (love.keyboard.isDown("lctrl") or love.keyboard.isDown("rctrl")) then
    M.scroll_y = M.scroll_y + love.graphics.getHeight() * 0.5
    M.cursor_word = M.line_word_offsets[
      math.min(#M.line_word_offsets,
        math.floor(M.scroll_y / math.max(1, M.line_height)) + 1)
    ] or M.cursor_word

  elseif key == kb:get("reader_half_page_up")
      and (love.keyboard.isDown("lctrl") or love.keyboard.isDown("rctrl")) then
    M.scroll_y = math.max(0, M.scroll_y - love.graphics.getHeight() * 0.5)
    M.cursor_word = M.line_word_offsets[
      math.min(#M.line_word_offsets,
        math.floor(M.scroll_y / math.max(1, M.line_height)) + 1)
    ] or M.cursor_word

  -- ── Vim Ctrl+f / Ctrl+b (full page) ──

  elseif key == kb:get("reader_full_page_down")
      and (love.keyboard.isDown("lctrl") or love.keyboard.isDown("rctrl")) then
    M.scroll_y = M.scroll_y + love.graphics.getHeight() * 0.8
    M.cursor_word = M.line_word_offsets[
      math.min(#M.line_word_offsets,
        math.floor(M.scroll_y / math.max(1, M.line_height)) + 1)
    ] or M.cursor_word

  elseif key == kb:get("reader_full_page_up")
      and (love.keyboard.isDown("lctrl") or love.keyboard.isDown("rctrl")) then
    M.scroll_y = math.max(0, M.scroll_y - love.graphics.getHeight() * 0.8)
    M.cursor_word = M.line_word_offsets[
      math.min(#M.line_word_offsets,
        math.floor(M.scroll_y / math.max(1, M.line_height)) + 1)
    ] or M.cursor_word

  -- ── Page (space / backspace / PgDn / PgUp) ──

  elseif key == kb:get("reader_page_down") or key == "pagedown" then
    M.scroll_y = M.scroll_y + love.graphics.getHeight() * 0.8
    M.cursor_word = M.line_word_offsets[
      math.min(#M.line_word_offsets,
        math.floor(M.scroll_y / math.max(1, M.line_height)) + 1)
    ] or M.cursor_word

  elseif key == kb:get("reader_page_up") or key == "pageup" then
    M.scroll_y = math.max(0,
      M.scroll_y - love.graphics.getHeight() * 0.8)
    M.cursor_word = M.line_word_offsets[
      math.min(#M.line_word_offsets,
        math.floor(M.scroll_y / math.max(1, M.line_height)) + 1)
    ] or M.cursor_word

  -- ── Chapter navigation ──

  elseif key == kb:get("reader_search_next") then
    if M.has_matches then
      M:_search_next()
    elseif M.current_chapter < book:chapter_count() - 1 then
      M.current_chapter = M.current_chapter + 1
      M.scroll_y = 0
      M.cursor_word = 0
      M:_on_chapter_change()
    end

  elseif key == kb:get("reader_search_prev") then
    if M.has_matches then
      M:_search_prev()
    elseif M.current_chapter > 0 then
      M.current_chapter = M.current_chapter - 1
      M.scroll_y = 0
      M.cursor_word = 0
      M:_on_chapter_change()
    end

  elseif key == "p" and not M.has_matches then
    if M.current_chapter > 0 then
      M.current_chapter = M.current_chapter - 1
      M.scroll_y = 0
      M.cursor_word = 0
      M:_on_chapter_change()
    end

  -- ── Vim gg / G ──

  elseif key == kb:get("reader_chapter_top") then
    local now = love.timer.getTime()
    -- gt chord: g then t within 300ms enters TOC
    if M._gt_timer > 0 and (now - M._gt_timer) < 0.3 then
      M._gt_timer = 0
      -- handled below
    -- gg chord: double-tap g within 300ms goes to top
    elseif M._gg_timer > 0 and (now - M._gg_timer) < 0.3 then
      M.scroll_y = 0
      M.cursor_word = 0
      M._gg_timer = 0
      M._gt_timer = 0
    else
      M._gg_timer = now
      M._gt_timer = now
    end

  -- gt chord: 't' after 'g' enters TOC
  elseif key == "t" and M._gt_timer > 0
      and (love.timer.getTime() - M._gt_timer) < 0.3 then
    M._gt_timer = 0
    M:_push_jump()
    require("ui.toc"):enter(M.current_chapter, M.cursor_word)
    set_mode("toc")
    return

  elseif key == kb:get("reader_chapter_bottom")
      and (love.keyboard.isDown("lshift") or love.keyboard.isDown("rshift")) then
    M._gg_timer = 0
    local max_scroll = math.max(0,
      #M.wrapped_lines * M.line_height - love.graphics.getHeight() + 40)
    M.scroll_y = max_scroll
    M.cursor_word = M.line_word_offsets[#M.line_word_offsets] or 0

  -- ── Enter RSVP at cursor position ──

  elseif key == kb:get("reader_toggle_rsvp") then
    require("rsvp.rsvp"):enter()
    set_mode("rsvp")

  -- ── Back to menu / clear search ──

  elseif key == kb:get("reader_escape") then
    -- Clear selection first, then search matches, then go to menu
    if M.selection_anchor then
      M.selection_anchor = nil
      M.visual_line_mode = false
    elseif M.has_matches then
      M.has_matches = false
      M.search_matches = {}
      M.search_idx = 0
    else
      set_mode("menu")
    end

  -- ── Jump back (Ctrl+o) ──

  elseif key == "o" and (love.keyboard.isDown("lctrl") or love.keyboard.isDown("rctrl")) then
    if #M.jump_stack > 0 then
      local pos = table.remove(M.jump_stack)
      M.current_chapter = pos[1]
      M.cursor_word = pos[2]
      M.scroll_y = 0
      M:_on_chapter_change()
      M:_scroll_to_cursor()
    end

  -- ── Manual save (Ctrl+S) ──

  elseif key == "s" and (love.keyboard.isDown("lctrl") or love.keyboard.isDown("rctrl")) then
    M:_save_progress()

  -- Theme cycling (t = next, T = previous)
  elseif key == kb:get("reader_cycle_theme") then
    config:cycle_theme(1)
    M:init()
    M:_reflow()
    love.graphics.setBackgroundColor(unpack(config.theme.reader.bg))

  elseif key == kb:get("reader_cycle_theme_rev") then
    config:cycle_theme(-1)
    M:init()
    M:_reflow()
    love.graphics.setBackgroundColor(unpack(config.theme.reader.bg))

  -- ── Layout: margins and max column width ──

  elseif key == kb:get("reader_margin_narrow") then
    M.margin = math.max(0, M.margin - 10)
    M:_reflow()

  elseif key == kb:get("reader_margin_wide") then
    M.margin = math.min(400, M.margin + 10)
    M:_reflow()

  elseif key == kb:get("reader_col_narrow") then
    if M.max_col_width > 0 then
      local next_w = M.max_col_width - 40
      -- Below a readable floor, turn the limit off
      M.max_col_width = next_w < 400 and 0 or next_w
      M:_reflow()
    end

  elseif key == kb:get("reader_col_wide") then
    -- Off -> start at a readable measure
    M.max_col_width = M.max_col_width == 0 and 800
      or math.min(1600, M.max_col_width + 40)
    M:_reflow()

  end
end

function M:textinput(t)
  if M.search_active and #t == 1 then
    M.search_query = M.search_query .. t
  end
end

function M:wheelmoved(x, y)
  M.scroll_y = math.max(0, M.scroll_y - y * M.line_height * 5)
  -- Nudge cursor to follow scroll position
  M.cursor_word = M.line_word_offsets[
    math.min(#M.line_word_offsets,
      math.floor(M.scroll_y / math.max(1, M.line_height)) + 1)
  ] or M.cursor_word
end

--- Save current progress for the open book.
function M:_save_progress()
  if not book:is_loaded() then return end
  local progress = require("progress")
  progress:save(book.file_path, {
    current_chapter = M.current_chapter,
    scroll_y = M.scroll_y,
    current_word = M.cursor_word,
    word_index = book:current_index(),
    wpm = config.wpm,
  })
  M._flash = {1.5, "Saved"}
end

--- Count words on a wrapped line (by word index i).
function M:_words_in_line(line_idx)
  if not M.wrapped_lines[line_idx] then return 0 end
  local count = 0
  for _ in M.wrapped_lines[line_idx]:gmatch("%S+") do
    count = count + 1
  end
  return count
end

--- Build selected text from the current selection range and copy to clipboard.
function M:_yank_selection()
  if not M.selection_anchor then return end
  local start_w = math.min(M.selection_anchor, M.cursor_word)
  local end_w = math.max(M.selection_anchor, M.cursor_word)
  local parts = {}
  for li = 1, #M.wrapped_lines do
    local first = M.line_word_offsets[li]
    local words = M:_words_in_line(li)
    if words > 0 then
      local last = first + words - 1
      if first <= end_w and last >= start_w then
        local line_words = {}
        local wi = 0
        for w in M.wrapped_lines[li]:gmatch("%S+") do
          local global = first + wi
          wi = wi + 1
          if global >= start_w and global <= end_w then
            table.insert(line_words, w)
          end
        end
        if #line_words > 0 then
          table.insert(parts, table.concat(line_words, " "))
        end
      end
    end
  end
  local text = table.concat(parts, " ")
  if text ~= "" then
    love.system.setClipboard(text)
  end
  M.selection_anchor = nil
  M.visual_line_mode = false
  M._flash = {1.5, "Yanked"}
end

--- Find which word index is at screen position (x, y), or nil if none.
function M:_word_at_position(x, y)
  local line_idx = math.floor((y - M._header_h + M.scroll_y) / M.line_height) + 1
  if line_idx < 1 or line_idx > #M.wrapped_lines then return nil end
  local offsets = M._line_word_x[line_idx]
  if not offsets then return nil end
  local words = {}
  for w in M.wrapped_lines[line_idx]:gmatch("%S+") do table.insert(words, w) end
  for i = #offsets, 1, -1 do
    if offsets[i] <= (x - M._origin_x) then
      return (M.line_word_offsets[line_idx] or 0) + i - 1
    end
  end
  return M.line_word_offsets[line_idx] or 0
end

--- Mouse press: start selection on left click.
function M:mousepressed(x, y, button, istouch, presses)
  if button ~= 1 then return end
  local word_idx = M:_word_at_position(x, y)
  if word_idx then
    M.selection_anchor = word_idx
    M.cursor_word = word_idx
    M._mouse_dragging = true
  end
end

--- Mouse release: finish selection drag.
function M:mousereleased(x, y, button, istouch, presses)
  if button == 1 then
    M._mouse_dragging = false
  end
end

--- Mouse move: extend selection while dragging.
function M:mousemoved(x, y, dx, dy, istouch)
  if not M._mouse_dragging or not M.selection_anchor then return end
  local word_idx = M:_word_at_position(x, y)
  if word_idx then
    M.cursor_word = word_idx
    -- Auto-scroll when near top/bottom edges
    local h = love.graphics.getHeight()
    if y < M._header_h + 30 then
      M.scroll_y = math.max(0, M.scroll_y - M.line_height * 2)
    elseif y > h - 30 then
      M.scroll_y = M.scroll_y + M.line_height * 2
    end
  end
end

return M
