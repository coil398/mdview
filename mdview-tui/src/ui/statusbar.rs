use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::path::Path;

use crate::theme::TuiTheme;

#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    filepath: &Path,
    scroll: usize,
    total: usize,
    toc_open: bool,
    notes_open: bool,
    notes_edit_mode: bool,
    status_error: Option<&str>,
    theme: &TuiTheme,
    search_mode: bool,
    search_input: &str,
    search_query: &str,
    search_match_count: usize,
    search_cursor: usize,
) {
    let (status, bg) = if let Some(msg) = status_error {
        // エラー表示が最優先
        (format!(" [ERROR] {}", msg), theme.statusbar_error_bg)
    } else if search_mode {
        // 検索入力中: vim コマンドライン互換表示
        (format!("/{}\u{2588}", search_input), theme.statusbar_bg)
    } else {
        let filename = filepath.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let pct = if total > 1 {
            (100 * scroll / (total - 1)).min(100)
        } else {
            100
        };
        let toc_hint = if toc_open { "[t]close" } else { "[t]TOC" };
        let notes_hint = if notes_edit_mode {
            "[Esc]back-to-normal"
        } else if notes_open {
            "[m]close-notes [i]edit"
        } else {
            "[m]notes"
        };
        // 検索クエリが確定している場合はマッチ情報を末尾に追記
        let search_info = if !search_query.is_empty() {
            format!(
                "  [{}/{}] /{}",
                search_cursor + 1,
                search_match_count,
                search_query
            )
        } else {
            String::new()
        };
        (
            format!(
                " {}  {}/{}  {}%  {}  {}  [/]search [n/N]next/prev [T/Ctrl-T]theme [o]open-link [j/k]scroll [g/G]top/end [r]force-reload [q]quit{}",
                filename,
                scroll + 1,
                total,
                pct,
                toc_hint,
                notes_hint,
                search_info,
            ),
            theme.statusbar_bg,
        )
    };

    let paragraph = Paragraph::new(Line::from(status)).style(
        Style::default()
            .bg(bg)
            .fg(theme.statusbar_fg)
            .add_modifier(Modifier::BOLD),
    );

    frame.render_widget(paragraph, area);
}
