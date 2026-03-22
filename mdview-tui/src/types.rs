use ratatui::style::Style;

#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

pub type StyledLine = Vec<StyledSpan>;
