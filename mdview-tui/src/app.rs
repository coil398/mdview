use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Frame;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Duration;

use mdview_core::parser::parse_markdown;
use mdview_core::TocEntry;

use crate::highlighter::Highlighter;
use crate::style::{convert_document, StyledOutput};
use crate::types::StyledLine;
use crate::ui::{statusbar, toc, viewer};
use crate::watcher::FileWatcher;

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
}

impl App {
    pub fn new(path: PathBuf) -> Result<Self> {
        let highlighter = Arc::new(Highlighter::new());
        let (tx, rx) = mpsc::channel();
        let watcher = FileWatcher::new(path.clone(), tx)?;

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
        };

        app.load()?;
        Ok(app)
    }

    pub fn load(&mut self) -> Result<()> {
        let text = std::fs::read_to_string(&self.filepath)?;
        let doc = parse_markdown(&text);
        let StyledOutput {
            lines,
            block_starts,
            toc,
        } = convert_document(&doc, &self.highlighter);
        self.lines = lines;
        self.toc = toc;
        self.block_starts = block_starts;
        if self.toc_sel >= self.toc.len() {
            self.toc_sel = 0;
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
                if let Err(e) = self.load() {
                    self.status_error =
                        Some((format!("Reload failed: {}", e), std::time::Instant::now()));
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

                    let max_scroll = if self.wrapped_line_count > 0 {
                        self.wrapped_line_count.saturating_sub(content_height)
                    } else {
                        self.lines.len().saturating_sub(content_height)
                    };

                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,

                        KeyCode::Char('j') | KeyCode::Down => {
                            if self.toc_open {
                                if !self.toc.is_empty() {
                                    self.toc_sel = (self.toc_sel + 1).min(self.toc.len() - 1);
                                }
                            } else {
                                self.scroll = (self.scroll + 1).min(max_scroll);
                            }
                        }

                        KeyCode::Char('k') | KeyCode::Up => {
                            if self.toc_open {
                                self.toc_sel = self.toc_sel.saturating_sub(1);
                            } else {
                                self.scroll = self.scroll.saturating_sub(1);
                            }
                        }

                        KeyCode::PageDown => {
                            self.scroll =
                                (self.scroll + content_height.saturating_sub(1)).min(max_scroll);
                        }

                        KeyCode::PageUp => {
                            self.scroll =
                                self.scroll.saturating_sub(content_height.saturating_sub(1));
                        }

                        KeyCode::Char('g') => {
                            self.scroll = 0;
                        }

                        KeyCode::Char('G') => {
                            self.scroll = max_scroll;
                        }

                        KeyCode::Char('t') => {
                            self.toc_open = !self.toc_open;
                            if self.toc_open && self.toc_sel >= self.toc.len() {
                                self.toc_sel = 0;
                            }
                        }

                        KeyCode::Char('r') => {
                            if let Err(e) = self.load() {
                                self.status_error = Some((
                                    format!("Reload failed: {}", e),
                                    std::time::Instant::now(),
                                ));
                            }
                        }

                        KeyCode::Enter => {
                            if self.toc_open && !self.toc.is_empty() {
                                let entry = &self.toc[self.toc_sel];
                                let target_line = self
                                    .block_starts
                                    .get(entry.block_index)
                                    .copied()
                                    .unwrap_or(0);
                                self.scroll = target_line.min(max_scroll);
                                self.toc_open = false;
                            }
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

        // TOCが開いている場合は水平分割
        let viewer_area: Rect;
        let toc_area_opt: Option<Rect>;

        if self.toc_open && !self.toc.is_empty() {
            let toc_width = 40u16.min(content_area.width / 2);
            let horizontal_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(toc_width), Constraint::Min(0)])
                .split(content_area);
            toc_area_opt = Some(horizontal_chunks[0]);
            viewer_area = horizontal_chunks[1];
        } else {
            toc_area_opt = None;
            viewer_area = content_area;
        }

        // TOC描画
        if let Some(toc_area) = toc_area_opt {
            toc::render(frame, toc_area, &self.toc, self.toc_sel);
        }

        // ビューア描画（wrap 後行数を返す）
        let wrapped_line_count = viewer::render(frame, viewer_area, &self.lines, self.scroll);

        // ステータスバー描画
        statusbar::render(
            frame,
            status_area,
            &self.filepath,
            self.scroll,
            self.lines.len().max(1),
            self.toc_open,
            self.status_error.as_ref().map(|(m, _)| m.as_str()),
        );

        wrapped_line_count
    }
}
