use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::types::{Document, Line, Span, SpanKind, TocEntry};

pub fn parse_markdown(text: &str) -> Document {
    let mut lines: Vec<Line> = Vec::new();
    let mut toc: Vec<TocEntry> = Vec::new();

    let mut current_line: Line = Vec::new();

    // インラインの状態管理
    let mut in_heading: Option<u8> = None;
    let mut in_code_block: bool = false;
    let mut code_block_lang: Option<String> = None;
    let mut code_block_content: String = String::new();
    let mut in_strong: bool = false;
    let mut in_emphasis: bool = false;
    let mut in_link: bool = false;
    let mut link_url: String = String::new();
    let mut item_depth: u32 = 0;
    let mut item_prefix_done: bool = false;
    let mut in_blockquote: bool = false;

    let flush_line = |lines: &mut Vec<Line>, current_line: &mut Line| {
        let line = std::mem::take(current_line);
        lines.push(line);
    };

    let options = Options::all();
    let parser = Parser::new_ext(text, options);

    for event in parser {
        match event {
            // ── ブロック開始 ──────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                let lvl = level as u8;
                in_heading = Some(lvl);
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(lvl) = in_heading.take() {
                    let line_index = lines.len();
                    // TOC用テキストを current_line から取得
                    let title = current_line
                        .iter()
                        .map(|s| s.text.as_str())
                        .collect::<Vec<_>>()
                        .join("");
                    toc.push(TocEntry {
                        line_index,
                        title,
                        level: lvl,
                    });
                    flush_line(&mut lines, &mut current_line);
                }
            }

            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) => {
                in_code_block = true;
                let lang_str = lang.to_string();
                code_block_lang = if lang_str.is_empty() {
                    None
                } else {
                    Some(lang_str)
                };
                code_block_content.clear();
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                in_code_block = true;
                code_block_lang = None;
                code_block_content.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                // コードブロックの内容を改行で分割して各行をSpanとして追加
                // 末尾の改行は除去する
                let content = code_block_content.trim_end_matches('\n');
                for code_line in content.split('\n') {
                    lines.push(vec![Span {
                        text: code_line.to_string(),
                        kind: SpanKind::CodeBlock {
                            lang: code_block_lang.clone(),
                        },
                    }]);
                }
                // コードブロック後に空行を追加
                lines.push(Vec::new());
                code_block_content.clear();
                code_block_lang = None;
            }

            Event::Start(Tag::Strong) => {
                in_strong = true;
            }
            Event::End(TagEnd::Strong) => {
                in_strong = false;
            }

            Event::Start(Tag::Emphasis) => {
                in_emphasis = true;
            }
            Event::End(TagEnd::Emphasis) => {
                in_emphasis = false;
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                in_link = true;
                link_url = dest_url.to_string();
            }
            Event::End(TagEnd::Link) => {
                in_link = false;
                link_url.clear();
            }

            Event::Start(Tag::Item) => {
                item_depth += 1;
                if item_depth == 1 {
                    item_prefix_done = false;
                }
            }
            Event::End(TagEnd::Item) => {
                item_depth -= 1;
                item_prefix_done = false;
                if !current_line.is_empty() {
                    flush_line(&mut lines, &mut current_line);
                }
            }

            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                if !current_line.is_empty() {
                    flush_line(&mut lines, &mut current_line);
                }
                // 段落後に空行を追加
                lines.push(Vec::new());
            }

            Event::Start(Tag::BlockQuote(_)) => {
                in_blockquote = true;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                in_blockquote = false;
                if !current_line.is_empty() {
                    flush_line(&mut lines, &mut current_line);
                }
            }

            Event::Start(Tag::List(_)) => {}
            Event::End(TagEnd::List(_)) => {
                // リスト後に空行を追加
                lines.push(Vec::new());
            }

            // ── テキスト ──────────────────────────────────────────────────
            Event::Text(t) => {
                if in_code_block {
                    code_block_content.push_str(&t);
                } else {
                    // リストアイテムのバレット prefix を追加
                    if item_depth > 0 && !item_prefix_done {
                        item_prefix_done = true;
                        current_line.push(Span {
                            text: "  • ".to_string(),
                            kind: SpanKind::ListMarker,
                        });
                    }
                    let kind = current_span_kind(
                        in_heading,
                        in_strong,
                        in_emphasis,
                        in_link,
                        &link_url,
                        in_blockquote,
                    );
                    current_line.push(Span {
                        text: t.to_string(),
                        kind,
                    });
                }
            }

            Event::Code(t) => {
                current_line.push(Span {
                    text: t.to_string(),
                    kind: SpanKind::CodeInline,
                });
            }

            Event::SoftBreak => {
                if !in_code_block {
                    // 段落内のソフトブレークはスペースとして扱う
                    let kind = current_span_kind(
                        in_heading,
                        in_strong,
                        in_emphasis,
                        in_link,
                        &link_url,
                        in_blockquote,
                    );
                    current_line.push(Span {
                        text: " ".to_string(),
                        kind,
                    });
                }
            }

            Event::HardBreak => {
                if !in_code_block {
                    flush_line(&mut lines, &mut current_line);
                }
            }

            Event::Rule => {
                lines.push(vec![Span {
                    text: String::new(),
                    kind: SpanKind::Rule,
                }]);
            }

            _ => {}
        }
    }

    // 残りのコンテンツをフラッシュ
    if !current_line.is_empty() {
        lines.push(current_line);
    }

    Document { lines, toc }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ドキュメント内の全 Span の kind をフラットに収集する
    fn flat_kinds(doc: &Document) -> Vec<SpanKind> {
        doc.lines
            .iter()
            .flat_map(|l| l.iter().map(|s| s.kind.clone()))
            .collect()
    }

    // ── ヘルパー: kind のパターンマッチ ──────────────────────────────────

    fn is_heading(kind: &SpanKind, expected_level: u8) -> bool {
        matches!(kind, SpanKind::Heading(l) if *l == expected_level)
    }

    fn is_bold(kind: &SpanKind) -> bool {
        matches!(kind, SpanKind::Bold)
    }

    fn is_italic(kind: &SpanKind) -> bool {
        matches!(kind, SpanKind::Italic)
    }

    fn is_bold_italic(kind: &SpanKind) -> bool {
        matches!(kind, SpanKind::BoldItalic)
    }

    fn is_code_inline(kind: &SpanKind) -> bool {
        matches!(kind, SpanKind::CodeInline)
    }

    fn is_code_block_with_lang(kind: &SpanKind, expected_lang: Option<&str>) -> bool {
        match kind {
            SpanKind::CodeBlock { lang } => lang.as_deref() == expected_lang,
            _ => false,
        }
    }

    fn is_link(kind: &SpanKind, expected_url: &str) -> bool {
        matches!(kind, SpanKind::Link { url } if url == expected_url)
    }

    fn is_list_marker(kind: &SpanKind) -> bool {
        matches!(kind, SpanKind::ListMarker)
    }

    fn is_rule(kind: &SpanKind) -> bool {
        matches!(kind, SpanKind::Rule)
    }

    // ── テストケース ──────────────────────────────────────────────────────

    #[test]
    fn test_heading_level1() {
        let doc = parse_markdown("# Foo");
        // TOC エントリの確認
        assert_eq!(doc.toc.len(), 1);
        let entry = &doc.toc[0];
        assert_eq!(entry.level, 1);
        assert_eq!(entry.title, "Foo");
        assert_eq!(entry.line_index, 0);
        // SpanKind::Heading(1) を含む
        let kinds = flat_kinds(&doc);
        assert!(
            kinds.iter().any(|k| is_heading(k, 1)),
            "SpanKind::Heading(1) が見つかりません: {kinds:?}"
        );
    }

    #[test]
    fn test_bold() {
        let doc = parse_markdown("**bold**");
        let kinds = flat_kinds(&doc);
        assert!(
            kinds.iter().any(|k| is_bold(k)),
            "SpanKind::Bold が見つかりません: {kinds:?}"
        );
    }

    #[test]
    fn test_italic() {
        let doc = parse_markdown("*italic*");
        let kinds = flat_kinds(&doc);
        assert!(
            kinds.iter().any(|k| is_italic(k)),
            "SpanKind::Italic が見つかりません: {kinds:?}"
        );
    }

    #[test]
    fn test_bold_italic() {
        let doc = parse_markdown("***bolditalic***");
        let kinds = flat_kinds(&doc);
        assert!(
            kinds.iter().any(|k| is_bold_italic(k)),
            "SpanKind::BoldItalic が見つかりません: {kinds:?}"
        );
    }

    #[test]
    fn test_code_inline() {
        let doc = parse_markdown("`code`");
        let kinds = flat_kinds(&doc);
        assert!(
            kinds.iter().any(|k| is_code_inline(k)),
            "SpanKind::CodeInline が見つかりません: {kinds:?}"
        );
    }

    #[test]
    fn test_fenced_code_block_with_lang() {
        let md = "```rust\nfn main(){}\n```";
        let doc = parse_markdown(md);
        // 行ごとに分割されているので少なくとも1行は CodeBlock { lang: Some("rust") }
        assert!(
            doc.lines.iter().flatten().any(|s| is_code_block_with_lang(&s.kind, Some("rust"))),
            "SpanKind::CodeBlock {{ lang: Some(\"rust\") }} が見つかりません"
        );
        // テキストに "fn main(){}" を含む行があること
        let has_content = doc.lines.iter().flatten().any(|s| {
            is_code_block_with_lang(&s.kind, Some("rust")) && s.text.contains("fn main()")
        });
        assert!(has_content, "コードブロックの内容が正しく分割されていません");
    }

    #[test]
    fn test_fenced_code_block_without_lang() {
        let md = "```\ncode\n```";
        let doc = parse_markdown(md);
        assert!(
            doc.lines.iter().flatten().any(|s| is_code_block_with_lang(&s.kind, None)),
            "SpanKind::CodeBlock {{ lang: None }} が見つかりません"
        );
    }

    #[test]
    fn test_link() {
        let doc = parse_markdown("[text](https://example.com)");
        let kinds = flat_kinds(&doc);
        assert!(
            kinds.iter().any(|k| is_link(k, "https://example.com")),
            "SpanKind::Link {{ url: \"https://example.com\" }} が見つかりません: {kinds:?}"
        );
    }

    #[test]
    fn test_heading_with_link() {
        // 見出し内リンクは SpanKind::Link が優先される
        let doc = parse_markdown("# [text](https://example.com)");
        let kinds = flat_kinds(&doc);
        assert!(
            kinds.iter().any(|k| is_link(k, "https://example.com")),
            "見出し内リンクで SpanKind::Link が見つかりません: {kinds:?}"
        );
    }

    #[test]
    fn test_list_marker() {
        let doc = parse_markdown("- item");
        let kinds = flat_kinds(&doc);
        assert!(
            kinds.iter().any(|k| is_list_marker(k)),
            "SpanKind::ListMarker が見つかりません: {kinds:?}"
        );
    }

    #[test]
    fn test_horizontal_rule() {
        let doc = parse_markdown("---");
        let kinds = flat_kinds(&doc);
        assert!(
            kinds.iter().any(|k| is_rule(k)),
            "SpanKind::Rule が見つかりません: {kinds:?}"
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let md = "# Title\n\n**bold** and *italic*\n\n- list item\n";
        let doc = parse_markdown(md);
        let json = serde_json::to_string(&doc).expect("シリアライズ失敗");
        let doc2: Document = serde_json::from_str(&json).expect("デシリアライズ失敗");
        // 行数・TOC数が一致することを確認
        assert_eq!(doc.lines.len(), doc2.lines.len(), "lines の長さが一致しません");
        assert_eq!(doc.toc.len(), doc2.toc.len(), "toc の長さが一致しません");
        // 各行の Span 数が一致することを確認
        for (i, (line1, line2)) in doc.lines.iter().zip(doc2.lines.iter()).enumerate() {
            assert_eq!(
                line1.len(),
                line2.len(),
                "lines[{i}] の Span 数が一致しません"
            );
        }
        // TOC エントリの内容が一致することを確認
        for (i, (entry1, entry2)) in doc.toc.iter().zip(doc2.toc.iter()).enumerate() {
            assert_eq!(entry1.level, entry2.level, "toc[{i}].level が一致しません");
            assert_eq!(entry1.title, entry2.title, "toc[{i}].title が一致しません");
            assert_eq!(
                entry1.line_index,
                entry2.line_index,
                "toc[{i}].line_index が一致しません"
            );
        }
    }
}

fn current_span_kind(
    in_heading: Option<u8>,
    in_strong: bool,
    in_emphasis: bool,
    in_link: bool,
    link_url: &str,
    in_blockquote: bool,
) -> SpanKind {
    if in_link {
        return SpanKind::Link {
            url: link_url.to_string(),
        };
    }
    if let Some(level) = in_heading {
        return SpanKind::Heading(level);
    }
    if in_strong && in_emphasis {
        return SpanKind::BoldItalic;
    }
    if in_strong {
        return SpanKind::Bold;
    }
    if in_emphasis {
        return SpanKind::Italic;
    }
    if in_blockquote {
        return SpanKind::BlockQuote;
    }
    SpanKind::Normal
}
