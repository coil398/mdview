use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::types::StyledLine;

pub fn render(frame: &mut Frame, area: Rect, lines: &[StyledLine], scroll: usize) {
    let ratatui_lines: Vec<Line> = lines
        .iter()
        .map(|styled_line| {
            let spans: Vec<Span> = styled_line
                .iter()
                .map(|span| Span::styled(span.text.clone(), span.style))
                .collect();
            Line::from(spans)
        })
        .collect();

    let text = Text::from(ratatui_lines);
    let paragraph = Paragraph::new(text)
        .scroll((scroll as u16, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}
