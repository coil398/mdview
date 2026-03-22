use mdview_core::{Document, SpanKind, TocEntry};
use ratatui::style::{Color, Modifier, Style};

use crate::highlighter::Highlighter;
use crate::types::{StyledLine, StyledSpan};

pub fn span_kind_to_style(kind: &SpanKind) -> Style {
    match kind {
        SpanKind::Heading(1) => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        SpanKind::Heading(2) => Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        SpanKind::Heading(_) => Style::default().fg(Color::Green),
        SpanKind::Bold => Style::default().add_modifier(Modifier::BOLD),
        SpanKind::Italic => Style::default().add_modifier(Modifier::ITALIC),
        SpanKind::BoldItalic => Style::default().add_modifier(Modifier::BOLD | Modifier::ITALIC),
        SpanKind::CodeInline => Style::default().fg(Color::Yellow),
        SpanKind::Link { .. } => Style::default().fg(Color::Cyan),
        SpanKind::ListMarker => Style::default().fg(Color::Yellow),
        SpanKind::BlockQuote => Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        SpanKind::Rule => Style::default().fg(Color::DarkGray),
        SpanKind::CodeBlock { .. } => Style::default(),
        SpanKind::Normal => Style::default(),
    }
}

pub fn convert_document(
    doc: &Document,
    hl: &Highlighter,
) -> (Vec<StyledLine>, Vec<TocEntry>) {
    let badge_style = Style::default()
        .fg(Color::Black)
        .bg(Color::Green)
        .add_modifier(Modifier::BOLD);

    let mut styled_lines: Vec<StyledLine> = Vec::new();

    // コードブロックの連続するSpanを検出してまとめてハイライト
    // Document.lines は core パーサーが生成した行リスト
    // CodeBlock の行をまとめてhighlighterに渡す必要がある
    //
    // 方針: lines を走査し、CodeBlock spanを持つ連続行を検出したら
    // 一度バッジ行を出力し、その後 highlighterでハイライトした行を追加

    let mut i = 0;
    while i < doc.lines.len() {
        let line = &doc.lines[i];

        // この行がCodeBlock spanだけで構成されているか確認
        if line.iter().any(|s| matches!(s.kind, SpanKind::CodeBlock { .. })) {
            // lang を取得（最初のCodeBlock spanから）
            let lang = line.iter().find_map(|s| {
                if let SpanKind::CodeBlock { lang } = &s.kind {
                    Some(lang.clone().unwrap_or_default())
                } else {
                    None
                }
            }).unwrap_or_default();

            // コードブロックの全行を収集
            let mut code_lines: Vec<String> = Vec::new();
            let mut j = i;
            while j < doc.lines.len() {
                let l = &doc.lines[j];
                if l.iter().any(|s| matches!(s.kind, SpanKind::CodeBlock { .. })) {
                    let text = l.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("");
                    code_lines.push(text);
                    j += 1;
                } else {
                    break;
                }
            }

            // バッジ行
            let badge_text = if lang.is_empty() {
                " code ".to_string()
            } else {
                format!(" {} ", lang)
            };
            styled_lines.push(vec![StyledSpan { text: badge_text, style: badge_style }]);

            // ハイライト
            let code = code_lines.join("\n");
            let highlighted = hl.highlight_code(&code, &lang);
            for hl_line in highlighted {
                let mut line: StyledLine = vec![StyledSpan {
                    text: "  ".to_string(),
                    style: Style::default(),
                }];
                line.extend(hl_line);
                styled_lines.push(line);
            }

            i = j;
        } else if line.is_empty() {
            // 空行
            styled_lines.push(Vec::new());
            i += 1;
        } else {
            // 通常行
            let mut styled_line: StyledLine = Vec::new();
            let mut heading_level: Option<u8> = None;

            // 見出しレベルを検出（プレフィックス付加のため）
            for span in line {
                if let SpanKind::Heading(lvl) = span.kind {
                    heading_level = Some(lvl);
                    break;
                }
            }

            // 見出しプレフィックスを先頭に追加
            if let Some(lvl) = heading_level {
                let (prefix, style) = match lvl {
                    1 => ("█ ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    2 => ("▌ ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
                    _ => ("  ▸ ", Style::default().fg(Color::Green)),
                };
                styled_line.push(StyledSpan { text: prefix.to_string(), style });
            }

            for span in line {
                let style = span_kind_to_style(&span.kind);
                // Rule は特殊処理
                if matches!(span.kind, SpanKind::Rule) {
                    styled_line.push(StyledSpan {
                        text: "─".repeat(60),
                        style,
                    });
                } else {
                    styled_line.push(StyledSpan {
                        text: span.text.clone(),
                        style,
                    });
                }
            }
            styled_lines.push(styled_line);
            i += 1;
        }
    }

    (styled_lines, doc.toc.clone())
}
