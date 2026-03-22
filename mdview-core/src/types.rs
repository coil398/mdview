use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    pub kind: SpanKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpanKind {
    Normal,
    Heading(u8),
    Bold,
    Italic,
    BoldItalic,
    CodeInline,
    CodeBlock { lang: Option<String> },
    Link { url: String },
    ListMarker,
    BlockQuote,
    Rule,
}

pub type Line = Vec<Span>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TocEntry {
    pub line_index: usize,
    pub title: String,
    pub level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub lines: Vec<Line>,
    pub toc: Vec<TocEntry>,
}
