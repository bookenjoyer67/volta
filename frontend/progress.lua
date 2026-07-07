--[[
Volta progress.lua — per-book reading progress persistence

Stores the last position for each opened book in a JSON file.
Uses absolute paths via io.open to avoid LOVE's unreliable
filesystem identity resolution.  Save dir: ~/.local/share/volta/
]]

local json = require("json")

local M = {}

-- Resolve save path: ~/.local/share/volta/
local home = os.getenv("HOME") or os.getenv("USERPROFILE") or "/tmp"
M._save_dir = home .. "/.local/share/volta"
M._progress_file = M._save_dir .. "/progress.json"

M.data = {}
M.dirty = false

--- Ensure the save directory exists.
function M:_ensure_dir()
  os.execute("mkdir -p '" .. M._save_dir:gsub("'", "'\\''") .. "'")
end

--- Initialise: load existing progress from disk.
function M:init()
  self:_ensure_dir()
  local f = io.open(self._progress_file, "r")
  if f then
    local content = f:read("*a")
    f:close()
    if content and content ~= "" then
      local ok, decoded = pcall(json.decode, content)
      if ok and type(decoded) == "table" then
        self.data = decoded
      end
    end
  end
end

--- Save a position entry for a book.
function M:save(path, entry)
  if not path then return end
  entry = entry or {}
  entry.last_ts = os.time()
  self.data[path] = entry
  self.dirty = true
  self:_flush()
end

--- Load the saved position for a book, or nil if none.
function M:load(path)
  if not path then return nil end
  return self.data[path]
end

--- Persist in-memory data to disk.
function M:_flush()
  if not self.dirty then return end
  self:_ensure_dir()
  local json_str = json.encode(self.data)
  if json_str then
    local f = io.open(self._progress_file, "w")
    if f then
      f:write(json_str)
      f:close()
      self.dirty = false
    end
  end
end

function M:flush()
  self:_flush()
end

return M
