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

# Electron .app ビルド（macOS arm64、ad-hoc 署名込み）
cd mdview-electron && npm run dist
# → dist/mac-arm64/mdview.app

# .app を /Applications/ にインストール（dist + cp + xattr -cr）
cd mdview-electron && npm run dist:install
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
- `syntect` のテーマはテーマ設定から選択（default は `base16-ocean.dark`）。テーマ ID → syntect テーマ名のマッピングは `theme.rs` の `TuiTheme::*()` コンストラクタを参照
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
- **Electron renderer のプロトコル**: Phase C（2026-04-19）以降、renderer は `file://` ではなく **`app://local/` カスタムスキーム** で起動する（`win.loadURL('app://local/renderer/index.html')`）。`file://` 配下の WASM fetch が sandbox 下で不確定な挙動をする問題を設計段階で排除するための切替。`protocol.registerSchemesAsPrivileged([{scheme: 'app', privileges: {standard: true, secure: true, supportFetchAPI: true}}])` を `app.whenReady()` 前に呼び、`app.whenReady().then()` 先頭で `protocol.handle('app', handler)` を登録する。handler 内で path traversal 防止のため `path.normalize(path.join(__dirname, relPath))` 後に `__dirname + path.sep` prefix check を必須とする
- **Electron セキュリティ設定**: Phase C で `sandbox: true` / `nodeIntegration: false` / `contextIsolation: true` / `webSecurity: true`（Electron デフォルト）の全保護を維持。`app://` スキームは `secure: true` で登録されているため CSP 的に `'self'` 扱いとなり、renderer 内の相対 import (`'../wasm/mdview_core.js'` 等) は通常 HTTP と同等の URL 解決で動作する
- **Electron CSP**: Phase C で `style-src 'self' 'unsafe-inline'` を `style-src 'self'; style-src-attr 'unsafe-inline'` に CSP Level 3 ディレクティブ分割。外部 `<style>` 要素の injection を禁止しつつ、Table の `style="text-align:..."` 属性のみ inline 許可する最小権限設計。highlight.js 11 ESM バンドルは DOM 要素に class 名のみ付与し inline style を生成しないため `style-src 'self'` で問題なし（将来 hljs v12+ にアップデートする際は `style.` / `cssText` / `setAttribute('style')` の有無を grep で再確認すること）
- **Electron マルチウィンドウ対応**: `main.js` の watcher / debounce 状態はシングルトン変数ではなく `WeakMap<WebContents, State>` で保持する（Phase C 導入）。`win.on('closed', ...)` 内で `win.webContents` にアクセスすると `Error: Object has been destroyed` が発生しうるため、`const wc = win.webContents` で事前にキャプチャしてから `stopWatching(wc)` を呼ぶパターンを必須とする
- **Electron ステータスバー**: TUI の `scroll+1/total` 行番号表示は Electron では意味がない（ピクセル単位スクロール）ため Phase C で `%` 表示のみ採用。全スクロール経路（キーバインド・マウスホイール・TOC クリック・`scrollIntoView`）は `#content` の単一 `scroll` イベントリスナーで `updateStatusBar()` を呼ぶ DRY 設計
- **Electron .app パッケージング（2026-04-30）**: `electron-builder` v26 を採用。設定は `mdview-electron/package.json` の `"build"` キーに集約（外部設定ファイルなし）。`npm run dist` → `dist/mac-arm64/mdview.app` を生成。`predist` フックで `build:assets`（hljs / themes / mermaid copy + WASM ビルド）を自動実行。`mac.identity = null` で electron-builder の署名は明示スキップし、後段で `codesign --force --deep --sign - dist/mac-arm64/mdview.app` を chained で呼んで **ad-hoc 署名**する 2 段構え（macOS 13+ では arm64 `.app` は ad-hoc でも何らかの署名が必須なため、`identity: null` のままだと「壊れている」エラーで起動できない）。`asarUnpack: ["wasm/**/*"]` で WASM を asar 外に出して `WebAssembly.instantiateStreaming` の MIME チェック問題を回避。`npm run dist:install` は `dist` の後に `rm -rf /Applications/mdview.app && cp -R ... && xattr -cr /Applications/mdview.app` まで一気にやって Spotlight 起動可能にする
- **Electron の runtime 依存ゼロ原則**: `mdview-electron/package.json` の `dependencies` は **空**（`electron` / `mermaid` / `@highlightjs/cdn-assets` を含めて全て `devDependencies`）。理由は 2 つ: (1) electron-builder は `dependencies` セクションに `electron` があるとビルドエラーで止める仕様、(2) renderer が使う npm パッケージはビルド時に `renderer/vendor/` 配下にコピー済みなので、`.app` 同梱の `app.asar` には node_modules を含める必要がない。`build.files` でも `node_modules/**/*` を指定していないため、サイズは renderer/vendor + wasm のみ
- **テーマ機能（Phase1: 2026-04-19）**: `~/.config/mdview/config.json` に `{"schema_version":1,"theme":"<id>"}` を書いて TUI / Electron 両方でテーマを切り替えられる。有効 ID は `vscode-dark`（default） / `vscode-light` / `github-dark` / `github-light`。未知 ID は warn + default にフォールバック。Phase2 で `solarized-*` / `tokyo-night-*` / `iceberg-*` を追加予定
- **Electron 見出しメモ機能 MVP（2026-04-19）**: `~/.config/mdview/notes.json` に `{schema_version, buckets: {filePath: [{heading_text, heading_level, occurrence_index, body, updated_at}]}}` 形式で保存する。`config.json` の `schema_version` は v1 → v2 に bump（読み込み時のみ互換補完し、即時書き込みしない）。**アンカーキーは `WeakMap<HTMLElement, AnchorKey>` で renderer 側に保持**し DOM 属性に漏らさない（区切り文字エスケープ・GC 自動解放の利点）。DOM 生成は `renderDocument(doc)` で innerHTML 代入 → `querySelectorAll('h1..h6')` で拾い `collectHeadingMeta` と zip 対応付け。**書き込み頻度が高い `notes.json` のみ atomic write（tmp→rename、tmp 名に `process.pid + Math.random().toString(36)` を含める）を採用**し、頻度の低い `config.json` は通常 write のまま。IPC は `notes:get` / `notes:set` の 2 チャネルで、`authorizeNotesAccess(event, filePath)` で `watcherStates.get(event.sender).watchedPath` と **strict equal 比較**してから処理する。**ファイルオープン経路は 2 系統（`onFileOpened` IPC / `open-btn` クリック）あり、両方に `flushPendingNote` / `notes.get` / `currentHeadingKey=null` + `updateCurrentHeading` の同一前処理を対称に適用する**必要がある（片方だけ実装するのは NG）。**notes:set の IPC ハンドラは `authorizeNotesAccess` 直後に `Array.isArray(payload.entries)` ガードを置く**（`validateNotesEntries(undefined)` は `[]` を返すため、ガードなしで渡すとファイルの全メモが消える）。`updateCurrentHeading` は `getBoundingClientRect()` をループ内で呼ぶため `requestAnimationFrame` で 1 フレーム 1 回に throttle する。preload API は他ドメインがフラットな中 `mdview.notes.{get,set}` の 1 階層ネストを採用（将来 `list` / `delete` 追加の拡張性を根拠に合意済み）

### テーマ機能メンテナンスガイド

#### 新規テーマを追加するときの手順

1. **`mdview-tui/src/theme.rs`**: `TuiTheme::from_id` の `match` 分岐に `"new-theme-id" => Self::new_theme()` を追加し、`new_theme()` コンストラクタを実装する。`syntect_theme` フィールドには `ThemeSet::load_defaults()` の実測キー名を使うこと（下記確認方法を参照）
2. **`mdview-electron/renderer/renderer.js`**: `THEME_REGISTRY` に `'new-theme-id': { cssVars: {...}, hljsCss: 'vendor/themes/hljs/xxx.css', background: '#...' }` を追加する
3. **`mdview-electron/main.js`**: `THEME_BACKGROUNDS` と `VALID_THEME_IDS` に追加し、テーマメニューの `themeSubmenu` に radio 項目を追加する
4. **`mdview-electron/package.json`**: `copy:themes` スクリプトに対応する hljs CSS ファイル名を追加する
5. **`README.md`**: テーマ一覧の表を更新する

#### syntect テーマ名の確認方法

```rust
// mdview-tui の任意のテストに下記を追記して cargo test -- --nocapture で実行
let ts = syntect::highlighting::ThemeSet::load_defaults();
let mut keys: Vec<&str> = ts.themes.keys().map(|s| s.as_str()).collect();
keys.sort();
for k in &keys { println!("THEME_KEY: {}", k); }
```

Phase1 時点の実測値（全 7 件）:
- `InspiredGitHub`
- `Solarized (dark)` / `Solarized (light)`
- `base16-eighties.dark` / `base16-mocha.dark`
- `base16-ocean.dark` / `base16-ocean.light`

Phase1 ID → syntect テーマ名マッピング:
- `vscode-dark` → `base16-ocean.dark`
- `vscode-light` → `base16-ocean.light`
- `github-dark` → `base16-eighties.dark`（近似）
- `github-light` → `InspiredGitHub`

#### hljs CSS ファイル名の確認方法

```bash
ls mdview-electron/node_modules/@highlightjs/cdn-assets/styles/ | grep -iE "vs|github|solarized|tokyo"
```

Phase1 で採用した 4 ファイル（実在確認済み）:
- `vs.css`（VS Code Light）/ `vs2015.css`（VS Code Dark）
- `github.css`（GitHub Light）/ `github-dark.css`（GitHub Dark）
