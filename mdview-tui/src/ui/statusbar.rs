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
    status_error: Option<&str>,
    theme: &TuiTheme,
) {
    let (status, bg) = if let Some(msg) = status_error {
        (format!(" [ERROR] {}", msg), theme.statusbar_error_bg)
    } else {
        let filename = filepath.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let pct = if total > 1 {
            (100 * scroll / (total - 1)).min(100)
        } else {
            100
        };
        let toc_hint = if toc_open { "[t]close" } else { "[t]TOC" };
        (
            format!(
                " {}  {}/{}  {}%  {}  [j/k]scroll [g/G]top/end [r]force-reload [q]quit",
                filename,
                scroll + 1,
                total,
                pct,
                toc_hint
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
