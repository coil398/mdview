-- lua/mdview/config.lua
-- デフォルト設定とユーザー opts のディープマージを提供する。

local M = {}

---@class MdviewWindowConfig
---@field width  number  0〜1 なら editor 比率、整数なら絶対セル数
---@field height number  0〜1 なら editor 比率、整数なら絶対セル数
---@field border string  nvim_open_win の border 引数
---@field title  string  floating window タイトル

---@class MdviewConfig
---@field bin               string          mdview バイナリ名または絶対パス
---@field window            MdviewWindowConfig
---@field auto_close_on_exit boolean        TUI 終了時に floating window を自動 close するか

---@type MdviewConfig
M.defaults = {
  bin    = 'mdview',
  window = {
    width  = 0.85,
    height = 0.85,
    border = 'rounded',
    title  = ' mdview ',
  },
  auto_close_on_exit = true,
}

--- tbl_deep_extend の薄いラッパー。
---@param base table
---@param override table|nil
---@return table
local function deep_merge(base, override)
  if override == nil then
    return vim.deepcopy(base)
  end
  return vim.tbl_deep_extend('force', vim.deepcopy(base), override)
end

--- デフォルト設定にユーザー opts をマージして返す。
---@param opts MdviewConfig|nil
---@return MdviewConfig
function M.resolve(opts)
  return deep_merge(M.defaults, opts)
end

return M
