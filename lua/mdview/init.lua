-- lua/mdview/init.lua
-- 公開 API: setup(opts) / open(opts) / config()

local M = {}

-- モジュール内に現在の floating win/buf を保持（多重起動防止）
local _state = {
  win = nil,
  buf = nil,
  cfg = nil,  -- setup() で確定した設定
}

--- プラグインを初期化する。
--- lazy.nvim 等では opts = {} を渡して呼ぶ。
---@param opts MdviewConfig|nil
function M.setup(opts)
  local config = require('mdview.config')
  _state.cfg = config.resolve(opts)
end

--- 現在の解決済み設定を返す。
--- health.lua から参照する。setup 未呼び出し時はデフォルト設定を返す。
---@return MdviewConfig
function M.config()
  if _state.cfg == nil then
    _state.cfg = require('mdview.config').resolve(nil)
  end
  return _state.cfg
end

--- floating window で mdview を起動する。
---@param opts { path?: string, force?: boolean }|nil
function M.open(opts)
  opts = opts or {}

  -- setup が呼ばれていなければデフォルト設定を使用
  if _state.cfg == nil then
    _state.cfg = require('mdview.config').resolve(nil)
  end
  local cfg = _state.cfg

  local runner = require('mdview.runner')
  local window = require('mdview.window')

  -- バイナリ検出
  if not runner.find_bin(cfg.bin) then
    vim.notify(
      ('mdview: `%s` が PATH 上に見つかりません。\n' ..
       '  cargo install --path mdview-tui  でインストールしてください。\n' ..
       '  または setup({ bin = "/絶対/パス/mdview" }) でパスを指定してください。'):format(cfg.bin),
      vim.log.levels.ERROR
    )
    return
  end

  -- ファイルパス解決
  local path, tmp_path, err = runner.resolve_path(opts)
  if err then
    vim.notify(err, vim.log.levels.ERROR)
    return
  end

  -- 既存 window が有効なら close して新規 open
  if _state.win and vim.api.nvim_win_is_valid(_state.win) then
    window.close(_state.win)
    _state.win = nil
    _state.buf = nil
  end

  -- floating window 作成
  local buf, win = window.create_floating(cfg.window)
  _state.win = win
  _state.buf = buf

  -- termopen 実行
  runner.launch(buf, win, cfg.bin, path, tmp_path, cfg.auto_close_on_exit, window)

  -- window が close されたら state をリセット
  local group = vim.api.nvim_create_augroup('MdviewCleanup_' .. win, { clear = true })
  vim.api.nvim_create_autocmd('WinClosed', {
    group   = group,
    pattern = tostring(win),
    once    = true,
    callback = function()
      if _state.win == win then
        _state.win = nil
        _state.buf = nil
      end
      vim.api.nvim_del_augroup_by_name('MdviewCleanup_' .. win)
    end,
  })
end

return M
