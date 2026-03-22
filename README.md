# mdview

Markdown ビューア。TUI・Neovim・Electron など複数のフロントエンドに対応したワークスペース構成。

## クレート構成

| クレート | 役割 |
|---|---|
| `mdview-core` | パーサーライブラリ（ratatui 非依存、serde 対応） |
| `mdview-tui` | TUI ビューア（`mdview` コマンド） |
| `mdview-json` | JSON 出力（外部ツール連携用） |

## インストール

```bash
cargo install --git https://github.com/coil398/mdview mdview-tui
cargo install --git https://github.com/coil398/mdview mdview-json
```

## 使い方

### TUI

```bash
mdview <file.md>
```

#### キーバインド

| キー | 動作 |
|---|---|
| `q` / `Esc` | 終了 |
| `j` / `↓`, `k` / `↑` | スクロール |
| `PageDown` / `PageUp` | ページスクロール |
| `g` / `G` | 先頭 / 末尾へ |
| `t` | TOC トグル |
| `Enter` | TOC 項目へジャンプ |
| `r` | 手動リロード |

ファイルを保存すると自動でリロードされる。

### JSON 出力（Neovim・Electron 連携）

```bash
mdview-json <file.md>
```

`Document` を JSON で標準出力する。外部ツールはこのバイナリをサブプロセスとして呼び出し、出力を受け取って独自の描画を行う。

```json
{
  "lines": [
    [{ "text": "Hello", "kind": { "Heading": 1 } }]
  ],
  "toc": [
    { "line_index": 0, "title": "Hello", "level": 1 }
  ]
}
```

#### SpanKind 一覧

| kind | 意味 |
|---|---|
| `Normal` | 通常テキスト |
| `Heading(n)` | 見出し（レベル 1〜6） |
| `Bold` / `Italic` / `BoldItalic` | 強調 |
| `CodeInline` | インラインコード |
| `CodeBlock { lang }` | コードブロック（言語付き） |
| `Link { url }` | リンク |
| `ListMarker` | リストの bullet |
| `BlockQuote` | 引用 |
| `Rule` | 水平線 |

## 要件

- Rust 1.86+
