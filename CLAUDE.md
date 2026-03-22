# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## コマンド

```bash
# ビルド
cargo build

# リリースビルド
cargo build --release

# 実行
cargo run -- <path/to/file.md>

# テスト
cargo test

# 特定テスト
cargo test <test_name>

# Lint
cargo clippy

# フォーマット
cargo fmt
```

バイナリ名は `mdview`（`cargo install` 後は `mdview <file.md>` で起動）。

## アーキテクチャ

### 全体構造

Rust製のTUIアプリ。`ratatui` + `crossterm` で描画・入力、`pulldown-cmark` でMarkdownパース、`syntect` でコードハイライト、`notify-debouncer-full` でファイル監視を行う。

```
src/
  main.rs          # CLIエントリポイント（clap）
  lib.rs           # モジュール公開
  app.rs           # App構造体: イベントループ・レイアウト・状態管理
  types.rs         # StyledSpan, StyledLine, TocEntry
  parser/
    mod.rs         # pub use
    markdown.rs    # parse_markdown(): MD→StyledLine+TocEntry変換
    highlighter.rs # Highlighter: syntectによるコードハイライト
  ui/
    mod.rs
    viewer.rs      # 本文描画
    toc.rs         # TOCサイドパネル描画
    statusbar.rs   # ステータスバー描画
  watcher.rs       # FileWatcher: ファイル変更検知
```

### データフロー

1. `App::new()` → `FileWatcher` 起動・`parse_markdown()` でロード
2. `parse_markdown()` → `Vec<StyledLine>` と `Vec<TocEntry>` を返す
3. `StyledLine = Vec<StyledSpan>` — テキストと `ratatui::Style` のペア
4. メインループ: ファイル変更 (`reload_rx`) を50msポーリング → 再ロード → 描画

### キーバインド

| キー | 動作 |
|------|------|
| `q` / `Esc` | 終了 |
| `j` / `↓`, `k` / `↑` | スクロール / TOCカーソル移動 |
| `PageDown` / `PageUp` | ページスクロール |
| `g` / `G` | 先頭 / 末尾へ |
| `t` | TOCトグル |
| `r` | 手動リロード |
| `Enter` | TOC項目へジャンプ |

### 注意点

- ファイル監視は **親ディレクトリ** を非再帰的に監視する（エディタのrename+create方式の保存に対応するため）
- `syntect` のテーマは `base16-ocean.dark` を使用
- コードブロックの背景色は設定しない（ターミナルのデフォルト背景を壊さないため）
- Rust edition 2021 / MSRV 1.86
