-- plugin/mdview.lua
-- :Mdview コマンドを登録する。実際のロジックは lua/mdview/ に委譲する（遅延ロード）。

if vim.g.loaded_mdview then
  return
end
vim.g.loaded_mdview = true

vim.api.nvim_create_user_command('Mdview', function(args)
  local path = args.args ~= '' and args.args or nil
  require('mdview').open({
    path = path,
    force = args.bang,
  })
end, {
  bang = true,
  nargs = '?',
  complete = 'file',
  desc = 'Open the current (or specified) Markdown file in a floating mdview TUI window',
})
