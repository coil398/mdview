---
name: add-theme
description: mdview に新規テーマを追加する。theme.rs / THEME_CYCLE / THEME_REGISTRY / main.js / package.json / README.md の6箇所更新と WCAG AA 5ペアのコントラスト確認を一括で行う。「テーマ追加」「新しいテーマ」「tokyo-night 入れて」等の要望に使う。
---

# add-theme — mdview テーマ追加ワークフロー

mdview に新規テーマを追加する。テーマ定義は **Rust（TUI）と JS（Electron）の6ファイルにまたがる** ため、1箇所でも漏らすとコンパイルエラーや T キー循環の不整合が起きる。本スキルは抜け漏れを防ぐチェックリストとして機能する。

> **SSOT**: 手順の正典はリポジトリの `CLAUDE.md`「テーマ機能メンテナンスガイド」。本スキルはそれを実行可能なチェックリストに落としたもの。詳細・最新の注意書きは必ず `CLAUDE.md` 側を確認すること。

## 入力

ユーザーから「テーマ ID」（例 `tokyo-night-dark`）と light/dark の別、ベースにする配色を聞く。不明なら確認する。

## 手順（全ステップ完了まで止まらない）

新規テーマ ID を `<id>` とする。

- [ ] **1. `mdview-tui/src/theme.rs`**: `TuiTheme::from_id` の `match` に `"<id>" => Self::<name>()` を追加し、`<name>()` コンストラクタを実装する。`syntect_theme` フィールドは `ThemeSet::load_defaults()` の実測キー名を使う（確認方法は CLAUDE.md「syntect テーマ名の確認方法」）。
- [ ] **2. `mdview-tui/src/app.rs`**: `THEME_CYCLE` 定数に同じ `<id>` を追加する（`from_id` と `THEME_CYCLE` は独立した2箇所。片方だけだと `T` キー循環が壊れる）。
- [ ] **3. `mdview-electron/renderer/renderer.js`**: `THEME_REGISTRY` に `'<id>': { cssVars: {...}, hljsCss: 'vendor/themes/hljs/xxx.css', background: '#...' }` を追加する。検索ハイライト色 `--search-*` も忘れず含める。
- [ ] **4. `mdview-electron/main.js`**: `THEME_BACKGROUNDS` と `VALID_THEME_IDS` に追加し、`themeSubmenu` に radio 項目を追加する。
- [ ] **5. `mdview-electron/package.json`**: `copy:themes` に対応する hljs CSS ファイル名を追加する。公式 CDN 未収録テーマは自前 CSS を `renderer/vendor/themes/hljs/<id>.css` に手書きし `git add -f` で commit する（`copy:themes` には含めない。詳細は CLAUDE.md「自前カスタム hljs CSS の追加手順」）。
- [ ] **6. `README.md`**: テーマ一覧の表を更新する。
- [ ] **7. WCAG AA コントラスト確認**: 後述の `wcag.py` で **5ペア**（`statusbar` / `toc_highlight` / `code_badge` / `search_match` / `search_current` の各 fg/bg）が 4.5:1 以上か確認する。未達なら色値を調整して再確認する。
- [ ] **8. ビルド確認**: `cargo build --workspace`（全6+テーマコンストラクタのフィールド漏れを検出）と `cd mdview-electron && node --check renderer/renderer.js`。

## WCAG AA コントラスト確認

このスキルに同梱の `wcag.py` を使う。fg/bg のペアを並べて渡す:

```bash
python3 "$CLAUDE_PROJECT_DIR/.claude/skills/add-theme/wcag.py" \
  "<statusbar_fg>" "<statusbar_bg>" \
  "<toc_highlight_fg>" "<toc_highlight_bg>" \
  "<code_badge_fg>" "<code_badge_bg>" \
  "<search_match_fg>" "<search_match_bg>" \
  "<search_current_fg>" "<search_current_bg>"
```

1ペアでも `FAIL` が出たら色値を調整して全ペア `PASS` になるまで繰り返す。TUI（`theme.rs`）と Electron（`THEME_REGISTRY`）で同一色値を使うこと。

## 完了報告

更新した6ファイル + WCAG 確認結果（各ペアの比率）+ ビルド結果をユーザーに報告する。
