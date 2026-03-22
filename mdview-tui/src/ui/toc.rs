use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListItem, ListState};
use ratatui::Frame;

use mdview_core::TocEntry;

pub fn render(frame: &mut Frame, area: Rect, toc: &[TocEntry], toc_sel: usize) {
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

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black),
        );

    let mut state = ListState::default();
    if !toc.is_empty() {
        state.select(Some(toc_sel));
    }

    frame.render_stateful_widget(list, area, &mut state);
}
