use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListItem, ListState};
use ratatui::Frame;

use mdview_core::TocEntry;

use crate::theme::TuiTheme;

pub fn render(frame: &mut Frame, area: Rect, toc: &[TocEntry], toc_sel: usize, theme: &TuiTheme) {
    let block = Block::bordered().title(Line::from(" ≡ Contents "));

    let items: Vec<ListItem> = toc
        .iter()
        .map(|entry| {
            let indent = "  ".repeat((entry.level as usize).saturating_sub(1));
            let marker = match entry.level {
                1 => "▸ ",
                2 => "· ",
                _ => "  ",
            };
            let text = format!("{}{}{}", indent, marker, entry.title);
            ListItem::new(text)
        })
        .collect();

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(theme.toc_highlight_bg)
            .fg(theme.toc_highlight_fg),
    );

    let mut state = ListState::default();
    if !toc.is_empty() {
        state.select(Some(toc_sel));
    }

    frame.render_stateful_widget(list, area, &mut state);
}
