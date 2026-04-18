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

# WASM ビルド（cargo 1.86+ では --out-dir が unstable フラグに衝突するため 2 ステップ）
wasm-pack build mdview-core --target web --features wasm
cp mdview-core/pkg/{mdview_core_bg.wasm,mdview_core_bg.wasm.d.ts,mdview_core.js,mdview_core.d.ts,package.json} mdview-electron/wasm/

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
    types.rs        # Block, Span, SpanKind, Document, TocEntry（serde付き）
    parser.rs       # parse_markdown(): MD → Document 変換（Block ツリー構造）

mdview-tui/         # TUI バイナリ
  src/
    main.rs         # CLIエントリポイント（clap）
    lib.rs          # モジュール公開
    app.rs          # App構造体: イベントループ・レイアウト・状態管理
    types.rs        # StyledSpan, StyledLine
    style.rs        # SpanKind → ratatui::Style 変換・Document → StyledOutput 変換
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

1. `parse_markdown(text)` → `Document { blocks: Vec<Block>, toc: Vec<TocEntry> }`
2. `Block` は再帰的ツリー構造（Paragraph/Heading/CodeBlock/BlockQuote/List/Table/Rule）。各 Block が `Vec<Span>` を保持
3. `TocEntry.block_index` は `blocks` 配列内の対応 Heading の index
4. **mdview-tui**: `convert_document(&doc, &hl)` → `StyledOutput { lines, block_starts, toc }` に変換して ratatui で描画。`block_starts[i]` は `blocks[i]` の先頭行番号（TOC ジャンプに使用）
5. **mdview-json**: `serde_json::to_string(&doc)` → stdout 出力（JSON スキーマは B-2 で破壊的変更あり。旧スキーマとは非互換）
6. メインループ: ファイル変更 (`reload_rx`) を50msポーリング → 再ロード → 描画

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
- pulldown-cmark の **tight list** では `Tag::Item` 直下に `Tag::Paragraph` が来ないことがある。parser.rs では「暗黙 Paragraph」として Item builder 上に Paragraph builder を push し、次のブロック系イベントで flush する方式で対応している
- `wasm-pack 0.13/0.14` + cargo 1.86+ では `--out-dir` が unstable `--artifact-dir` にマップされてエラーになる。`--out-dir` を省略してデフォルト `pkg/` に出力し、必要ファイルを `mdview-electron/wasm/` にコピーすること
- JSON スキーマは B-2 rewrite（2026-04-18）で破壊的変更済み。neovim 連携の再対応は次フェーズ D-4 で実施予定
- TUI Table の列幅はフェーズ6（2026-04-18）から動的幅に変更済み。`unicode-width 0.2.2` を `mdview-tui` に導入し、各列を `clamp(3, 40)` した display width ベースで計算する（`TABLE_COL_MIN_WIDTH = 3` / `TABLE_COL_MAX_WIDTH = 40`）
- `Document.schema_version = 2`（`mdview-core::types::SCHEMA_VERSION` 定数）。フェーズ2（2026-04-18）で導入。旧 v1 との互換なし
- `mdview-json` は stdin 対応・`--compact` フラグ対応済み（フェーズ2）。`file` 引数省略で stdin を読む
- WASM API `parse_markdown_to_json` の返却形式は `{"ok": {...Document...}}` または `{"error": {"kind": "...", "message": "..."}}` の discriminated union（フェーズ2以降）。旧形式（成功時に直接 Document を返す）は廃止
- `ratatui 0.30` の `Paragraph::line_count(width)` は `#[instability::unstable]` によりプライベート API（`E0624`）。フェーズ6で `estimate_wrapped_line_count`（`viewer.rs` 内 private 関数）を自前実装し、`viewer::render` が wrap 後推定行数を返すよう変更済み。この関数は ratatui の `WordWrapper` とは word-boundary で数行ズレうるが、スクロール上限計算には十分な精度（仕様内）
- フェーズ間繰越しの運用: planner が実装困難・不確実な問題を意図的に flag し、fallback 実装で暫定対応 → 次フェーズで取り直す運用パターンが定着。フェーズ2バグ2（`G` 末尾ズレ）はこの流れでフェーズ6で解消した
- 絵文字（特に `🚀` U+1F680 など）は Unicode 標準では East Asian Width = W（幅 2）だが、端末エミュレータによっては幅 1 で描画されることがある（Tabby / Windows Terminal の一部バージョン等）。mdview は `unicode-width` の返す Unicode 標準値を信頼する設計で、端末側の描画差異は対応範囲外とする。絵文字 Table の崩れは端末のフォント・設定で解決することを推奨
