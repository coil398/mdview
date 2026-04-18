use serde::{Deserialize, Serialize};

/// インラインスパンの種別。
///
/// pulldown-cmark のインライン文脈に揃え、Block 相当の変種は持たない。
/// （見出しや引用などはすべて [`Block`] 側で表現する）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanKind {
    Normal,
    Bold,
    Italic,
    BoldItalic,
    CodeInline,
    Link { url: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    pub kind: SpanKind,
}

/// テーブルセル。Markdown 仕様の通りインライン Span のみを許容する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub spans: Vec<Span>,
}

/// テーブル列の整列指定。pulldown-cmark の `Alignment` を直接マップ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

/// リスト項目。中に任意の Block 列を保持できる（段落・ネストリスト・コードブロックなど）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    pub blocks: Vec<Block>,
}

/// ブロック要素。Document はこの列のツリー。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Block {
    /// 段落。HardBreak 区切りで複数行になる。
    Paragraph { lines: Vec<Vec<Span>> },
    /// 見出し。レベル 1〜6。spans 内に Bold/Italic/Link 等を含み得る。
    Heading { level: u8, spans: Vec<Span> },
    /// 順序付き or 順序なしリスト。`start` は順序付きリストの開始番号。
    List {
        ordered: bool,
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    /// 引用ブロック。中に再帰的に Block を持てる。
    BlockQuote { blocks: Vec<Block> },
    /// コードブロック。`code` は末尾改行を含まない素のテキスト。
    CodeBlock { lang: Option<String>, code: String },
    /// テーブル。`align` は列ごとの整列指定で、列数はヘッダの長さに揃う。
    Table {
        header: Vec<Cell>,
        rows: Vec<Vec<Cell>>,
        align: Vec<Alignment>,
    },
    /// 水平線。
    Rule,
}

/// 目次エントリ。`block_index` は [`Document::blocks`] の何番目に対応するかを示す。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TocEntry {
    pub block_index: usize,
    pub title: String,
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub toc: Vec<TocEntry>,
}
