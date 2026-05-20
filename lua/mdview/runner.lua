-- lua/mdview/runner.lua
-- mdview バイナリの検出・パス解決・termopen 実行を担当する。

local M = {}

--- bin が PATH 上で実行可能かチェックする。
---@param bin string
---@return boolean
function M.find_bin(bin)
  return vim.fn.executable(bin) == 1
end

--- :Mdview で開くファイルパスを決定する。
--- opts.path 指定 > カレントバッファのパス の優先順。
--- バッファに未保存の変更がある場合は force に応じて処理する。
---
---@param opts { path?: string, force?: boolean }
---@return string|nil path      成功時はパス文字列
---@return string|nil tmp_path  force で tmp を作成した場合のパス（TermClose で削除する）
---@return string|nil err       エラーメッセージ（nil なら成功）
function M.resolve_path(opts)
  -- 引数パス指定がある場合は存在確認してから返す
  if opts.path and opts.path ~= '' then
    if vim.fn.filereadable(opts.path) ~= 1 then
      return nil, nil, 'mdview: ファイルが見つかりません: ' .. opts.path
    end
    return opts.path, nil, nil
  end

  local bufnr = vim.api.nvim_get_current_buf()
  local bufname = vim.api.nvim_buf_get_name(bufnr)

  if bufname == '' then
    return nil, nil, 'mdview: バッファにファイル名がありません。ファイルを保存してから実行してください。'
  end

  -- 未保存の変更チェック
  local modified = vim.api.nvim_get_option_value('modified', { buf = bufnr })
  if modified then
    if not opts.force then
      return nil, nil,
        'mdview: バッファに未保存の変更があります。`:w` で保存するか `:Mdview!` で強制表示してください。'
    end

    -- force=true: tmpfile に書き出す
    local tmp = vim.fn.tempname() .. '.md'
    local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
    local ok, write_err = pcall(function()
      vim.fn.writefile(lines, tmp)
    end)
    if not ok then
      return nil, nil, 'mdview: 一時ファイルへの書き出しに失敗しました: ' .. tostring(write_err)
    end
    return tmp, tmp, nil
  end

  return bufname, nil, nil
end

--- floating window の buf 上で termopen を実行する。
--- TermClose autocmd で window を close し、tmp ファイルがあれば削除する。
---
---@param buf      integer          termopen 対象バッファ
---@param win      integer          floating window（close 対象）
---@param bin      string           mdview バイナリパス
---@param path     string           開くファイルパス
---@param tmp_path string|nil       TermClose で削除する一時ファイル（nil なら削除なし）
---@param auto_close boolean        TermClose 時に win を close するか
---@param window_mod table          lua/mdview/window モジュール（cleanup 用）
function M.launch(buf, win, bin, path, tmp_path, auto_close, window_mod)
  -- termopen は current window が buf を表示している状態で呼ぶ必要がある
  vim.api.nvim_set_current_win(win)
  vim.api.nvim_set_current_buf(buf)

  --- termopen 失敗時に floating window とバッファを確実にクリーンアップする。
  local function cleanup_on_error(err_msg)
    window_mod.close(win)
    if vim.api.nvim_buf_is_valid(buf) then
      vim.api.nvim_buf_delete(buf, { force = true })
    end
    if tmp_path then
      vim.fn.delete(tmp_path)
    end
    vim.notify('mdview: termopen に失敗しました: ' .. tostring(err_msg), vim.log.levels.ERROR)
  end

  local ok, job_id = pcall(vim.fn.termopen, { bin, path }, {
    on_exit = function(_, _, _)
      -- TermClose は非同期なので vim.schedule でメインループに委譲
      vim.schedule(function()
        if tmp_path then
          vim.fn.delete(tmp_path)
        end
        if auto_close then
          if vim.api.nvim_win_is_valid(win) then
            vim.api.nvim_win_close(win, true)
          end
        end
      end)
    end,
  })

  -- pcall が例外を捕捉した場合
  if not ok then
    cleanup_on_error(job_id)
    return
  end

  -- job_id <= 0 は termopen の失敗（-1: バイナリ実行不可、0: 無効な引数）
  if job_id <= 0 then
    cleanup_on_error('job_id が無効です (job_id=' .. tostring(job_id) .. ')')
    return
  end

  -- terminal バッファをすぐ terminal モードに
  vim.cmd('startinsert')
end

return M
