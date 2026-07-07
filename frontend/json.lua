--[[
Minimal JSON encoder/decoder for Volta (LOVE 11.x).

LOVE 11.x does not ship with JSON support.  This handles the
subset we need: objects with string keys and string/number/boolean
values, plus nested objects and arrays.
]]

local M = {}

function M.encode(t)
  if type(t) == "string" then
    return '"' .. t:gsub('\\', '\\\\'):gsub('"', '\\"'):gsub('\n', '\\n') .. '"'
  elseif type(t) == "number" then
    return tostring(t)
  elseif type(t) == "boolean" then
    return t and "true" or "false"
  elseif type(t) == "table" then
    -- Check if array
    local is_array, max_k = true, 0
    for k in pairs(t) do
      if type(k) ~= "number" or k < 1 or k ~= math.floor(k) then is_array = false; break end
      if k > max_k then max_k = k end
    end
    if is_array and max_k > 0 then
      local parts = {}
      for i = 1, max_k do parts[i] = M.encode(t[i]) end
      return "[" .. table.concat(parts, ",") .. "]"
    else
      local parts = {}
      for k, v in pairs(t) do
        parts[#parts+1] = M.encode(k) .. ":" .. M.encode(v)
      end
      return "{" .. table.concat(parts, ",") .. "}"
    end
  elseif t == nil then
    return "null"
  else
    return '""'
  end
end

function M.decode(s)
  if type(s) ~= "string" then return nil end
  s = s:match("^%s*(.-)%s*$")
  if s == "" then return nil end

  local idx = 1

  local function skip_ws()
    while idx <= #s do
      local c = s:sub(idx, idx)
      if c ~= " " and c ~= "\n" and c ~= "\r" and c ~= "\t" then return end
      idx = idx + 1
    end
  end

  local function read_string()
    local c = s:sub(idx, idx)
    if c ~= '"' then return nil end
    idx = idx + 1
    local parts = {}
    while idx <= #s do
      c = s:sub(idx, idx)
      if c == '"' then idx = idx + 1; return table.concat(parts)
      elseif c == '\\' then
        idx = idx + 1
        local esc = s:sub(idx, idx)
        if esc == 'n' then parts[#parts+1] = '\n'
        elseif esc == 't' then parts[#parts+1] = '\t'
        elseif esc == '\\' then parts[#parts+1] = '\\'
        elseif esc == '"' then parts[#parts+1] = '"'
        elseif esc == 'u' then
          local hex = s:sub(idx+1, idx+4)
          parts[#parts+1] = utf8.char(tonumber(hex, 16) or 63)
          idx = idx + 4
        else parts[#parts+1] = esc end
        idx = idx + 1
      else
        parts[#parts+1] = c
        idx = idx + 1
      end
    end
    return nil
  end

  local function read_number()
    local start = idx
    if s:sub(idx, idx) == '-' then idx = idx + 1 end
    while idx <= #s do
      local c = s:sub(idx, idx)
      if (c >= '0' and c <= '9') or c == '.' or c == 'e' or c == 'E' or c == '+' or c == '-' then
        idx = idx + 1
      else break end
    end
    return tonumber(s:sub(start, idx-1))
  end

  local function read_value()
    skip_ws()
    if idx > #s then return nil end
    local c = s:sub(idx, idx)

    if c == '"' then
      return read_string()
    elseif c == '{' then
      idx = idx + 1
      local t = {}
      skip_ws()
      if s:sub(idx, idx) == '}' then idx = idx + 1; return t end
      while true do
        skip_ws()
        local key = read_string()
        if not key then return nil end
        skip_ws()
        if s:sub(idx, idx) ~= ':' then return nil end
        idx = idx + 1
        local val = read_value()
        t[key] = val
        skip_ws()
        c = s:sub(idx, idx)
        if c == ',' then idx = idx + 1
        elseif c == '}' then idx = idx + 1; return t
        else return nil end
      end
    elseif c == '[' then
      idx = idx + 1
      local t = {}
      skip_ws()
      if s:sub(idx, idx) == ']' then idx = idx + 1; return t end
      while true do
        local val = read_value()
        t[#t+1] = val
        skip_ws()
        c = s:sub(idx, idx)
        if c == ',' then idx = idx + 1
        elseif c == ']' then idx = idx + 1; return t
        else return nil end
      end
    elseif c == 't' then
      if s:sub(idx, idx+3) == "true" then idx = idx + 4; return true else return nil end
    elseif c == 'f' then
      if s:sub(idx, idx+4) == "false" then idx = idx + 5; return false else return nil end
    elseif c == 'n' then
      if s:sub(idx, idx+3) == "null" then idx = idx + 4; return nil else return nil end
    elseif (c >= '0' and c <= '9') or c == '-' then
      return read_number()
    end
    return nil
  end

  local val = read_value()
  if val == nil and idx <= #s then return nil end
  skip_ws()
  return val
end

return M
