-- lua/mdview/window.lua
-- floating window の生成・破棄を担当する。

local M = {}

--- width/height 値を絶対セル数に変換する。
--- 0 < v < 1 なら editor 比率、それ以外は整数として扱う。
---@param v      number  設定値
---@param total  integer editor の幅または高さ（セル数）
---@return integer
local function resolve_size(v, total)
  if v > 0 and v < 1 then
    return math.floor(total * v)
  end
  return math.floor(v)
end

--- floating window を新規バッファとともに作成して返す。
---@param cfg MdviewWindowConfig
---@return integer buf, integer win
function M.create_floating(cfg)
  local editor_w = vim.o.columns
  local editor_h = vim.o.lines - vim.o.cmdheight - 1

  local w = resolve_size(cfg.width,  editor_w)
  local h = resolve_size(cfg.height, editor_h)

  -- 最低サイズを保証（3x3）
  w = math.max(w, 3)
  h = math.max(h, 3)

  local row = math.floor((editor_h - h) / 2)
  local col = math.floor((editor_w - w) / 2)

  local buf = vim.api.nvim_create_buf(false, true)

  local win_opts = {
    relative = 'editor',
    row      = row,
    col      = col,
    width    = w,
    height   = h,
    style    = 'minimal',
    border   = cfg.border,
  }

  -- title は neovim 0.9+ で利用可能
  if vim.fn.has('nvim-0.9') == 1 then
    win_opts.title          = cfg.title
    win_opts.title_pos      = 'center'
  end

  local win = vim.api.nvim_open_win(buf, true, win_opts)

  -- terminal バッファらしい外見にする
  vim.wo[win].number         = false
  vim.wo[win].relativenumber = false
  vim.wo[win].signcolumn     = 'no'
  vim.wo[win].cursorline     = false

  return buf, win
end

--- win が有効なら close する。
---@param win integer|nil
function M.close(win)
  if win and vim.api.nvim_win_is_valid(win) then
    vim.api.nvim_win_close(win, true)
  end
end

return M
