--[[
Volta rsvp.lua — Rapid Serial Visual Presentation speed reading

Displays words one at a time at a configurable WPM rate, with
the ORP (Optimal Recognition Point) character highlighted.
The pivot position is centre-screen by default.

On enter, seeks to the word the user was looking at in reader
mode — not just the chapter start.  This uses the reader module's
line_word_offsets array to map scroll position → word index.
]]

local book = require("book")
local config = require("config")
local input_mod = require("input")

local M = {}

M.wpm = 300
M.accumulator = 0
M.show_stats = false
M.pivot_x = 0.5  -- 0.0=left, 0.5=center, 1.0=right
M.font_size = 48
M._save_flash = 0  -- "Saved" confirmation timer

function M:enter()
  M.font = config:resolve_font(config.theme.rsvp.font, M.font_size)
  M.wpm = 300
  M.accumulator = 0
  M.show_stats = false
  M._save_flash = 0

  -- Seek to the word the cursor was on in reader mode.
  local reader = require("reader.reader")
  local ch = reader.current_chapter
  local cursor = reader.cursor_word or 0
  local chapter_start_idx = book:chapter_start(ch)
  local idx = chapter_start_idx + cursor

  -- Clamp to document bounds
  idx = math.min(idx, math.max(0, book:word_count() - 1))

  book:seek(idx)
  book:play()
end

function M:update(dt)
  if not book:is_loaded() or not book:is_playing() then return end

  local ms = dt * 1000
  local jumped = book:tick(ms)
  M.accumulator = M.accumulator + ms

  -- Track WPM from actual ticks
  if M.accumulator >= 1000 then
    M.accumulator = M.accumulator - 1000
  end
end

function M:draw()
  if not book:is_loaded() then return end
  if not M.font then M:enter() end

  local w, h = love.graphics.getWidth(), love.graphics.getHeight()
  local theme = config.theme.rsvp

  love.graphics.clear(unpack(theme.bg))

  local idx = book:current_index()
  local total = book:word_count()
  local word = book:word_at(idx)

  -- Pivot word display with ORP highlighting
  if word ~= "" then
    love.graphics.setFont(M.font)
    love.graphics.setColor(unpack(theme.word))

    -- ORP: ~40% into the word is where the eye naturally fixates.
    -- Use utf8.len for character count (not #byte length) to avoid
    -- splitting multi-byte characters like em-dashes and curly quotes.
    local char_count = utf8.len(word) or #word
    local pivot_idx = math.floor(char_count * 0.4) + 1
    pivot_idx = math.min(pivot_idx, char_count)

    -- Convert character index to byte offset for substring extraction
    local pivot_byte = utf8.offset(word, pivot_idx) or pivot_idx
    local pivot_next = utf8.offset(word, pivot_idx + 1) or (#word + 1)

    local left_part = word:sub(1, pivot_byte - 1)
    local pivot = word:sub(pivot_byte, pivot_next - 1)
    local right_part = word:sub(pivot_next)

    local pivot_x = w * M.pivot_x
    local pivot_width = M.font:getWidth(pivot)
    local left_width = M.font:getWidth(left_part)

    local start_x = pivot_x - left_width - pivot_width / 2
    local y = h / 2 - M.font_size / 2

    -- Left part (faded)
    love.graphics.setColor(unpack(theme.word_fade))
    love.graphics.print(left_part, start_x, y)

    -- Pivot character (full brightness)
    love.graphics.setColor(unpack(theme.word))
    love.graphics.print(pivot, start_x + left_width, y)

    -- Right part (faded)
    love.graphics.setColor(unpack(theme.word_fade))
    love.graphics.print(right_part,
      start_x + left_width + pivot_width, y)
  end

  -- Bottom HUD: WPM, position, chapter
  love.graphics.setFont(love.graphics.newFont(14))
  love.graphics.setColor(unpack(theme.hud))
  love.graphics.print(
    string.format("WPM: %d  |  Word: %d/%d  |  Chapter: %d/%d",
      M.wpm, idx + 1, total,
      book:chapter_at(idx) + 1, book:chapter_count()),
    10, h - 25)

  -- Progress bar (global document position)
  if total > 0 then
    local bar_h = 4
    love.graphics.setColor(0.2, 0.2, 0.2)
    love.graphics.rectangle("fill", 0, h - bar_h - 30, w, bar_h)
    love.graphics.setColor(unpack(theme.progress))
    love.graphics.rectangle("fill", 0, h - bar_h - 30,
      w * ((idx + 1) / total), bar_h)
  end

  -- Stats overlay (toggled with 's')
  if M.show_stats then
    love.graphics.setColor(0, 0, 0, 0.7)
    love.graphics.rectangle("fill",
      w / 2 - 150, h / 2 - 60, 300, 120)
    love.graphics.setFont(love.graphics.newFont(16))
    love.graphics.setColor(unpack(theme.hud))
    love.graphics.print(
      string.format(
        "WPM: %d\nWords: %d / %d\nChapter: %d / %d\nProgress: %.1f%%",
        M.wpm, idx + 1, total,
        book:chapter_at(idx) + 1, book:chapter_count(),
        total > 0 and ((idx + 1) / total * 100) or 0),
      w / 2 - 140, h / 2 - 50)
  end

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

  if key == kb:get("rsvp_play_pause") then
    if book:is_playing() then
      book:pause()
    else
      book:play()
    end
  elseif key == kb:get("rsvp_seek_back_10") then
    local idx = book:current_index()
    book:seek(idx >= 10 and idx - 10 or 0)
  elseif key == kb:get("rsvp_seek_forward_10") then
    local idx = book:current_index()
    local total = book:word_count()
    book:seek(math.min(idx + 10, total - 1))
  elseif key == kb:get("rsvp_seek_back_100") then
    local idx = book:current_index()
    book:seek(idx >= 100 and idx - 100 or 0)
  elseif key == kb:get("rsvp_seek_forward_100") then
    local idx = book:current_index()
    local total = book:word_count()
    book:seek(math.min(idx + 100, total - 1))

  -- Vim-style hjkl seeking
  elseif key == kb:get("rsvp_seek_back_10_vim") then
    local idx = book:current_index()
    book:seek(idx >= 10 and idx - 10 or 0)
  elseif key == kb:get("rsvp_seek_forward_10_vim") then
    local idx = book:current_index()
    local total = book:word_count()
    book:seek(math.min(idx + 10, total - 1))
  elseif key == kb:get("rsvp_seek_back_100_vim") then
    local idx = book:current_index()
    book:seek(idx >= 100 and idx - 100 or 0)
  elseif key == kb:get("rsvp_seek_forward_100_vim") then
    local idx = book:current_index()
    local total = book:word_count()
    book:seek(math.min(idx + 100, total - 1))

  elseif key == kb:get("rsvp_speed_up") then
    M.wpm = math.min(1000, M.wpm + 25)
    book:set_wpm(M.wpm)
  elseif key == kb:get("rsvp_speed_down") then
    M.wpm = math.max(50, M.wpm - 25)
    book:set_wpm(M.wpm)
  elseif key == kb:get("rsvp_exit") then
    book:pause()
    set_mode("reader")
    require("reader.reader"):enter()
  elseif key == kb:get("rsvp_toggle_stats") then
    M.show_stats = not M.show_stats

  -- Font size adjustment
  elseif key == kb:get("rsvp_font_up") then
    M.font_size = math.min(128, M.font_size + 8)
    M.font = config:resolve_font(config.theme.rsvp.font, M.font_size)
  elseif key == kb:get("rsvp_font_down") then
    M.font_size = math.max(16, M.font_size - 8)
    M.font = config:resolve_font(config.theme.rsvp.font, M.font_size)

  -- Manual save (Ctrl+S)
  elseif key == "s" and (love.keyboard.isDown("lctrl") or love.keyboard.isDown("rctrl")) then
    local idx = book:current_index()
    local ch = book:chapter_at(idx)
    local ch_start = book:chapter_start(ch)
    local progress = require("progress")
    progress:save(book.file_path, {
      chapter = ch,
      scroll_y = 0,
      cursor_word = idx - ch_start,
      word_index = idx,
      wpm = M.wpm,
    })
    M._save_flash = 1.5
  end
end

function M:textinput(t)
  -- Numeric WPM entry (type a number, press Enter)
  if tonumber(t) then
    M.wpm_input = (M.wpm_input or "") .. t
  end
end

return M
