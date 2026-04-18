//! Markdown → [`Document`] 変換。
//!
//! pulldown-cmark のイベントをスタックベースの `BlockBuilder` で受け、
//! [`Block`] ツリーを組み立てる。インライン状態（Bold/Italic/Link）は
//! スタックとは別の `InlineState` で管理する。

use pulldown_cmark::{
    Alignment as PdAlignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd,
};

use crate::types::{
    Alignment, Block, Cell, Document, ListItem, Span, SpanKind, TocEntry, SCHEMA_VERSION,
};

// ===========================================================================
// Public API
// ===========================================================================

pub fn parse_markdown(text: &str) -> Document {
    let options = Options::all();
    let parser = Parser::new_ext(text, options);

    let mut state = ParseState::new();
    for event in parser {
        state.handle_event(event);
    }
    state.finish()
}

// ===========================================================================
// Inline state
// ===========================================================================

/// インライン要素の重ね合わせ状態。`Tag::Strong` などの開閉に応じて増減する。
#[derive(Default, Debug)]
struct InlineState {
    strong_depth: u32,
    emphasis_depth: u32,
    /// 現在開いているリンクの URL スタック（リンクのネストは GFM では発生しない想定だが、
    /// 念のためスタックにしておく）。
    link_urls: Vec<String>,
}

impl InlineState {
    fn current_kind(&self) -> SpanKind {
        if let Some(url) = self.link_urls.last() {
            return SpanKind::Link { url: url.clone() };
        }
        match (self.strong_depth > 0, self.emphasis_depth > 0) {
            (true, true) => SpanKind::BoldItalic,
            (true, false) => SpanKind::Bold,
            (false, true) => SpanKind::Italic,
            (false, false) => SpanKind::Normal,
        }
    }
}

// ===========================================================================
// Block builders
// ===========================================================================

/// スタックに積まれる「組み立て中のブロック」。
#[derive(Debug)]
enum BlockBuilder {
    Paragraph {
        lines: Vec<Vec<Span>>,
        current: Vec<Span>,
    },
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    /// リストアイテム内に積まれた中間ブロック群。終了時に親 List に流し込む。
    List {
        ordered: bool,
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Item {
        blocks: Vec<Block>,
    },
    BlockQuote {
        blocks: Vec<Block>,
    },
    CodeBlock {
        lang: Option<String>,
        code: String,
    },
    Table {
        header: Vec<Cell>,
        rows: Vec<Vec<Cell>>,
        align: Vec<Alignment>,
        in_header: bool,
        current_row: Vec<Cell>,
    },
    /// テーブルセル組み立て中の Span 列。
    TableCell {
        spans: Vec<Span>,
    },
}

// ===========================================================================
// ParseState
// ===========================================================================

struct ParseState {
    /// 完成したトップレベル Block 列。
    blocks: Vec<Block>,
    /// 目次。Heading 終端時点で `block_index = blocks.len() + (上で出力されるであろう個数)`
    /// となるよう、後段の `push_block` 経由で値が確定する。
    toc: Vec<TocEntry>,
    /// 組み立て中ブロックスタック。
    stack: Vec<BlockBuilder>,
    /// インライン重ね合わせ状態。
    inline: InlineState,
    /// 直前に閉じた Heading の TocEntry。次に push される block_index を確定させるためにバッファ。
    pending_heading_toc: Option<TocEntry>,
}

impl ParseState {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            toc: Vec::new(),
            stack: Vec::new(),
            inline: InlineState::default(),
            pending_heading_toc: None,
        }
    }

    fn finish(mut self) -> Document {
        // 健全な Markdown ではここでスタックは空になっているはずだが、
        // 念のため積み残しを順次 pop して親に流す。
        while let Some(builder) = self.stack.pop() {
            let block = self.builder_into_block(builder);
            self.append_block_to_top(block);
        }
        Document {
            schema_version: SCHEMA_VERSION,
            blocks: self.blocks,
            toc: self.toc,
        }
    }

    // -------------------------------------------------------------------
    // Event dispatch
    // -------------------------------------------------------------------

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.handle_start(tag),
            Event::End(tag_end) => self.handle_end(tag_end),
            Event::Text(t) => self.handle_text(t.into_string()),
            Event::Code(t) => self.handle_inline_code(t.into_string()),
            Event::SoftBreak => self.handle_soft_break(),
            Event::HardBreak => self.handle_hard_break(),
            Event::Rule => self.push_block(Block::Rule),

            // 以下、フェーズ 1 では未対応のイベント。明示的に何もしないことを示す。
            Event::InlineMath(_) => {}
            Event::DisplayMath(_) => {}
            Event::Html(_) => {}
            Event::InlineHtml(_) => {}
            Event::FootnoteReference(_) => {}
            Event::TaskListMarker(_) => {}
        }
    }

    fn handle_start(&mut self, tag: Tag) {
        // ブロック系タグの開始時のみ、暗黙 Paragraph をクローズする
        // （インラインタグ Strong/Emphasis/Link 等はクローズしない）
        if is_block_tag(&tag) {
            self.flush_implicit_paragraph();
        }
        match tag {
            Tag::Paragraph => {
                self.stack.push(BlockBuilder::Paragraph {
                    lines: Vec::new(),
                    current: Vec::new(),
                });
            }
            Tag::Heading { level, .. } => {
                self.stack.push(BlockBuilder::Heading {
                    level: level as u8,
                    spans: Vec::new(),
                });
            }
            Tag::BlockQuote(_) => {
                self.stack
                    .push(BlockBuilder::BlockQuote { blocks: Vec::new() });
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Indented => None,
                    CodeBlockKind::Fenced(s) => {
                        let s = s.into_string();
                        if s.is_empty() {
                            None
                        } else {
                            Some(s)
                        }
                    }
                };
                self.stack.push(BlockBuilder::CodeBlock {
                    lang,
                    code: String::new(),
                });
            }
            Tag::List(start) => {
                let ordered = start.is_some();
                self.stack.push(BlockBuilder::List {
                    ordered,
                    start,
                    items: Vec::new(),
                });
            }
            Tag::Item => {
                self.stack.push(BlockBuilder::Item { blocks: Vec::new() });
            }
            Tag::Table(aligns) => {
                self.stack.push(BlockBuilder::Table {
                    header: Vec::new(),
                    rows: Vec::new(),
                    align: aligns.into_iter().map(map_alignment).collect(),
                    in_header: false,
                    current_row: Vec::new(),
                });
            }
            Tag::TableHead => {
                if let Some(BlockBuilder::Table { in_header, .. }) = self.stack.last_mut() {
                    *in_header = true;
                }
            }
            Tag::TableRow => {
                if let Some(BlockBuilder::Table { current_row, .. }) = self.stack.last_mut() {
                    current_row.clear();
                }
            }
            Tag::TableCell => {
                self.stack
                    .push(BlockBuilder::TableCell { spans: Vec::new() });
            }
            Tag::Strong => {
                self.inline.strong_depth += 1;
            }
            Tag::Emphasis => {
                self.inline.emphasis_depth += 1;
            }
            Tag::Link { dest_url, .. } => {
                self.inline.link_urls.push(dest_url.into_string());
            }

            // 未対応 / 無視するブロック・インラインタグ
            Tag::HtmlBlock => {}
            Tag::FootnoteDefinition(_) => {}
            Tag::DefinitionList => {}
            Tag::DefinitionListTitle => {}
            Tag::DefinitionListDefinition => {}
            Tag::Strikethrough => {}
            Tag::Superscript => {}
            Tag::Subscript => {}
            Tag::Image { .. } => {}
            Tag::MetadataBlock(_) => {}
        }
    }

    fn handle_end(&mut self, tag_end: TagEnd) {
        match tag_end {
            TagEnd::Paragraph => {
                if let Some(BlockBuilder::Paragraph { mut lines, current }) =
                    self.pop_matching(|b| matches!(b, BlockBuilder::Paragraph { .. }))
                {
                    if !current.is_empty() || lines.is_empty() {
                        lines.push(current);
                    }
                    self.push_block(Block::Paragraph { lines });
                }
            }
            TagEnd::Heading(level) => {
                if let Some(BlockBuilder::Heading { level: lvl, spans }) =
                    self.pop_matching(|b| matches!(b, BlockBuilder::Heading { .. }))
                {
                    let _ = level; // pulldown 側 level と保存 level は等しい想定
                    let title = spans.iter().map(|s| s.text.as_str()).collect::<String>();
                    // TocEntry は次に push される Block の index を指す。
                    // push_block 内で確定させるため、いったんバッファに入れる。
                    self.pending_heading_toc = Some(TocEntry {
                        block_index: 0, // placeholder — push_block で上書き
                        title,
                        level: lvl,
                    });
                    self.push_block(Block::Heading { level: lvl, spans });
                }
            }
            TagEnd::BlockQuote(_) => {
                self.flush_implicit_paragraph();
                if let Some(BlockBuilder::BlockQuote { blocks }) =
                    self.pop_matching(|b| matches!(b, BlockBuilder::BlockQuote { .. }))
                {
                    self.push_block(Block::BlockQuote { blocks });
                }
            }
            TagEnd::CodeBlock => {
                if let Some(BlockBuilder::CodeBlock { lang, mut code }) =
                    self.pop_matching(|b| matches!(b, BlockBuilder::CodeBlock { .. }))
                {
                    // pulldown-cmark のテキストは末尾に改行を含むことが多いので除去
                    while code.ends_with('\n') {
                        code.pop();
                    }
                    self.push_block(Block::CodeBlock { lang, code });
                }
            }
            TagEnd::List(_) => {
                self.flush_implicit_paragraph();
                if let Some(BlockBuilder::List {
                    ordered,
                    start,
                    items,
                }) = self.pop_matching(|b| matches!(b, BlockBuilder::List { .. }))
                {
                    self.push_block(Block::List {
                        ordered,
                        start,
                        items,
                    });
                }
            }
            TagEnd::Item => {
                self.flush_implicit_paragraph();
                if let Some(BlockBuilder::Item { blocks }) =
                    self.pop_matching(|b| matches!(b, BlockBuilder::Item { .. }))
                {
                    if let Some(BlockBuilder::List { items, .. }) = self.stack.last_mut() {
                        items.push(ListItem { blocks });
                    }
                }
            }
            TagEnd::Table => {
                if let Some(BlockBuilder::Table {
                    header,
                    rows,
                    align,
                    ..
                }) = self.pop_matching(|b| matches!(b, BlockBuilder::Table { .. }))
                {
                    self.push_block(Block::Table {
                        header,
                        rows,
                        align,
                    });
                }
            }
            TagEnd::TableHead => {
                if let Some(BlockBuilder::Table {
                    header,
                    in_header,
                    current_row,
                    ..
                }) = self
                    .stack
                    .iter_mut()
                    .rev()
                    .find(|b| matches!(b, BlockBuilder::Table { .. }))
                {
                    *header = std::mem::take(current_row);
                    *in_header = false;
                }
            }
            TagEnd::TableRow => {
                if let Some(BlockBuilder::Table {
                    rows,
                    in_header,
                    current_row,
                    ..
                }) = self
                    .stack
                    .iter_mut()
                    .rev()
                    .find(|b| matches!(b, BlockBuilder::Table { .. }))
                {
                    if !*in_header {
                        rows.push(std::mem::take(current_row));
                    } else {
                        current_row.clear();
                    }
                }
            }
            TagEnd::TableCell => {
                if let Some(BlockBuilder::TableCell { spans }) =
                    self.pop_matching(|b| matches!(b, BlockBuilder::TableCell { .. }))
                {
                    if let Some(BlockBuilder::Table { current_row, .. }) = self.stack.last_mut() {
                        current_row.push(Cell { spans });
                    }
                }
            }
            TagEnd::Strong => {
                self.inline.strong_depth = self.inline.strong_depth.saturating_sub(1);
            }
            TagEnd::Emphasis => {
                self.inline.emphasis_depth = self.inline.emphasis_depth.saturating_sub(1);
            }
            TagEnd::Link => {
                self.inline.link_urls.pop();
            }

            // 未対応 / 無視するブロック・インラインタグの終端
            TagEnd::HtmlBlock => {}
            TagEnd::FootnoteDefinition => {}
            TagEnd::DefinitionList => {}
            TagEnd::DefinitionListTitle => {}
            TagEnd::DefinitionListDefinition => {}
            TagEnd::Strikethrough => {}
            TagEnd::Superscript => {}
            TagEnd::Subscript => {}
            TagEnd::Image => {}
            TagEnd::MetadataBlock(_) => {}
        }
    }

    fn handle_text(&mut self, text: String) {
        // CodeBlock の中なら生テキストとして蓄積
        if let Some(BlockBuilder::CodeBlock { code, .. }) = self.stack.last_mut() {
            code.push_str(&text);
            return;
        }
        let kind = self.inline.current_kind();
        let span = Span { text, kind };
        self.push_inline_span(span);
    }

    fn handle_inline_code(&mut self, text: String) {
        // インラインコードはリンクの中に入っていてもインラインコードとして扱う
        // （pulldown-cmark のセマンティクスに合わせる）
        let span = Span {
            text,
            kind: SpanKind::CodeInline,
        };
        self.push_inline_span(span);
    }

    fn handle_soft_break(&mut self) {
        // 段落内のソフトブレークはスペースに変換（既存挙動踏襲）
        let kind = self.inline.current_kind();
        self.push_inline_span(Span {
            text: " ".to_string(),
            kind,
        });
    }

    fn handle_hard_break(&mut self) {
        // 段落内なら lines を切り替える
        if let Some(BlockBuilder::Paragraph { lines, current }) = self.stack.last_mut() {
            let line = std::mem::take(current);
            lines.push(line);
        }
        // 段落以外（見出し内 HardBreak など）は無視
    }

    // -------------------------------------------------------------------
    // Stack helpers
    // -------------------------------------------------------------------

    /// インライン Span を最も内側の「Span を受け取れる」Builder に積む。
    ///
    /// - 既に Paragraph / Heading / TableCell が開いていればそこに直接 push
    /// - 開いていない状態で Item / BlockQuote にぶつかったら、暗黙 Paragraph を
    ///   作って push する（pulldown-cmark の tight list は Item 直下に Paragraph
    ///   タグを発行しないので、そのフォールバック）
    /// - どちらでもなければトップレベルに暗黙 Paragraph を作る
    fn push_inline_span(&mut self, span: Span) {
        for builder in self.stack.iter_mut().rev() {
            match builder {
                BlockBuilder::Paragraph { current, .. } => {
                    current.push(span);
                    return;
                }
                BlockBuilder::Heading { spans, .. } => {
                    spans.push(span);
                    return;
                }
                BlockBuilder::TableCell { spans } => {
                    spans.push(span);
                    return;
                }
                BlockBuilder::Item { .. } | BlockBuilder::BlockQuote { .. } => {
                    // Item / BlockQuote 直下に暗黙 Paragraph を開く
                    self.stack.push(BlockBuilder::Paragraph {
                        lines: Vec::new(),
                        current: vec![span],
                    });
                    return;
                }
                _ => continue,
            }
        }
        // スタックが空 or インライン受け入れ可能な親が無かった場合：
        // トップレベルに暗黙 Paragraph を作る
        self.stack.push(BlockBuilder::Paragraph {
            lines: Vec::new(),
            current: vec![span],
        });
    }

    /// 最内側が Paragraph で、それが「Item/BlockQuote 直下に作られた暗黙 Paragraph」
    /// あるいはトップレベルに作られた暗黙 Paragraph の場合、それを Block::Paragraph
    /// として親に流し込む。pulldown-cmark の Paragraph タグで作られた明示 Paragraph も
    /// 同様に扱って良い（明示 Paragraph には対応する End イベントが来るため、
    /// このフラッシュは「次のブロックが始まる前」or「親が閉じる前」のいずれかに
    /// 確実に呼ばれる）。
    fn flush_implicit_paragraph(&mut self) {
        if let Some(BlockBuilder::Paragraph { .. }) = self.stack.last() {
            // 取り出して Block::Paragraph に変換
            if let Some(BlockBuilder::Paragraph { mut lines, current }) = self.stack.pop() {
                if !current.is_empty() || lines.is_empty() {
                    lines.push(current);
                }
                // 中身が完全に空（行が空のみ）なら捨てる（無駄な Paragraph を残さない）
                let has_content = lines.iter().any(|l| !l.is_empty());
                if has_content {
                    self.append_block_to_top(Block::Paragraph { lines });
                }
            }
        }
    }

    /// 述語にマッチする最も内側の Builder を pop する。
    /// マッチしない要素が間にあった場合はそれらも pop して破棄する
    /// （健全な Markdown では発生しない想定だが、念のためのフォールバック）。
    fn pop_matching(&mut self, pred: impl Fn(&BlockBuilder) -> bool) -> Option<BlockBuilder> {
        let pos = self.stack.iter().rposition(&pred)?;
        // pos 以降を一括で取り出す（pos より上にある builder は捨てる）
        let drained: Vec<BlockBuilder> = self.stack.drain(pos..).collect();
        let mut iter = drained.into_iter();
        let target = iter.next();
        // 残りは捨てる（保険）
        for _ in iter {}
        target
    }

    /// 完成した Block を「現在開いている最内側のコンテナ」に追加する。
    /// 最内側がトップレベル（スタック空）の場合は `self.blocks` に直接追加する。
    /// Heading 直後の TocEntry をここで確定させる。
    fn push_block(&mut self, block: Block) {
        // pending_heading_toc は「次に push される Block の index」を指す。
        // 通常は Heading 自体が次に push される。
        if let Some(mut entry) = self.pending_heading_toc.take() {
            // ただし保険として、現在 push されようとしている block が Heading でない場合は
            // この TocEntry は捨てる（理論上発生しない）。
            if matches!(block, Block::Heading { .. }) {
                let idx = self.next_top_level_index();
                entry.block_index = idx;
                self.toc.push(entry);
            }
        }
        self.append_block_to_top(block);
    }

    /// 次にトップレベル `blocks` に push される時の index を返す。
    /// （現在スタックにブロックが積まれていても、Heading は仕様上トップレベルにのみ出るため、
    ///   常にトップレベルに append される前提で `self.blocks.len()` を返す）
    fn next_top_level_index(&self) -> usize {
        self.blocks.len()
    }

    fn append_block_to_top(&mut self, block: Block) {
        if let Some(top) = self.stack.last_mut() {
            match top {
                BlockBuilder::Item { blocks } => blocks.push(block),
                BlockBuilder::BlockQuote { blocks } => blocks.push(block),
                // Paragraph/Heading/CodeBlock/Table/TableCell/List はブロックを直接受け取れない
                _ => {
                    // フォールバック：トップレベルに流す
                    self.blocks.push(block);
                }
            }
        } else {
            self.blocks.push(block);
        }
    }

    fn builder_into_block(&self, builder: BlockBuilder) -> Block {
        match builder {
            BlockBuilder::Paragraph { mut lines, current } => {
                if !current.is_empty() || lines.is_empty() {
                    lines.push(current);
                }
                Block::Paragraph { lines }
            }
            BlockBuilder::Heading { level, spans } => Block::Heading { level, spans },
            BlockBuilder::List {
                ordered,
                start,
                items,
            } => Block::List {
                ordered,
                start,
                items,
            },
            BlockBuilder::Item { blocks } => {
                // Item 単独で完成することは無いが、保険として Paragraph 化
                Block::Paragraph {
                    lines: vec![blocks
                        .into_iter()
                        .flat_map(|_| Vec::<Span>::new())
                        .collect()],
                }
            }
            BlockBuilder::BlockQuote { blocks } => Block::BlockQuote { blocks },
            BlockBuilder::CodeBlock { lang, code } => Block::CodeBlock { lang, code },
            BlockBuilder::Table {
                header,
                rows,
                align,
                ..
            } => Block::Table {
                header,
                rows,
                align,
            },
            BlockBuilder::TableCell { spans } => {
                // TableCell が Table の外で完成することは無いが、保険
                Block::Paragraph { lines: vec![spans] }
            }
        }
    }
}

fn map_alignment(a: PdAlignment) -> Alignment {
    match a {
        PdAlignment::None => Alignment::None,
        PdAlignment::Left => Alignment::Left,
        PdAlignment::Center => Alignment::Center,
        PdAlignment::Right => Alignment::Right,
    }
}

/// pulldown-cmark の `Tag` がブロック系か（インライン系でないか）を判定する。
/// ブロック系の Start を受けたタイミングで暗黙 Paragraph をクローズするのに用いる。
fn is_block_tag(tag: &Tag) -> bool {
    match tag {
        // ブロック系
        Tag::Paragraph
        | Tag::Heading { .. }
        | Tag::BlockQuote(_)
        | Tag::CodeBlock(_)
        | Tag::HtmlBlock
        | Tag::List(_)
        | Tag::Item
        | Tag::FootnoteDefinition(_)
        | Tag::DefinitionList
        | Tag::DefinitionListTitle
        | Tag::DefinitionListDefinition
        | Tag::Table(_)
        | Tag::TableHead
        | Tag::TableRow
        | Tag::TableCell
        | Tag::MetadataBlock(_) => true,
        // インライン系
        Tag::Emphasis
        | Tag::Strong
        | Tag::Strikethrough
        | Tag::Superscript
        | Tag::Subscript
        | Tag::Link { .. }
        | Tag::Image { .. } => false,
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // ヘルパー
    // -------------------------------------------------------------------

    fn first_paragraph_spans(doc: &Document) -> &[Span] {
        for b in &doc.blocks {
            if let Block::Paragraph { lines } = b {
                if let Some(line) = lines.first() {
                    return line;
                }
            }
        }
        panic!("Paragraph が見つかりません: {:?}", doc.blocks);
    }

    fn first_heading<'a>(doc: &'a Document) -> (&'a u8, &'a [Span]) {
        for b in &doc.blocks {
            if let Block::Heading { level, spans } = b {
                return (level, spans);
            }
        }
        panic!("Heading が見つかりません: {:?}", doc.blocks);
    }

    // -------------------------------------------------------------------
    // 基本要素（旧テスト相当の書き直し）
    // -------------------------------------------------------------------

    #[test]
    fn heading_l1() {
        let doc = parse_markdown("# Foo");
        assert_eq!(doc.toc.len(), 1);
        assert_eq!(doc.toc[0].level, 1);
        assert_eq!(doc.toc[0].title, "Foo");
        assert_eq!(doc.toc[0].block_index, 0);
        let (level, spans) = first_heading(&doc);
        assert_eq!(*level, 1);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "Foo");
        assert_eq!(spans[0].kind, SpanKind::Normal);
    }

    #[test]
    fn heading_levels_2_to_6() {
        let md = "## H2\n\n### H3\n\n#### H4\n\n##### H5\n\n###### H6";
        let doc = parse_markdown(md);
        let levels: Vec<u8> = doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading { level, .. } => Some(*level),
                _ => None,
            })
            .collect();
        assert_eq!(levels, vec![2, 3, 4, 5, 6]);
        assert_eq!(doc.toc.len(), 5);
        for (i, e) in doc.toc.iter().enumerate() {
            assert_eq!(e.level as usize, i + 2);
            assert_eq!(e.block_index, i);
        }
    }

    #[test]
    fn bold() {
        let doc = parse_markdown("**bold**");
        let spans = first_paragraph_spans(&doc);
        assert!(spans
            .iter()
            .any(|s| s.kind == SpanKind::Bold && s.text == "bold"));
    }

    #[test]
    fn italic() {
        let doc = parse_markdown("*italic*");
        let spans = first_paragraph_spans(&doc);
        assert!(spans
            .iter()
            .any(|s| s.kind == SpanKind::Italic && s.text == "italic"));
    }

    #[test]
    fn bold_italic() {
        let doc = parse_markdown("***bolditalic***");
        let spans = first_paragraph_spans(&doc);
        assert!(spans.iter().any(|s| s.kind == SpanKind::BoldItalic));
    }

    #[test]
    fn code_inline() {
        let doc = parse_markdown("`code`");
        let spans = first_paragraph_spans(&doc);
        assert!(spans
            .iter()
            .any(|s| s.kind == SpanKind::CodeInline && s.text == "code"));
    }

    #[test]
    fn fenced_code_block_with_lang() {
        let doc = parse_markdown("```rust\nfn main(){}\n```");
        let block = doc.blocks.first().expect("blocks is empty");
        match block {
            Block::CodeBlock { lang, code } => {
                assert_eq!(lang.as_deref(), Some("rust"));
                assert_eq!(code, "fn main(){}");
            }
            other => panic!("CodeBlock を期待: {:?}", other),
        }
    }

    #[test]
    fn fenced_code_block_without_lang() {
        let doc = parse_markdown("```\ncode\n```");
        let block = doc.blocks.first().unwrap();
        match block {
            Block::CodeBlock { lang, code } => {
                assert!(lang.is_none());
                assert_eq!(code, "code");
            }
            other => panic!("CodeBlock を期待: {:?}", other),
        }
    }

    #[test]
    fn link_in_paragraph() {
        let doc = parse_markdown("[text](https://example.com)");
        let spans = first_paragraph_spans(&doc);
        let link = spans
            .iter()
            .find(|s| matches!(&s.kind, SpanKind::Link { .. }))
            .unwrap();
        match &link.kind {
            SpanKind::Link { url } => assert_eq!(url, "https://example.com"),
            _ => unreachable!(),
        }
        assert_eq!(link.text, "text");
    }

    #[test]
    fn heading_with_link() {
        let doc = parse_markdown("# [text](https://example.com)");
        let (level, spans) = first_heading(&doc);
        assert_eq!(*level, 1);
        let link = spans
            .iter()
            .find(|s| matches!(&s.kind, SpanKind::Link { .. }))
            .unwrap();
        match &link.kind {
            SpanKind::Link { url } => assert_eq!(url, "https://example.com"),
            _ => unreachable!(),
        }
        // TOC タイトルは内側テキスト
        assert_eq!(doc.toc[0].title, "text");
    }

    #[test]
    fn unordered_list_marker() {
        let doc = parse_markdown("- item");
        let block = doc.blocks.first().unwrap();
        match block {
            Block::List {
                ordered,
                start,
                items,
            } => {
                assert!(!*ordered);
                assert!(start.is_none());
                assert_eq!(items.len(), 1);
            }
            other => panic!("List を期待: {:?}", other),
        }
    }

    #[test]
    fn horizontal_rule() {
        let doc = parse_markdown("---");
        assert!(matches!(doc.blocks.first(), Some(Block::Rule)));
    }

    #[test]
    fn serde_roundtrip() {
        let md = "# Title\n\n**bold** and *italic*\n\n- item\n";
        let doc = parse_markdown(md);
        let json = serde_json::to_string(&doc).unwrap();
        let doc2: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, doc2);
    }

    // -------------------------------------------------------------------
    // 新規テスト
    // -------------------------------------------------------------------

    #[test]
    fn ordered_list_with_start() {
        let doc = parse_markdown("3. three\n4. four");
        let block = doc.blocks.first().unwrap();
        match block {
            Block::List {
                ordered,
                start,
                items,
            } => {
                assert!(*ordered);
                assert_eq!(*start, Some(3));
                assert_eq!(items.len(), 2);
            }
            other => panic!("List を期待: {:?}", other),
        }
    }

    #[test]
    fn nested_list_two_levels() {
        let md = "- a\n  - b\n  - c\n- d";
        let doc = parse_markdown(md);
        let outer = match doc.blocks.first().unwrap() {
            Block::List { items, .. } => items,
            other => panic!("List を期待: {:?}", other),
        };
        assert_eq!(outer.len(), 2);
        // 1 番目の Item の中にネスト List があるはず
        let inner_block_count = outer[0].blocks.len();
        assert!(
            inner_block_count >= 2,
            "ネスト List が組み立てられていない: {:?}",
            outer[0]
        );
        let nested = outer[0]
            .blocks
            .iter()
            .find(|b| matches!(b, Block::List { .. }));
        assert!(
            nested.is_some(),
            "ネスト List が見つかりません: {:?}",
            outer[0]
        );
        if let Some(Block::List {
            items: inner_items, ..
        }) = nested
        {
            assert_eq!(inner_items.len(), 2);
        }
    }

    #[test]
    fn block_quote_basic() {
        let doc = parse_markdown("> hello\n> world");
        let block = doc.blocks.first().unwrap();
        match block {
            Block::BlockQuote { blocks } => {
                // 中に Paragraph が 1 つ
                assert!(matches!(blocks.first(), Some(Block::Paragraph { .. })));
            }
            other => panic!("BlockQuote を期待: {:?}", other),
        }
    }

    #[test]
    fn nested_block_quote() {
        let md = "> outer\n>\n> > inner";
        let doc = parse_markdown(md);
        let outer = match doc.blocks.first().unwrap() {
            Block::BlockQuote { blocks } => blocks,
            other => panic!("BlockQuote を期待: {:?}", other),
        };
        let inner = outer.iter().find(|b| matches!(b, Block::BlockQuote { .. }));
        assert!(
            inner.is_some(),
            "ネスト BlockQuote が見つかりません: {:?}",
            outer
        );
    }

    #[test]
    fn list_item_with_paragraph_and_codeblock() {
        let md = "- first paragraph\n\n  ```\n  code\n  ```\n";
        let doc = parse_markdown(md);
        let items = match doc.blocks.first().unwrap() {
            Block::List { items, .. } => items,
            other => panic!("List を期待: {:?}", other),
        };
        assert_eq!(items.len(), 1);
        let blocks = &items[0].blocks;
        assert!(blocks.iter().any(|b| matches!(b, Block::Paragraph { .. })));
        assert!(blocks.iter().any(|b| matches!(b, Block::CodeBlock { .. })));
    }

    #[test]
    fn schema_version_is_current() {
        let doc = parse_markdown("# hello");
        assert_eq!(doc.schema_version, crate::types::SCHEMA_VERSION);
        assert_eq!(doc.schema_version, 2);
    }

    #[test]
    fn empty_document() {
        let doc = parse_markdown("");
        assert!(doc.blocks.is_empty());
        assert!(doc.toc.is_empty());
    }

    #[test]
    fn paragraph_hard_break() {
        // CommonMark の hard break: 行末バックスラッシュ
        let md = "line1\\\nline2";
        let doc = parse_markdown(md);
        let block = doc.blocks.first().unwrap();
        match block {
            Block::Paragraph { lines } => {
                assert_eq!(
                    lines.len(),
                    2,
                    "HardBreak で 2 行になっていない: {:?}",
                    lines
                );
                assert!(lines[0].iter().any(|s| s.text.contains("line1")));
                assert!(lines[1].iter().any(|s| s.text.contains("line2")));
            }
            other => panic!("Paragraph を期待: {:?}", other),
        }
    }

    #[test]
    fn table_basic() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n";
        let doc = parse_markdown(md);
        let block = doc.blocks.first().unwrap();
        match block {
            Block::Table {
                header,
                rows,
                align,
            } => {
                assert_eq!(header.len(), 2);
                assert_eq!(header[0].spans[0].text, "a");
                assert_eq!(header[1].spans[0].text, "b");
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0][0].spans[0].text, "1");
                assert_eq!(rows[1][1].spans[0].text, "4");
                assert_eq!(align.len(), 2);
            }
            other => panic!("Table を期待: {:?}", other),
        }
    }

    #[test]
    fn table_with_alignment() {
        let md = "| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n";
        let doc = parse_markdown(md);
        let block = doc.blocks.first().unwrap();
        match block {
            Block::Table { align, .. } => {
                assert_eq!(
                    align,
                    &vec![Alignment::Left, Alignment::Center, Alignment::Right]
                );
            }
            other => panic!("Table を期待: {:?}", other),
        }
    }

    #[test]
    fn toc_block_index_with_intermediate_paragraphs() {
        let md = "intro\n\n# H1\n\nbody\n\n## H2\n";
        let doc = parse_markdown(md);
        // 期待: Paragraph, Heading(H1), Paragraph, Heading(H2)
        assert_eq!(doc.blocks.len(), 4);
        assert_eq!(doc.toc.len(), 2);
        assert_eq!(doc.toc[0].block_index, 1);
        assert_eq!(doc.toc[0].title, "H1");
        assert_eq!(doc.toc[1].block_index, 3);
        assert_eq!(doc.toc[1].title, "H2");
    }

    #[test]
    fn link_url_safety_preserved() {
        // フェーズ 1 では URL のサニタイズはせず生のまま保持する
        let doc = parse_markdown("[click](javascript:alert(1))");
        let spans = first_paragraph_spans(&doc);
        let link = spans
            .iter()
            .find(|s| matches!(&s.kind, SpanKind::Link { .. }))
            .unwrap();
        if let SpanKind::Link { url } = &link.kind {
            assert!(url.starts_with("javascript:"));
        }
    }
}
