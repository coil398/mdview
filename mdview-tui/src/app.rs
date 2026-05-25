use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Frame;
use ratatui_textarea::TextArea;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

use mdview_core::parser::{collect_heading_anchors, parse_markdown};
use mdview_core::{AnchorKey, Block, TocEntry};

use crate::highlighter::Highlighter;
use crate::notes;
use crate::search::{self, SearchMatch};
use crate::style::{convert_document, StyledOutput};
use crate::theme::TuiTheme;
use crate::types::StyledLine;
use crate::ui::{notes as ui_notes, statusbar, toc, viewer};
use crate::watcher::FileWatcher;

/// テーマランタイム切替の循環リスト（前フェーズ phase.rs の定義順に揃える）。
///
/// **このリストを変更する場合は `mdview-tui/src/theme.rs` の `TuiTheme::from_id` の
/// match 分岐も同時に更新すること。** また CLAUDE.md「テーマ機能メンテナンスガイド /
/// 新規テーマを追加するときの手順」step 1 も参照。
const THEME_CYCLE: &[&str] = &[
    "vscode-dark",
    "vscode-light",
    "github-dark",
    "github-light",
    "solarized-dark",
    "solarized-light",
];

pub struct App {
    pub filepath: PathBuf,
    pub lines: Vec<StyledLine>,
    pub toc: Vec<TocEntry>,
    /// `Document.blocks[i]` の描画開始行 index。TOC ジャンプに使う。
    pub block_starts: Vec<usize>,
    pub scroll: usize,
    pub toc_open: bool,
    pub toc_sel: usize,
    pub highlighter: Arc<Highlighter>,
    pub reload_rx: Receiver<()>,
    _watcher: FileWatcher,
    /// 最後に描画した wrap 後行数。max_scroll 計算に使う。
    pub wrapped_line_count: usize,
    /// リロードエラーメッセージと表示開始時刻。5 秒後に自動クリア。
    pub status_error: Option<(String, std::time::Instant)>,
    /// 現在適用中のテーマ。
    pub theme: TuiTheme,
    /// `THEME_CYCLE` 配列における現在のテーマインデックス。
    pub theme_index: usize,

    // ── メモ機能フィールド ──────────────────────────────────────────────
    /// 現在開いている文書の全見出しアンカー（GUI collectHeadingMeta 互換、List/BlockQuote 内も含む）。
    pub anchors: Vec<AnchorKey>,
    /// `anchors[i]` が対応するトップレベル `Document.blocks` の index。
    /// `anchors` と件数は必ずしも一致しない（リスト内 Heading は block_index を持てない）ため、
    /// トップレベル Heading のみを対象とした別配列として保持する。
    pub anchor_block_indices: Vec<usize>,
    /// トップレベル Heading のみの anchor 配列（`block_starts` と対応させるため分離）。
    pub toplevel_anchors: Vec<AnchorKey>,
    /// メモパネル開閉フラグ。
    pub notes_open: bool,
    /// 編集モード（Insert）かどうか。
    pub notes_edit_mode: bool,
    /// notes 永続化ストア（全ファイル分のメモ）。
    pub notes_store: notes::NotesStore,
    /// 現フォーカス見出し（スクロール位置より前の最も近い Heading）。
    /// `None` = スクロール位置より前に見出しなし。
    pub current_anchor: Option<AnchorKey>,
    /// ratatui-textarea 編集バッファ。Insert モード時のみ意味を持つ。
    pub notes_textarea: TextArea<'static>,

    // ── 検索機能フィールド ──────────────────────────────────────────────
    /// コマンドライン検索入力中フラグ（`/` または `Ctrl+F` 入力中）。
    pub search_mode: bool,
    /// 確定済みの検索クエリ（`n`/`N` ジャンプの対象）。
    pub search_query: String,
    /// 入力中のバッファ（`search_mode` 中に編集、Enter で `search_query` に確定）。
    pub search_input: String,
    /// 確定クエリのマッチ全件。
    pub search_matches: Vec<SearchMatch>,
    /// 現在フォーカス中のマッチ index（`search_matches` 内）。
    pub search_cursor: usize,
}

impl App {
    pub fn new(path: PathBuf, theme: TuiTheme, notes_panel_open: bool) -> Result<Self> {
        let highlighter = Arc::new(
            Highlighter::with_syntect_theme(theme.syntect_theme).unwrap_or_else(|e| {
                eprintln!("mdview: syntect theme load failed: {}. using default.", e);
                Highlighter::new()
            }),
        );
        let (tx, rx) = mpsc::channel();
        let watcher = FileWatcher::new(path.clone(), tx)?;

        let theme_index = THEME_CYCLE
            .iter()
            .position(|&id| id == theme.id)
            .unwrap_or(0);

        let mut app = App {
            filepath: path,
            lines: Vec::new(),
            toc: Vec::new(),
            block_starts: Vec::new(),
            scroll: 0,
            toc_open: false,
            toc_sel: 0,
            highlighter,
            reload_rx: rx,
            _watcher: watcher,
            wrapped_line_count: 0,
            status_error: None,
            theme,
            theme_index,
            // メモ機能フィールド初期化
            anchors: Vec::new(),
            anchor_block_indices: Vec::new(),
            toplevel_anchors: Vec::new(),
            notes_open: notes_panel_open,
            notes_edit_mode: false,
            notes_store: notes::load(),
            current_anchor: None,
            notes_textarea: TextArea::default(),
            // 検索機能フィールド初期化
            search_mode: false,
            search_query: String::new(),
            search_input: String::new(),
            search_matches: Vec::new(),
            search_cursor: 0,
        };

        app.load()?;
        // 初期 scroll=0 時点での見出し検出
        app.refresh_current_anchor();
        if app.notes_open {
            app.load_textarea_for_current();
        }
        Ok(app)
    }

    pub fn load(&mut self) -> Result<()> {
        let text = std::fs::read_to_string(&self.filepath)?;
        let doc = parse_markdown(&text);

        // GUI 互換の全 anchor（List/BlockQuote 内も含む）を計算
        self.anchors = collect_heading_anchors(&doc.blocks);

        // トップレベル Heading のみの anchor と block_index を計算
        // occurrence_index は全 anchors から抽出して GUI と同じカウンタを維持する
        let mut toplevel_anchors = Vec::new();
        let mut anchor_block_indices = Vec::new();

        // collect_heading_anchors を一度走らせた結果から、トップレベル Heading を抽出する。
        // 各トップレベルブロックが再帰的に生成する anchor の数（offset）を正確に計算し、
        // anchors[anchor_offset] でトップレベル Heading に対応する anchor を特定する。
        // これにより「List 内 Heading が同名のトップレベル Heading より前に出現する場合でも
        // 正しい occurrence_index を持つ anchor が選ばれる」ことを保証する。
        {
            let mut anchor_offset = 0usize;
            for (block_index, block) in doc.blocks.iter().enumerate() {
                let anchor_count = count_anchors_in_block(block);
                if matches!(block, Block::Heading { .. }) {
                    // トップレベル Heading は anchors[anchor_offset] に対応する
                    // (anchor_count == 1 のはず)
                    if let Some(anchor) = self.anchors.get(anchor_offset) {
                        toplevel_anchors.push(anchor.clone());
                        anchor_block_indices.push(block_index);
                    }
                }
                anchor_offset += anchor_count;
            }
        }

        self.toplevel_anchors = toplevel_anchors;
        self.anchor_block_indices = anchor_block_indices;

        let StyledOutput {
            lines,
            block_starts,
            toc,
        } = convert_document(&doc, &self.highlighter, &self.theme);
        self.lines = lines;
        self.toc = toc;
        self.block_starts = block_starts;
        if self.toc_sel >= self.toc.len() {
            self.toc_sel = 0;
        }

        // ロード後に current_anchor を更新
        self.refresh_current_anchor();

        // 確定クエリがある場合は DOM 再構築後にマッチを再計算する
        if !self.search_query.is_empty() {
            match search::find_matches(&self.lines, &self.search_query) {
                Ok(matches) => {
                    self.search_matches = matches;
                    // カーソルが範囲外にならないようクランプ
                    if !self.search_matches.is_empty() {
                        self.search_cursor = self.search_cursor.min(self.search_matches.len() - 1);
                    } else {
                        self.search_cursor = 0;
                    }
                }
                Err(_) => {
                    self.search_matches.clear();
                    self.search_cursor = 0;
                }
            }
        }

        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        let mut terminal = ratatui::init();

        loop {
            // エラーメッセージの自動クリア（5 秒経過で消去）
            if let Some((_, t)) = &self.status_error {
                if t.elapsed() >= std::time::Duration::from_secs(5) {
                    self.status_error = None;
                }
            }

            // 毎ループでリロードチェック
            if self.reload_rx.try_recv().is_ok() {
                // 余分な通知を drain する
                while self.reload_rx.try_recv().is_ok() {}
                // 編集中ならメモを先に保存してからリロード
                self.flush_note_edit_mode();
                if let Err(e) = self.load() {
                    self.status_error =
                        Some((format!("Reload failed: {}", e), std::time::Instant::now()));
                }
                if self.notes_open {
                    self.load_textarea_for_current();
                }
            }

            // viewport_height を描画前に取得
            let viewport_height = terminal.size().map(|s| s.height as usize).unwrap_or(24);
            let content_height = viewport_height.saturating_sub(1); // ステータスバー分

            // スクロール上限クランプ（wrap 後行数ベース。初回描画前は document 行数でフォールバック）
            let max_scroll = if self.wrapped_line_count > 0 {
                self.wrapped_line_count.saturating_sub(content_height)
            } else {
                self.lines.len().saturating_sub(content_height)
            };
            self.scroll = self.scroll.min(max_scroll);

            // 描画。viewer::render の戻り値（wrap 後行数）を次ループの max_scroll 計算に使う
            let mut new_wrapped_line_count = 0usize;
            terminal.draw(|frame| {
                new_wrapped_line_count = self.render(frame);
            })?;
            self.wrapped_line_count = new_wrapped_line_count;

            // ノンブロッキング入力
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }

                    // 検索モード中は検索入力キーのみ処理（notes_edit_mode より前に置く）
                    if self.search_mode {
                        let max_scroll_for_search = if self.wrapped_line_count > 0 {
                            self.wrapped_line_count.saturating_sub(content_height)
                        } else {
                            self.lines.len().saturating_sub(content_height)
                        };
                        match key.code {
                            KeyCode::Esc => self.cancel_search(),
                            KeyCode::Enter => self.confirm_search(max_scroll_for_search),
                            KeyCode::Backspace => {
                                self.search_input.pop();
                            }
                            KeyCode::Char(c) => {
                                self.search_input.push(c);
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Insert モード中はメモ編集キーのみ処理
                    if self.notes_edit_mode {
                        match key.code {
                            KeyCode::Esc => {
                                // 編集モード離脱して保存
                                self.flush_note_edit_mode();
                            }
                            _ => {
                                // ratatui-textarea にキーを渡す
                                self.notes_textarea
                                    .input(ratatui_textarea::Input::from(key));
                            }
                        }
                        continue;
                    }

                    let max_scroll = if self.wrapped_line_count > 0 {
                        self.wrapped_line_count.saturating_sub(content_height)
                    } else {
                        self.lines.len().saturating_sub(content_height)
                    };

                    match key.code {
                        // `q` は終了、`Esc` はメモパネル内での編集中でなければ終了
                        KeyCode::Char('q') | KeyCode::Esc => break,

                        KeyCode::Char('j') | KeyCode::Down => {
                            if self.toc_open {
                                if !self.toc.is_empty() {
                                    self.toc_sel = (self.toc_sel + 1).min(self.toc.len() - 1);
                                }
                            } else {
                                self.scroll = (self.scroll + 1).min(max_scroll);
                                self.refresh_current_anchor();
                                if self.notes_open {
                                    self.load_textarea_for_current();
                                }
                            }
                        }

                        KeyCode::Char('k') | KeyCode::Up => {
                            if self.toc_open {
                                self.toc_sel = self.toc_sel.saturating_sub(1);
                            } else {
                                self.scroll = self.scroll.saturating_sub(1);
                                self.refresh_current_anchor();
                                if self.notes_open {
                                    self.load_textarea_for_current();
                                }
                            }
                        }

                        KeyCode::PageDown => {
                            self.scroll =
                                (self.scroll + content_height.saturating_sub(1)).min(max_scroll);
                            self.refresh_current_anchor();
                            if self.notes_open {
                                self.load_textarea_for_current();
                            }
                        }

                        KeyCode::PageUp => {
                            self.scroll =
                                self.scroll.saturating_sub(content_height.saturating_sub(1));
                            self.refresh_current_anchor();
                            if self.notes_open {
                                self.load_textarea_for_current();
                            }
                        }

                        KeyCode::Char('g') => {
                            self.scroll = 0;
                            self.refresh_current_anchor();
                            if self.notes_open {
                                self.load_textarea_for_current();
                            }
                        }

                        KeyCode::Char('G') => {
                            self.scroll = max_scroll;
                            self.refresh_current_anchor();
                            if self.notes_open {
                                self.load_textarea_for_current();
                            }
                        }

                        KeyCode::Char('T') => {
                            // Shift+T: テーマを順方向に循環
                            if let Err(e) = self.cycle_theme(true) {
                                self.status_error = Some((
                                    format!("theme error: {}", e),
                                    std::time::Instant::now(),
                                ));
                            }
                        }

                        KeyCode::Char('t') => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                // Ctrl+T: テーマを逆方向に循環
                                if let Err(e) = self.cycle_theme(false) {
                                    self.status_error = Some((
                                        format!("theme error: {}", e),
                                        std::time::Instant::now(),
                                    ));
                                }
                            } else {
                                self.toc_open = !self.toc_open;
                                // TOC を開くときは Notes を閉じる（排他制御）
                                if self.toc_open {
                                    // 編集中なら先に保存してから閉じる（plan.md Step E-1 対応）
                                    self.flush_note_edit_mode();
                                    self.notes_open = false;
                                }
                                if self.toc_open && self.toc_sel >= self.toc.len() {
                                    self.toc_sel = 0;
                                }
                            }
                        }

                        KeyCode::Char('/') => {
                            self.start_search();
                        }

                        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Ctrl+F: 検索開始（`/` と同等）。f 単独キーは _ アームで無処理。
                            self.start_search();
                        }
                        KeyCode::Char('n') => {
                            // TUI-7: `n` を検索次マッチに割り当て（メモトグルは `m` に移設）
                            self.next_match(max_scroll);
                        }

                        KeyCode::Char('N') => {
                            self.prev_match(max_scroll);
                        }

                        KeyCode::Char('m') => {
                            // TUI-7: メモパネルトグル（`n` から移設）
                            if self.notes_open {
                                // 編集中なら先に保存してから閉じる
                                self.flush_note_edit_mode();
                                // パネルを閉じる
                                self.notes_open = false;
                            } else {
                                // パネルを開く（TOC を閉じる）
                                self.notes_open = true;
                                self.toc_open = false;
                                self.refresh_current_anchor();
                                self.load_textarea_for_current();
                            }
                        }

                        // メモパネルが開いていて、見出しフォーカスがある場合のみ編集モードへ
                        KeyCode::Char('i') if self.notes_open && self.current_anchor.is_some() => {
                            self.notes_edit_mode = true;
                            self.load_textarea_for_current();
                        }

                        KeyCode::Char('r') => {
                            // 手動リロード前にメモを保存
                            self.flush_note_edit_mode();
                            if let Err(e) = self.load() {
                                self.status_error = Some((
                                    format!("Reload failed: {}", e),
                                    std::time::Instant::now(),
                                ));
                            }
                            if self.notes_open {
                                self.load_textarea_for_current();
                            }
                        }

                        KeyCode::Enter if self.toc_open && !self.toc.is_empty() => {
                            let entry = &self.toc[self.toc_sel];
                            let target_line = self
                                .block_starts
                                .get(entry.block_index)
                                .copied()
                                .unwrap_or(0);
                            self.scroll = target_line.min(max_scroll);
                            self.toc_open = false;
                            self.refresh_current_anchor();
                            if self.notes_open {
                                self.load_textarea_for_current();
                            }
                        }

                        KeyCode::Char('o') => {
                            // スクロール最上行のリンクを外部ブラウザで開く
                            self.open_focused_link();
                        }

                        _ => {}
                    }
                }
            }
        }

        ratatui::restore();
        Ok(())
    }

    fn render(&self, frame: &mut Frame) -> usize {
        let size = frame.area();

        // ステータスバー領域と本文領域を分割
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(size);

        let content_area = vertical_chunks[0];
        let status_area = vertical_chunks[1];

        // TOC または Notes パネルが開いている場合は水平分割
        let viewer_area: Rect;
        let side_area_opt: Option<(Rect, bool)>; // (area, is_notes)

        if self.notes_open {
            let panel_width = 40u16.min(content_area.width / 2);
            let horizontal_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0), Constraint::Length(panel_width)])
                .split(content_area);
            viewer_area = horizontal_chunks[0];
            side_area_opt = Some((horizontal_chunks[1], true));
        } else if self.toc_open && !self.toc.is_empty() {
            let toc_width = 40u16.min(content_area.width / 2);
            let horizontal_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(toc_width), Constraint::Min(0)])
                .split(content_area);
            viewer_area = horizontal_chunks[1];
            side_area_opt = Some((horizontal_chunks[0], false));
        } else {
            viewer_area = content_area;
            side_area_opt = None;
        }

        // サイドパネル描画
        if let Some((side_area, is_notes)) = side_area_opt {
            if is_notes {
                let current_note = self.current_anchor.as_ref().and_then(|a| {
                    let entries =
                        notes::get_entries_for(&self.notes_store, &self.filepath.to_string_lossy());
                    notes::find_entry(entries, a).map(|e| e.note.as_str())
                });
                ui_notes::render(
                    frame,
                    side_area,
                    self.current_anchor.as_ref(),
                    current_note,
                    self.notes_edit_mode,
                    &self.notes_textarea,
                    &self.theme,
                );
            } else {
                toc::render(frame, side_area, &self.toc, self.toc_sel, &self.theme);
            }
        }

        // ビューア描画（wrap 後行数を返す）
        let wrapped_line_count = viewer::render(
            frame,
            viewer_area,
            &self.lines,
            self.scroll,
            &self.search_matches,
            self.search_cursor,
            &self.theme,
        );

        // ステータスバー描画
        statusbar::render(
            frame,
            status_area,
            &self.filepath,
            self.scroll,
            self.lines.len().max(1),
            self.toc_open,
            self.notes_open,
            self.notes_edit_mode,
            self.status_error.as_ref().map(|(m, _)| m.as_str()),
            &self.theme,
            self.search_mode,
            &self.search_input,
            &self.search_query,
            self.search_matches.len(),
            self.search_cursor,
        );

        wrapped_line_count
    }

    // =========================================================================
    // メモ機能ヘルパーメソッド
    // =========================================================================

    /// スクロール位置から現在フォーカスしている見出しを検出して `current_anchor` を更新する。
    ///
    /// `block_starts[anchor_block_indices[i]] <= self.scroll` を満たす最大の i を探す。
    pub fn refresh_current_anchor(&mut self) {
        let new = self.find_current_anchor().cloned();
        if new != self.current_anchor {
            self.current_anchor = new;
        }
    }

    /// 現在のスクロール位置に対応する anchor を返す（不変参照）。
    fn find_current_anchor(&self) -> Option<&AnchorKey> {
        find_current_anchor_in(
            &self.anchor_block_indices,
            &self.toplevel_anchors,
            &self.block_starts,
            self.scroll,
        )
    }

    /// `current_anchor` に対応するメモを `notes_store` から取得して `textarea` にセットする。
    pub fn load_textarea_for_current(&mut self) {
        let note_text = self
            .current_anchor
            .as_ref()
            .and_then(|a| {
                let entries =
                    notes::get_entries_for(&self.notes_store, &self.filepath.to_string_lossy());
                notes::find_entry(entries, a).map(|e| e.note.clone())
            })
            .unwrap_or_default();

        let lines: Vec<String> = if note_text.is_empty() {
            vec![]
        } else {
            note_text.lines().map(String::from).collect()
        };
        self.notes_textarea = TextArea::from(lines);
    }

    /// 編集モード中であれば現在のメモを保存し、編集モードを終了する。
    ///
    /// 保存失敗時は `status_error` に転写する。
    fn flush_note_edit_mode(&mut self) {
        if self.notes_edit_mode {
            if let Err(e) = self.persist_current_note() {
                self.status_error = Some((
                    format!("Save notes failed: {}", e),
                    std::time::Instant::now(),
                ));
            }
            self.notes_edit_mode = false;
        }
    }

    /// 現在の `notes_textarea` の内容を `notes_store` に保存し、ファイルに書き込む。
    ///
    /// `current_anchor` が `None` の場合は no-op。
    pub fn persist_current_note(&mut self) -> anyhow::Result<()> {
        let anchor = match self.current_anchor.clone() {
            Some(a) => a,
            None => return Ok(()),
        };

        let note_text = self.notes_textarea.lines().join("\n");
        let file_path = self.filepath.to_string_lossy().to_string();
        let now = notes::now_iso();

        // 現在のエントリを取得してコピー
        let mut entries: Vec<notes::NoteEntry> =
            notes::get_entries_for(&self.notes_store, &file_path).to_vec();

        if let Some(entry) = notes::find_entry_mut(&mut entries, &anchor) {
            // 既存エントリを更新
            entry.note = note_text;
            entry.updated_at = Some(now.clone());
        } else {
            // 新規エントリを追加
            entries.push(notes::NoteEntry {
                heading_text: anchor.heading_text.clone(),
                heading_level: anchor.heading_level,
                occurrence_index: anchor.occurrence_index,
                note: note_text,
                created_at: Some(now.clone()),
                updated_at: Some(now.clone()),
            });
        }

        notes::set_entries_for(&mut self.notes_store, &file_path, entries, &now);
        notes::save(&self.notes_store).map_err(|e| anyhow::anyhow!("notes save failed: {}", e))?;

        Ok(())
    }

    // =========================================================================
    // 検索機能ヘルパーメソッド
    // =========================================================================

    /// 検索モードを開始する。
    ///
    /// メモ編集モードが有効な場合は先に確定させる（モード排他）。
    fn start_search(&mut self) {
        // メモ編集モードが有効なら先に保存して終了（モード排他）
        self.flush_note_edit_mode();
        self.search_mode = true;
        self.search_input.clear();
    }

    /// 検索入力を確定し、マッチへジャンプする。
    ///
    /// 不正な正規表現の場合は `status_error` に 5 秒表示し、マッチをクリアする。
    /// 成功時は `search_cursor = 0` で先頭マッチへスクロールする。
    fn confirm_search(&mut self, max_scroll: usize) {
        self.search_mode = false;
        self.search_query = self.search_input.clone();

        if self.search_query.is_empty() {
            self.search_matches.clear();
            self.search_cursor = 0;
            return;
        }

        match search::find_matches(&self.lines, &self.search_query) {
            Ok(matches) => {
                self.search_matches = matches;
                self.search_cursor = 0;
                if !self.search_matches.is_empty() {
                    self.jump_to_match(max_scroll);
                }
            }
            Err(e) => {
                self.status_error =
                    Some((format!("invalid regex: {}", e), std::time::Instant::now()));
                self.search_matches.clear();
                self.search_cursor = 0;
            }
        }
    }

    /// 検索をキャンセルする（Esc）。
    ///
    /// `search_query` / `search_matches` をクリアする（vim の `nohlsearch` 相当）。
    fn cancel_search(&mut self) {
        self.search_mode = false;
        self.search_input.clear();
        self.search_query.clear();
        self.search_matches.clear();
        self.search_cursor = 0;
    }

    /// 次のマッチへ移動する（`n` キー相当）。
    fn next_match(&mut self, max_scroll: usize) {
        if self.search_matches.is_empty() {
            return;
        }
        let len = self.search_matches.len();
        self.search_cursor = (self.search_cursor + 1) % len;
        self.jump_to_match(max_scroll);
    }

    /// 前のマッチへ移動する（`N` キー相当）。
    fn prev_match(&mut self, max_scroll: usize) {
        if self.search_matches.is_empty() {
            return;
        }
        let len = self.search_matches.len();
        self.search_cursor = (self.search_cursor + len - 1) % len;
        self.jump_to_match(max_scroll);
    }

    /// 現在の `search_cursor` が指すマッチ行へスクロールする。
    ///
    /// TOC ジャンプ（app.rs の `block_starts[entry.block_index].min(max_scroll)`）と同パターン。
    fn jump_to_match(&mut self, max_scroll: usize) {
        if let Some(m) = self.search_matches.get(self.search_cursor) {
            self.scroll = m.line.min(max_scroll);
        }
    }

    /// テーマを循環させる。`forward=true` で順方向、`false` で逆方向。
    ///
    /// 新テーマで Highlighter を再生成し、ドキュメントを再変換して描画データを更新する。
    /// テーマ ID を config.json に永続化する。
    fn cycle_theme(&mut self, forward: bool) -> anyhow::Result<()> {
        let new_index = next_theme_index(self.theme_index, forward, THEME_CYCLE.len());
        let new_theme = TuiTheme::from_id(THEME_CYCLE[new_index]);

        let new_highlighter = Highlighter::with_syntect_theme(new_theme.syntect_theme)
            .unwrap_or_else(|e| {
                eprintln!("mdview: syntect theme load failed: {}. using default.", e);
                Highlighter::new()
            });

        self.highlighter = Arc::new(new_highlighter);
        self.theme = new_theme;
        self.theme_index = new_index;

        // 描画データを再構築
        self.load()?;

        // config.json に永続化（失敗は status_error に転写。非 blocking）
        let mut cfg = crate::config::Config::load();
        cfg.theme = self.theme.id.to_string();
        if let Err(e) = cfg.save() {
            self.status_error = Some((
                format!("config save failed: {}", e),
                std::time::Instant::now(),
            ));
        }

        Ok(())
    }

    /// スクロール最上行にあるリンクのうち最初の 1 個を外部ブラウザで開く。
    ///
    /// リンクが無い・URL が不正なら静かに no-op。
    fn open_focused_link(&mut self) {
        let url = self.lines.get(self.scroll).and_then(|line| {
            line.iter()
                .find(|span| span.url.is_some())
                .and_then(|span| span.url.clone())
        });

        if let Some(u) = url {
            if is_safe_url(&u) {
                // JoinHandle を捨てることで TUI ループをブロックしない
                let _ = open::that_in_background(u);
            }
        }
    }
}

// ===========================================================================
// ヘルパー関数
// ===========================================================================

/// テーマサイクルの次インデックスを計算する（純粋関数、テスト可能）。
fn next_theme_index(current: usize, forward: bool, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    if forward {
        (current + 1) % len
    } else {
        current.checked_sub(1).unwrap_or(len - 1)
    }
}

/// URL スキーム検証（https / http / mailto のみ許可）。
///
/// javascript: / file: / data: 等の危険スキームを排除する。
fn is_safe_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:")
}

/// 指定スクロール位置に対応する anchor を返す（モジュールレベル pure function）。
///
/// `block_starts[anchor_block_indices[i]] <= scroll` を満たす最大の i を探し、
/// `toplevel_anchors[i]` を返す。`App::find_current_anchor` の委譲先。
/// テストから直接呼べるよう `App` impl 外に配置する（`count_anchors_in_block` と同パターン）。
fn find_current_anchor_in<'a>(
    anchor_block_indices: &[usize],
    toplevel_anchors: &'a [AnchorKey],
    block_starts: &[usize],
    scroll: usize,
) -> Option<&'a AnchorKey> {
    let mut best: Option<usize> = None;
    for (i, &block_index) in anchor_block_indices.iter().enumerate() {
        if let Some(&line) = block_starts.get(block_index) {
            if line <= scroll {
                best = Some(i);
            } else {
                break;
            }
        }
    }
    best.and_then(|i| toplevel_anchors.get(i))
}

/// トップレベルブロック 1 つが再帰的に生成する anchor（見出し）の数を返す。
///
/// `collect_heading_anchors_inner` と同一の再帰パターン。
/// `App::load()` の `toplevel_anchors` 抽出時に `anchor_offset` を正確に計算するために使う。
fn count_anchors_in_block(block: &Block) -> usize {
    match block {
        Block::Heading { .. } => 1,
        Block::List { items, .. } => items
            .iter()
            .flat_map(|item| item.blocks.iter())
            .map(count_anchors_in_block)
            .sum(),
        Block::BlockQuote { blocks: inner } => inner.iter().map(count_anchors_in_block).sum(),
        _ => 0,
    }
}

// ===========================================================================
// テスト
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::{count_anchors_in_block, find_current_anchor_in, is_safe_url, next_theme_index};
    use mdview_core::{AnchorKey, Block};

    fn make_anchor(text: &str, level: u8, occ: u32) -> AnchorKey {
        AnchorKey {
            heading_text: text.to_string(),
            heading_level: level,
            occurrence_index: occ,
        }
    }

    // =========================================================================
    // find_current_anchor_in のテスト
    // =========================================================================

    #[test]
    fn find_current_anchor_basic() {
        // block_starts: [0, 5, 10]
        // block index 1 (= line 5) が H1、block index 2 (= line 10) が H2
        let anchor_block_indices = vec![1, 2];
        let toplevel_anchors = vec![make_anchor("H1", 1, 0), make_anchor("H2", 2, 0)];
        let block_starts = vec![0, 5, 10];

        // scroll=0 は見出しより前 → None
        assert!(
            find_current_anchor_in(&anchor_block_indices, &toplevel_anchors, &block_starts, 0)
                .is_none()
        );

        // scroll=5 は最初の見出しと同じ行 → H1
        assert_eq!(
            find_current_anchor_in(&anchor_block_indices, &toplevel_anchors, &block_starts, 5)
                .unwrap()
                .heading_text,
            "H1"
        );
    }

    #[test]
    fn find_current_anchor_returns_latest_before_scroll() {
        // block_starts: [0, 3, 7, 12]
        // anchor が block_index 1 (line3) と block_index 3 (line12) に対応
        let anchor_block_indices = vec![1, 3];
        let toplevel_anchors = vec![make_anchor("First", 1, 0), make_anchor("Second", 1, 1)];
        let block_starts = vec![0, 3, 7, 12];

        // scroll=8 は line3 <= 8 かつ line12 > 8 → First
        assert_eq!(
            find_current_anchor_in(&anchor_block_indices, &toplevel_anchors, &block_starts, 8)
                .unwrap()
                .heading_text,
            "First"
        );
    }

    #[test]
    fn find_current_anchor_no_heading_above() {
        // 見出しが line 10 にしかない場合、scroll < 10 なら None
        let anchor_block_indices = vec![2];
        let toplevel_anchors = vec![make_anchor("Late", 1, 0)];
        let block_starts = vec![0, 5, 10];

        assert!(
            find_current_anchor_in(&anchor_block_indices, &toplevel_anchors, &block_starts, 3)
                .is_none()
        );
    }

    #[test]
    fn find_current_anchor_at_exactly_heading_line() {
        let anchor_block_indices = vec![1];
        let toplevel_anchors = vec![make_anchor("Exact", 1, 0)];
        let block_starts = vec![0, 10];

        assert_eq!(
            find_current_anchor_in(&anchor_block_indices, &toplevel_anchors, &block_starts, 10)
                .unwrap()
                .heading_text,
            "Exact"
        );
    }

    // =========================================================================
    // count_anchors_in_block のテスト
    // =========================================================================

    #[test]
    fn count_anchors_in_block_heading_is_1() {
        let block = Block::Heading {
            level: 1,
            spans: vec![],
        };
        assert_eq!(count_anchors_in_block(&block), 1);
    }

    #[test]
    fn count_anchors_in_block_list_with_nested_headings() {
        // List 内に Heading 2 つ → count は 2
        let block = Block::List {
            ordered: false,
            start: None,
            items: vec![
                mdview_core::types::ListItem {
                    blocks: vec![Block::Heading {
                        level: 2,
                        spans: vec![],
                    }],
                },
                mdview_core::types::ListItem {
                    blocks: vec![Block::Heading {
                        level: 3,
                        spans: vec![],
                    }],
                },
            ],
        };
        assert_eq!(count_anchors_in_block(&block), 2);
    }

    #[test]
    fn count_anchors_in_block_paragraph_is_0() {
        let block = Block::Paragraph { lines: vec![] };
        assert_eq!(count_anchors_in_block(&block), 0);
    }

    // =========================================================================
    // Phase F4: is_safe_url のテスト
    // =========================================================================

    #[test]
    fn is_safe_url_accepts_https() {
        assert!(is_safe_url("https://example.com"));
    }

    #[test]
    fn is_safe_url_accepts_http() {
        assert!(is_safe_url("http://example.com"));
    }

    #[test]
    fn is_safe_url_accepts_mailto() {
        assert!(is_safe_url("mailto:x@y.z"));
    }

    #[test]
    fn is_safe_url_rejects_javascript() {
        assert!(!is_safe_url("javascript:alert(1)"));
    }

    #[test]
    fn is_safe_url_rejects_file() {
        assert!(!is_safe_url("file:///etc/passwd"));
    }

    // =========================================================================
    // Phase F3: next_theme_index のテスト
    // =========================================================================

    #[test]
    fn cycle_theme_forward_increments_index() {
        // 6 テーマ: 0 → 1 → 2 → ... → 5 → 0（wrap）
        assert_eq!(next_theme_index(0, true, 6), 1);
        assert_eq!(next_theme_index(5, true, 6), 0); // wrap
        assert_eq!(next_theme_index(3, true, 6), 4);
    }

    #[test]
    fn cycle_theme_backward_decrements_index() {
        // 逆方向: 0 → 5 (wrap), 3 → 2
        assert_eq!(next_theme_index(0, false, 6), 5); // wrap
        assert_eq!(next_theme_index(3, false, 6), 2);
        assert_eq!(next_theme_index(1, false, 6), 0);
    }

    #[test]
    fn cycle_theme_single_element() {
        // 要素数 1 は常に 0
        assert_eq!(next_theme_index(0, true, 1), 0);
        assert_eq!(next_theme_index(0, false, 1), 0);
    }
}
