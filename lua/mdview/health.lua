-- lua/mdview/health.lua
-- :checkhealth mdview 用のヘルスチェック。

local M = {}

-- neovim 0.10+ は vim.health.start、それ以前は vim.health.report_start
local health = vim.health or require('health')
local h_start = health.start or health.report_start
local h_ok = health.ok or health.report_ok
local h_error = health.error or health.report_error
local h_warn = health.warn or health.report_warn
local h_info = health.info or health.report_info

function M.check()
  h_start('mdview')

  -- 設定の取得（setup 済みであれば）
  local cfg = require('mdview').config()
  local bin = cfg.bin

  -- バイナリ検出
  if vim.fn.executable(bin) ~= 1 then
    h_error(('`%s` が PATH 上に見つかりません。'):format(bin), {
      'cargo install --path mdview-tui  (リポジトリルートから)',
      'または setup({ bin = "/絶対/パス/mdview" }) でパスを指定してください。',
    })
    return
  end

  h_ok(('`%s` が見つかりました: %s'):format(bin, vim.fn.exepath(bin)))

  -- バージョン取得
  local result = vim.fn.system({ bin, '--version' })
  local code = vim.v.shell_error
  if code ~= 0 or result == '' then
    h_warn(
      '`'
        .. bin
        .. ' --version` の実行に失敗しました（終了コード: '
        .. tostring(code)
        .. '）'
    )
  else
    local version = vim.trim(result)
    h_ok('バージョン: ' .. version)
  end

  -- neovim バージョン確認（0.9+ 推奨）
  if vim.fn.has('nvim-0.9') == 1 then
    h_ok('neovim バージョン: OK (0.9+)')
  else
    h_warn('neovim 0.9 未満です。floating window のタイトルが表示されません。')
  end
end

return M
