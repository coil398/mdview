# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## コマンド

```bash
# ビルド
cargo build --workspace

# リリースビルド
cargo build --workspace --release

# TUI 実行
cargo run -p mdview-tui -- <path/to/file.md>

# JSON 出力
cargo run -p mdview-json -- <path/to/file.md>

# テスト
cargo test --workspace

# 特定テスト
cargo test -p mdview-core <test_name>

# Lint
cargo clippy --workspace

# フォーマット
cargo fmt --all
```

バイナリ名は `mdview`（TUI）と `mdview-json`（JSON出力）。`cargo install` 後はそれぞれ直接起動可能。

## アーキテクチャ

### 全体構造

Cargo ワークスペース構成。`mdview-core` でパース、`mdview-tui` で TUI 表示、`mdview-json` で JSON 出力。

```
mdview-core/        # ライブラリクレート（ratatui 非依存）
  src/
    lib.rs
    types.rs        # Span, SpanKind, Line, Document, TocEntry（serde付き）
    parser.rs       # parse_markdown(): MD → Document 変換

mdview-tui/         # TUI バイナリ
  src/
    main.rs         # CLIエントリポイント（clap）
    lib.rs          # モジュール公開
    app.rs          # App構造体: イベントループ・レイアウト・状態管理
    types.rs        # StyledSpan, StyledLine
    style.rs        # SpanKind → ratatui::Style 変換・Document → StyledLine 変換
    highlighter.rs  # Highlighter: syntectによるコードハイライト
    ui/
      viewer.rs     # 本文描画
      toc.rs        # TOCサイドパネル描画
      statusbar.rs  # ステータスバー描画
    watcher.rs      # FileWatcher: ファイル変更検知

mdview-json/        # JSON 出力バイナリ（neovim/electron 連携用）
  src/
    main.rs         # ファイル読み込み → parse_markdown → stdout JSON
```

### データフロー

1. `parse_markdown(text)` → `Document { lines: Vec<Line>, toc: Vec<TocEntry> }`
2. `Line = Vec<Span>` — テキストとセマンティック種別（`SpanKind`）のペア
3. **mdview-tui**: `convert_document(&doc, &hl)` → `Vec<StyledLine>` に変換して ratatui で描画
4. **mdview-json**: `serde_json::to_string(&doc)` → stdout 出力
5. メインループ: ファイル変更 (`reload_rx`) を50msポーリング → 再ロード → 描画

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
