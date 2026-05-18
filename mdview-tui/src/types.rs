use ratatui::style::Style;

#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
    /// リンク URL（`SpanKind::Link` 由来のみ設定される）。
    /// `None` はリンクでないことを示す。外部ブラウザ起動に使用する。
    pub url: Option<String>,
}

pub type StyledLine = Vec<StyledSpan>;
