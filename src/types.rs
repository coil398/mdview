use ratatui::style::Style;

#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

pub type StyledLine = Vec<StyledSpan>;

#[derive(Debug, Clone)]
pub struct TocEntry {
    pub line_index: usize,
    pub title: String,
    pub level: u8,
}
