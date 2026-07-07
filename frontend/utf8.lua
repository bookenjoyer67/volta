--[[
utf8.lua — minimal UTF-8 polyfill for LuaJIT (LÖVE)

LuaJIT (Lua 5.1) doesn't ship the utf8 module from Lua 5.3+.
This provides just the three functions Volta needs:
  utf8.len(s)     → codepoint count
  utf8.offset(s,n) → byte offset of nth codepoint (1-indexed)
  utf8.char(...)   → encode codepoints to UTF-8 string
]]

local utf8 = {}

function utf8.len(s)
  local len = 0
  local i = 1
  while i <= #s do
    local b = s:byte(i)
    if     b < 0x80 then i = i + 1
    elseif b < 0xE0 then i = i + 2
    elseif b < 0xF0 then i = i + 3
    else                 i = i + 4
    end
    len = len + 1
  end
  return len
end

function utf8.offset(s, n)
  if n <= 0 then return nil end
  local cp = 0
  local i = 1
  while i <= #s do
    cp = cp + 1
    if cp == n then return i end
    local b = s:byte(i)
    if     b < 0x80 then i = i + 1
    elseif b < 0xE0 then i = i + 2
    elseif b < 0xF0 then i = i + 3
    else                 i = i + 4
    end
  end
  return nil
end

function utf8.char(...)
  local t = {...}
  local out = {}
  for _, cp in ipairs(t) do
    if cp < 0x80 then
      out[#out + 1] = string.char(cp)
    elseif cp < 0x800 then
      out[#out + 1] = string.char(0xC0 + (cp / 64), 0x80 + (cp % 64))
    elseif cp < 0x10000 then
      out[#out + 1] = string.char(
        0xE0 + (cp / 4096),
        0x80 + ((cp / 64) % 64),
        0x80 + (cp % 64))
    else
      out[#out + 1] = string.char(
        0xF0 + (cp / 262144),
        0x80 + ((cp / 4096) % 64),
        0x80 + ((cp / 64) % 64),
        0x80 + (cp % 64))
    end
  end
  return table.concat(out)
end

return utf8
