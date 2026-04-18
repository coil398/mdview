# mdview

Markdown ビューア。TUI・Electron GUI・JSON 出力など複数のフロントエンドに対応したワークスペース構成。

## クレート構成

| クレート / ディレクトリ | 役割 |
|---|---|
| `mdview-core` | パーサーライブラリ（ratatui 非依存、serde 対応、WASM ビルド可） |
| `mdview-tui` | TUI ビューア（`mdview` コマンド） |
| `mdview-json` | JSON 出力（外部ツール連携用、`mdview-json` コマンド） |
| `mdview-electron` | Electron GUI アプリ（WASM 経由で `mdview-core` を利用） |

## インストール

```bash
cargo install --git https://github.com/coil398/mdview mdview-tui
cargo install --git https://github.com/coil398/mdview mdview-json
```

Electron GUI は `cd mdview-electron && npm install && npm start` で起動する（事前に WASM ビルドが必要、下記「Electron」参照）。

## 使い方

### TUI

```bash
mdview <file.md>
```

#### キーバインド

| キー | 動作 |
|---|---|
| `q` / `Esc` | 終了 |
| `j` / `↓`, `k` / `↑` | スクロール / TOC カーソル移動 |
| `PageDown` / `PageUp` | ページスクロール |
| `g` / `G` | 先頭 / 末尾へ |
| `t` | TOC トグル |
| `Enter` | TOC 項目へジャンプ |
| `r` | 手動リロード |

ファイルを保存すると自動でリロードされる。

### Electron GUI

```bash
# 初回のみ: WASM をビルドして mdview-electron/wasm に配置
wasm-pack build mdview-core --target web --features wasm
cp mdview-core/pkg/{mdview_core_bg.wasm,mdview_core_bg.wasm.d.ts,mdview_core.js,mdview_core.d.ts,package.json} mdview-electron/wasm/

# 起動
cd mdview-electron && npm install && npm start
```

> wasm-pack の `--out-dir` フラグは cargo 1.86+ で動作しないため、デフォルト出力 `pkg/` を手動コピーする。詳細は `CLAUDE.md` を参照。

### JSON 出力（外部ツール連携）

```bash
mdview-json <file.md>
```

`Document` を JSON で標準出力する。外部ツールはこのバイナリをサブプロセスとして呼び出し、出力を受け取って独自の描画を行う。

```json
{
  "blocks": [
    {
      "Heading": {
        "level": 1,
        "spans": [{ "text": "Hello", "kind": "Normal" }]
      }
    },
    {
      "Paragraph": {
        "lines": [
          [{ "text": "world", "kind": "Normal" }]
        ]
      }
    }
  ],
  "toc": [
    { "block_index": 0, "title": "Hello", "level": 1 }
  ]
}
```

> **注意**: JSON スキーマは 2026-04-18 の B-2 rewrite で破壊的に変更されている（旧 `lines` / `line_index` 形式とは非互換）。`schema_version` 導入は次フェーズで実施予定。

#### Block 一覧

| Block | 意味 |
|---|---|
| `Paragraph { lines }` | 段落（HardBreak で区切られた行のリスト） |
| `Heading { level, spans }` | 見出し（レベル 1〜6、インライン Span 列を保持） |
| `List { ordered, start, items }` | リスト（`items: Vec<ListItem>` にネスト可） |
| `BlockQuote { blocks }` | 引用（再帰的にブロックを保持） |
| `CodeBlock { lang, code }` | コードブロック（言語指定あり） |
| `Table { header, rows, align }` | テーブル（セルは `Vec<Span>`、列ごとの整列指定付き） |
| `Rule` | 水平線 |

#### SpanKind 一覧（インラインのみ）

| kind | 意味 |
|---|---|
| `Normal` | 通常テキスト |
| `Bold` / `Italic` / `BoldItalic` | 強調 |
| `CodeInline` | インラインコード |
| `Link { url }` | リンク |

> 旧 `Heading` / `CodeBlock` / `BlockQuote` / `ListMarker` / `Rule` は Block 側に昇格したため、`SpanKind` からは削除されている。

## 開発

- ビルド: `cargo build --workspace`
- テスト: `cargo test --workspace`
- Lint: `cargo clippy --workspace`
- フォーマット: `cargo fmt --all`

詳細は `CLAUDE.md` を参照。

## 要件

- Rust 1.86+
- Electron: Node.js、wasm-pack
