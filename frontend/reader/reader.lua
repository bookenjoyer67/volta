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
M.wrapped_lines = {}

-- line_word_offsets[i] = word index (within the chapter) that
-- line i starts at.  Used to calculate RSVP entry point.
M.line_word_offsets = {}

-- Word cursor (vim-style): word index within current chapter
M.cursor_word = 0
M._gg_timer = 0  -- time of last 'g' press for gg detection
M._line_word_x = {}  -- _line_word_x[line_i] = {word1_x, word2_x, ...} per-word x-offsets
M._save_flash = 0   -- seconds remaining for "Saved" confirmation

function M:init()
  M.font_size = config.theme.reader.font_size or 18
  M.font = config:resolve_font(config.theme.reader.font, M.font_size)
  M.line_height = M.font:getHeight() * 1.4
end

function M:enter()
  M:init()

  -- Restore saved position if available
  local saved = book._saved
  if saved and saved.chapter then
    M.current_chapter = math.min(saved.chapter, book:chapter_count() - 1)
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
  if saved and saved.cursor_word then
    M.cursor_word = math.min(saved.cursor_word,
      M.line_word_offsets[#M.line_word_offsets] or 0)
    M:_scroll_to_cursor()
  else
    M.cursor_word = M.line_word_offsets[1] or 0
  end

  -- Save initial position (so even auto-opened books are tracked)
  M:_save_progress()
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

  local max_width = love.graphics.getWidth() - M.margin * 2
  local space_w = M.font:getWidth(" ")
  M.wrapped_lines = {}
  M.line_word_offsets = {}
  M._line_word_x = {}

  local current_line = ""
  local word_idx = 0  -- counter through the chapter's words
  local line_start_word = 0  -- word index where current line started
  local current_x = 0  -- x-offset for next word on this line
  local line_word_x = {}  -- x-offsets for words on current line

  for word in text:gmatch("%S+") do
    local word_w = M.font:getWidth(word)
    local test = current_line == "" and word or current_line .. " " .. word

    if M.font:getWidth(test) > max_width then
      -- Line is full — commit it with x-offsets
      table.insert(M.wrapped_lines, current_line)
      table.insert(M.line_word_offsets, line_start_word)
      table.insert(M._line_word_x, line_word_x)
      current_line = word
      line_start_word = word_idx
      current_x = word_w + space_w
      line_word_x = {0}
    else
      table.insert(line_word_x, current_x)
      current_line = test
      current_x = current_x + word_w + space_w
    end

    word_idx = word_idx + 1
  end

  -- Commit the last line
  if current_line ~= "" then
    table.insert(M.wrapped_lines, current_line)
    table.insert(M.line_word_offsets, line_start_word)
    table.insert(M._line_word_x, line_word_x)
  end

  -- Clamp cursor to valid range for this chapter
  M.cursor_word = math.min(M.cursor_word,
    M.line_word_offsets[#M.line_word_offsets] or 0)
end

--- Get the word index (within the current chapter) of the first
-- visible line, based on current scroll position.
function M:visible_word_offset()
  local visible_lines = math.floor(
    (love.graphics.getHeight() - 40) / math.max(1, M.line_height)
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
  local visible_bottom = M.scroll_y + h - 40

  if line_y < visible_top then
    M.scroll_y = math.max(0, line_y - M.line_height)
  elseif line_y + M.line_height > visible_bottom then
    M.scroll_y = line_y + M.line_height - h + 40 + M.line_height
  end
end

function M:draw()
  if not book:is_loaded() then return end
  if not M.font then M:init() end

  local w, h = love.graphics.getWidth(), love.graphics.getHeight()
  local theme = config.theme.reader

  love.graphics.clear(unpack(theme.bg))
  love.graphics.setFont(M.font)

  -- Title bar
  local title = book:title() .. "  |  Chapter "
    .. (M.current_chapter + 1) .. "/" .. book:chapter_count()
  love.graphics.setColor(unpack(theme.heading))
  love.graphics.print(title, 10, 5)

  -- Chapter progress bar
  local total_chapters = book:chapter_count()
  if total_chapters > 0 then
    local bar_w = w - 400
    local bar_x = w - bar_w - 10
    love.graphics.setColor(0.3, 0.3, 0.3)
    love.graphics.rectangle("fill", bar_x, 10, bar_w, 6)
    love.graphics.setColor(unpack(theme.link))
    love.graphics.rectangle("fill", bar_x, 10,
      bar_w * ((M.current_chapter + 1) / total_chapters), 6)
  end

  -- Text content
  love.graphics.setColor(unpack(theme.text))
  local y = 40 - M.scroll_y
  for i, line in ipairs(M.wrapped_lines) do
    local ly = y + (i - 1) * M.line_height
    if ly + M.line_height > 0 and ly < h then
      love.graphics.print(line, M.margin, ly)
    end
  end

  -- Cursor word highlight
  local cursor_line_idx = M:_line_for_word(M.cursor_word)
  local cursor_y = 40 + (cursor_line_idx - 1) * M.line_height - M.scroll_y
  if cursor_y >= 40 and cursor_y < h
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
          M.margin + word_x - 2, cursor_y,
          word_width + 4, M.line_height)
      end
    end
  end

  -- Position indicator
  local visible_lines = math.floor((h - 40) / math.max(1, M.line_height))
  local pages = math.max(1,
    math.ceil(#M.wrapped_lines / math.max(1, visible_lines)))
  local current_page = math.min(pages,
    math.floor(M.scroll_y / math.max(1, visible_lines * M.line_height)) + 1)
  love.graphics.setColor(0.5, 0.5, 0.5)
  love.graphics.print(
    string.format("Page %d/%d", current_page, pages), w - 120, h - 25)

  -- "Saved" flash
  if M._save_flash > 0 then
    local dt = math.min(love.timer.getDelta(), 0.05)
    M._save_flash = M._save_flash - dt
    local alpha = math.min(1, M._save_flash)
    love.graphics.setColor(0, 1, 0.5, alpha)
    love.graphics.print("Saved", 10, h - 25)
  end
end

function M:keypressed(key, scancode, isrepeat)
  local kb = input_mod

  -- Reset gg timer on any key except 'g'
  if key ~= kb:get("reader_chapter_top") then
    M._gg_timer = 0
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

  elseif key == kb:get("reader_next_chapter") then
    if M.current_chapter < book:chapter_count() - 1 then
      M.current_chapter = M.current_chapter + 1
      M.scroll_y = 0
      M.cursor_word = 0
      M:_reflow()
      M:_save_progress()
    end

  elseif key == kb:get("reader_prev_chapter") then
    if M.current_chapter > 0 then
      M.current_chapter = M.current_chapter - 1
      M.scroll_y = 0
      M.cursor_word = 0
      M:_reflow()
      M:_save_progress()
    end

  -- ── Vim gg / G ──

  elseif key == kb:get("reader_chapter_top") then
    local now = love.timer.getTime()
    if M._gg_timer > 0 and (now - M._gg_timer) < 0.3 then
      M.scroll_y = 0
      M.cursor_word = 0
      M._gg_timer = 0
    else
      M._gg_timer = now
    end

  elseif key == kb:get("reader_chapter_bottom")
      and (love.keyboard.isDown("lshift") or love.keyboard.isDown("rshift")) then
    M._gg_timer = 0
    local max_scroll = math.max(0,
      #M.wrapped_lines * M.line_height - love.graphics.getHeight() + 40)
    M.scroll_y = max_scroll
    M.cursor_word = M.line_word_offsets[#M.line_word_offsets] or 0

  -- ── Enter RSVP at cursor position ──

  elseif key == kb:get("reader_toggle_rsvp") then
    M:_save_progress()
    require("rsvp.rsvp"):enter()
    set_mode("rsvp")

  -- ── Back to menu ──

  elseif key == kb:get("reader_escape") then
    M:_save_progress()
    set_mode("menu")

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
    chapter = M.current_chapter,
    scroll_y = M.scroll_y,
    cursor_word = M.cursor_word,
    word_index = book:current_index(),
    wpm = config.wpm,
  })
  M._save_flash = 1.5  -- show "Saved" for 1.5 seconds
end

return M
