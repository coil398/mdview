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

# WASM ビルド
wasm-pack build mdview-core --target web --features wasm --out-dir mdview-electron/wasm

# Electron 起動（WASM ビルド済みの場合）
cd mdview-electron && npm install && npm start

# Electron 開発（WASM ビルドから起動）
cd mdview-electron && npm run dev
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

mdview-electron/    # Electron GUI アプリ（WASM 経由で mdview-core を利用）
  main.js           # Electron メインプロセス（ウィンドウ管理・ファイル読み込み・メニュー）
  preload.js        # コンテキストブリッジ（IPC API を renderer に公開）
  renderer/
    index.html      # メインウィンドウ HTML
    renderer.js     # WASM 読み込み・Markdown レンダリング・TOC 構築
    style.css       # Catppuccin Mocha テーマ CSS
  wasm/             # wasm-pack ビルド成果物（.gitignore 対象）
  package.json      # npm パッケージ定義（electron・highlight.js）
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
- mdview-core の WASM ビルドには `--features wasm` が必要（`wasm-bindgen` は optional dependency）
- `crate-type = ["lib", "cdylib"]` で通常 Rust ライブラリと WASM の両方を同時提供
- Electron renderer は `file://` プロトコルで動作するため、highlight.js は ESM の `import()` で `node_modules/highlight.js/es/index.js` を読み込む（CommonJS の `<script>` タグ読み込みは不可）
- Electron IPC ではファイルパスをユーザー入力から直接受け取らない（`dialog.showOpenDialog` 経由のみ）。`file:read` のような汎用 IPC は作らない
- リンク URL は `https://`、`http://`、`mailto:` のみ許可し、`javascript:` スキーム等は `#` に置換する
- `ipcRenderer.on` は登録前に `removeAllListeners` を呼ぶか、一度限りの登録にしてリスナー蓄積を防ぐ
- `SpanKind` の serde シリアライズは enum タグ形式（文字列 or オブジェクト）。JS 側では `typeof kind === 'string'` で判定
