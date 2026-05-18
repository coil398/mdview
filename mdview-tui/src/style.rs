//! Document（Block ツリー）→ TUI 行列への変換。
//!
//! 各 [`mdview_core::Block`] を再帰的に行へ展開し、同時に
//! 「block_index → 開始行 index」マップ（`block_starts`）を構築する。
//! TOC ジャンプはこのマップを参照して `scroll = block_starts[entry.block_index]` で行う。

use mdview_core::{Alignment, Block, Cell, Document, ListItem, Span, SpanKind, TocEntry};
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::highlighter::Highlighter;
use crate::theme::TuiTheme;
use crate::types::{StyledLine, StyledSpan};

// テーブル描画パラメータ（各列のコンテンツ最大幅を min/max でクランプ）
const TABLE_COL_MIN_WIDTH: usize = 3;
const TABLE_COL_MAX_WIDTH: usize = 40;

#[derive(Debug)]
pub struct StyledOutput {
    pub lines: Vec<StyledLine>,
    /// `Document.blocks[i]` の描画開始行 index。
    pub block_starts: Vec<usize>,
    pub toc: Vec<TocEntry>,
}

/// Span 単体 → ratatui Style への変換。
/// 見出し色などの「コンテキスト依存スタイル」はここでは扱わず、別途付与する。
pub fn span_kind_to_style(kind: &SpanKind, theme: &TuiTheme) -> Style {
    match kind {
        SpanKind::Bold => Style::default().add_modifier(Modifier::BOLD),
        SpanKind::Italic => Style::default().add_modifier(Modifier::ITALIC),
        SpanKind::BoldItalic => Style::default().add_modifier(Modifier::BOLD | Modifier::ITALIC),
        SpanKind::CodeInline => Style::default().fg(theme.code_inline),
        SpanKind::Link { .. } => Style::default()
            .fg(theme.link)
            .add_modifier(Modifier::UNDERLINED),
        SpanKind::Normal => Style::default(),
    }
}

/// Heading レベルに応じた見出しスタイル（行プレフィックスとテキスト両方に適用）。
fn heading_style(level: u8, theme: &TuiTheme) -> Style {
    match level {
        1 => Style::default()
            .fg(theme.heading1)
            .add_modifier(Modifier::BOLD),
        2 => Style::default()
            .fg(theme.heading2)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(theme.heading3_plus),
    }
}

fn heading_prefix(level: u8) -> &'static str {
    match level {
        1 => "█ ",
        2 => "▌ ",
        _ => "  ▸ ",
    }
}

pub fn convert_document(doc: &Document, hl: &Highlighter, theme: &TuiTheme) -> StyledOutput {
    let mut ctx = RenderCtx::new();
    let mut block_starts = Vec::with_capacity(doc.blocks.len());
    for (idx, block) in doc.blocks.iter().enumerate() {
        // 連続する Heading 以外のブロックの間に空行を入れる（既存挙動の踏襲）
        if idx > 0 && !ctx.lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            ctx.lines.push(Vec::new());
        }
        block_starts.push(ctx.lines.len());
        render_block(block, &mut ctx, hl, theme, 0, 0);
    }
    StyledOutput {
        lines: ctx.lines,
        block_starts,
        toc: doc.toc.clone(),
    }
}

// ===========================================================================
// 内部
// ===========================================================================

struct RenderCtx {
    lines: Vec<StyledLine>,
}

impl RenderCtx {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }
}

fn render_block(
    block: &Block,
    ctx: &mut RenderCtx,
    hl: &Highlighter,
    theme: &TuiTheme,
    indent: usize,
    quote_depth: usize,
) {
    match block {
        Block::Paragraph { lines } => render_paragraph(lines, ctx, theme, indent, quote_depth),
        Block::Heading { level, spans } => render_heading(*level, spans, ctx, theme, quote_depth),
        Block::List {
            ordered,
            start,
            items,
        } => render_list(*ordered, *start, items, ctx, hl, theme, indent, quote_depth),
        Block::BlockQuote { blocks } => {
            for (i, b) in blocks.iter().enumerate() {
                if i > 0 && !ctx.lines.last().map(|l| l.is_empty()).unwrap_or(false) {
                    push_empty_line(ctx, indent, quote_depth + 1);
                }
                render_block(b, ctx, hl, theme, indent, quote_depth + 1);
            }
        }
        Block::CodeBlock { lang, code } => {
            render_code_block(lang, code, ctx, hl, theme, indent, quote_depth)
        }
        Block::Table {
            header,
            rows,
            align,
        } => render_table(header, rows, align, ctx, theme, indent, quote_depth),
        Block::Rule => render_rule(ctx, theme, indent, quote_depth),
    }
}

fn render_paragraph(
    para_lines: &[Vec<Span>],
    ctx: &mut RenderCtx,
    theme: &TuiTheme,
    indent: usize,
    quote_depth: usize,
) {
    for line_spans in para_lines {
        let mut line: StyledLine = Vec::new();
        push_indent(&mut line, indent);
        push_quote_prefix(&mut line, theme, quote_depth);
        for span in line_spans {
            let url = match &span.kind {
                SpanKind::Link { url } => Some(url.clone()),
                _ => None,
            };
            line.push(StyledSpan {
                text: span.text.clone(),
                style: span_kind_to_style(&span.kind, theme),
                url,
            });
        }
        ctx.lines.push(line);
    }
}

fn render_heading(
    level: u8,
    spans: &[Span],
    ctx: &mut RenderCtx,
    theme: &TuiTheme,
    quote_depth: usize,
) {
    let mut line: StyledLine = Vec::new();
    push_quote_prefix(&mut line, theme, quote_depth);
    let style = heading_style(level, theme);
    line.push(StyledSpan {
        text: heading_prefix(level).to_string(),
        style,
        url: None,
    });
    for span in spans {
        // 見出し内では「見出し色 + Span 由来の修飾子」を合成。
        // ただしリンクは特別扱い: 見出し色 + UNDERLINED で表現（URL は表示しない）
        let (span_style, url) = match &span.kind {
            SpanKind::Link { url } => (style.add_modifier(Modifier::UNDERLINED), Some(url.clone())),
            SpanKind::CodeInline => (style.fg(theme.code_inline), None),
            SpanKind::Bold => (style.add_modifier(Modifier::BOLD), None),
            SpanKind::Italic => (style.add_modifier(Modifier::ITALIC), None),
            SpanKind::BoldItalic => (style.add_modifier(Modifier::BOLD | Modifier::ITALIC), None),
            SpanKind::Normal => (style, None),
        };
        line.push(StyledSpan {
            text: span.text.clone(),
            style: span_style,
            url,
        });
    }
    ctx.lines.push(line);
}

#[allow(clippy::too_many_arguments)]
fn render_list(
    ordered: bool,
    start: Option<u64>,
    items: &[ListItem],
    ctx: &mut RenderCtx,
    hl: &Highlighter,
    theme: &TuiTheme,
    indent: usize,
    quote_depth: usize,
) {
    let bullet_style = Style::default().fg(theme.list_bullet);
    let mut counter = start.unwrap_or(1);
    for (i, item) in items.iter().enumerate() {
        // 項目間の見やすさのため、複数ブロックを含む item の前後では空行を入れる
        if i > 0
            && item.blocks.len() > 1
            && !ctx.lines.last().map(|l| l.is_empty()).unwrap_or(false)
        {
            push_empty_line(ctx, indent, quote_depth);
        }
        // 最初のブロック行の先頭にバレットを入れる必要があるので、
        // まず item の最初のブロックを通常通りレンダリングし、その先頭行にマーカーを差し込む
        let line_idx_before = ctx.lines.len();
        for (j, b) in item.blocks.iter().enumerate() {
            if j > 0 && !ctx.lines.last().map(|l| l.is_empty()).unwrap_or(false) {
                push_empty_line(ctx, indent + 1, quote_depth);
            }
            render_block(b, ctx, hl, theme, indent + 1, quote_depth);
        }
        // バレットを最初の行の indent 直後に挿入
        if line_idx_before < ctx.lines.len() {
            let first_line = &mut ctx.lines[line_idx_before];
            let bullet_text = if ordered {
                format!("{}. ", counter)
            } else {
                "• ".to_string()
            };
            // quote_depth ぶんと indent ぶんの prefix を飛ばして挿入
            let insert_pos =
                quote_prefix_span_count(quote_depth) + indent_span_count_for_item_marker(indent);
            // 既存の先頭 indent (indent+1 段) から 2 文字分（バレット幅）を削って差し替え
            let bullet_span = StyledSpan {
                text: bullet_text,
                style: bullet_style,
                url: None,
            };
            first_line.insert(insert_pos, bullet_span);
        }
        counter += 1;
    }
}

/// quote prefix が占める span 数（`push_quote_prefix` と一致させる）
fn quote_prefix_span_count(quote_depth: usize) -> usize {
    if quote_depth == 0 {
        0
    } else {
        1
    }
}

/// `render_list` のバレット挿入位置計算用。
/// `push_indent` は indent を 1 つの span として push しているため。
fn indent_span_count_for_item_marker(indent: usize) -> usize {
    if indent == 0 {
        0
    } else {
        1
    }
}

fn render_code_block(
    lang: &Option<String>,
    code: &str,
    ctx: &mut RenderCtx,
    hl: &Highlighter,
    theme: &TuiTheme,
    indent: usize,
    quote_depth: usize,
) {
    let badge_style = Style::default()
        .fg(theme.code_badge_fg)
        .bg(theme.code_badge_bg)
        .add_modifier(Modifier::BOLD);
    let lang_display = lang.as_deref().unwrap_or("");
    let badge_text = if lang_display.is_empty() {
        " code ".to_string()
    } else {
        format!(" {} ", lang_display)
    };
    let mut badge_line: StyledLine = Vec::new();
    push_indent(&mut badge_line, indent);
    push_quote_prefix(&mut badge_line, theme, quote_depth);
    badge_line.push(StyledSpan {
        text: badge_text,
        style: badge_style,
        url: None,
    });
    ctx.lines.push(badge_line);

    let highlighted = hl.highlight_code(code, lang_display);
    for hl_line in highlighted {
        let mut line: StyledLine = Vec::new();
        push_indent(&mut line, indent);
        push_quote_prefix(&mut line, theme, quote_depth);
        // コード本体の前にインデント 2 文字分
        line.push(StyledSpan {
            text: "  ".to_string(),
            style: Style::default(),
            url: None,
        });
        line.extend(hl_line);
        ctx.lines.push(line);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_table(
    header: &[Cell],
    rows: &[Vec<Cell>],
    align: &[Alignment],
    ctx: &mut RenderCtx,
    theme: &TuiTheme,
    indent: usize,
    quote_depth: usize,
) {
    let cols = header.len();
    if cols == 0 {
        return;
    }
    let col_widths = compute_table_col_widths(header, rows);
    let separator: String = (0..cols)
        .map(|i| "─".repeat(col_widths[i]))
        .collect::<Vec<_>>()
        .join("─┼─");

    let border_style = Style::default().fg(theme.table_border);

    // ヘッダ行（全 span に BOLD を合成）
    let mut header_line: StyledLine = Vec::new();
    push_indent(&mut header_line, indent);
    push_quote_prefix(&mut header_line, theme, quote_depth);
    for (i, (cell, &width)) in header.iter().zip(col_widths.iter()).enumerate() {
        if i > 0 {
            header_line.push(StyledSpan {
                text: " │ ".to_string(),
                style: border_style,
                url: None,
            });
        }
        let col_align = align.get(i).copied().unwrap_or(Alignment::Left);
        let mut cell_spans = render_cell_to_spans(cell, width, col_align, theme);
        // ヘッダは全 span に BOLD を合成
        for s in &mut cell_spans {
            s.style = s.style.add_modifier(Modifier::BOLD);
        }
        header_line.extend(cell_spans);
    }
    ctx.lines.push(header_line);

    // 区切り行
    let mut sep_line: StyledLine = Vec::new();
    push_indent(&mut sep_line, indent);
    push_quote_prefix(&mut sep_line, theme, quote_depth);
    sep_line.push(StyledSpan {
        text: separator.clone(),
        style: border_style,
        url: None,
    });
    ctx.lines.push(sep_line);

    // データ行
    let empty_cell = Cell { spans: vec![] };
    for row in rows {
        let mut line: StyledLine = Vec::new();
        push_indent(&mut line, indent);
        push_quote_prefix(&mut line, theme, quote_depth);
        for (i, &width) in col_widths.iter().enumerate() {
            if i > 0 {
                line.push(StyledSpan {
                    text: " │ ".to_string(),
                    style: border_style,
                    url: None,
                });
            }
            let col_align = align.get(i).copied().unwrap_or(Alignment::Left);
            let cell = row.get(i).unwrap_or(&empty_cell);
            line.extend(render_cell_to_spans(cell, width, col_align, theme));
        }
        ctx.lines.push(line);
    }
}

/// セル内の Span をスタイル化し、幅にアラインして `Vec<StyledSpan>` を返す。
///
/// - 左 padding / 右 padding をアラインメントに応じて計算する。
/// - 切り詰めが必要な場合は `width - 1` まで取り `…`（U+2026）を最後に付ける。
fn render_cell_to_spans(
    cell: &Cell,
    width: usize,
    align: Alignment,
    theme: &TuiTheme,
) -> Vec<StyledSpan> {
    // まずセル内の全 span を連結した display width を計測する
    let total_content_width: usize = cell
        .spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.text.as_str()))
        .sum();

    if total_content_width <= width {
        // パディングが必要なケース
        let pad = width - total_content_width;
        let (left_pad, right_pad) = match align {
            Alignment::Right => (pad, 0usize),
            Alignment::Center => {
                let left = pad / 2;
                let right = pad - left;
                (left, right)
            }
            Alignment::Left | Alignment::None => (0usize, pad),
        };

        let mut result: Vec<StyledSpan> = Vec::new();
        if left_pad > 0 {
            result.push(StyledSpan {
                text: " ".repeat(left_pad),
                style: Style::default(),
                url: None,
            });
        }
        for span in &cell.spans {
            let url = match &span.kind {
                SpanKind::Link { url } => Some(url.clone()),
                _ => None,
            };
            result.push(StyledSpan {
                text: span.text.clone(),
                style: span_kind_to_style(&span.kind, theme),
                url,
            });
        }
        if right_pad > 0 {
            result.push(StyledSpan {
                text: " ".repeat(right_pad),
                style: Style::default(),
                url: None,
            });
        }
        result
    } else {
        // 切り詰めが必要なケース（width - 1 まで取り `…` を付ける）
        let target_width = if width > 0 { width - 1 } else { 0 };
        let mut result: Vec<StyledSpan> = Vec::new();
        let mut acc = 0usize;
        'outer: for span in &cell.spans {
            let url = match &span.kind {
                SpanKind::Link { url } => Some(url.clone()),
                _ => None,
            };
            let span_style = span_kind_to_style(&span.kind, theme);
            let mut span_text = String::new();
            for ch in span.text.chars() {
                let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                if acc + cw > target_width {
                    // このスパンのここまでを push して外に出る
                    if !span_text.is_empty() {
                        result.push(StyledSpan {
                            text: span_text,
                            style: span_style,
                            url: url.clone(),
                        });
                    }
                    break 'outer;
                }
                span_text.push(ch);
                acc += cw;
            }
            if !span_text.is_empty() {
                result.push(StyledSpan {
                    text: span_text,
                    style: span_style,
                    url,
                });
            }
        }
        // 省略記号を末尾に付ける
        result.push(StyledSpan {
            text: "…".to_string(),
            style: Style::default(),
            url: None,
        });
        // width になるよう末尾パディング（`…` は width 1 なので acc + 1 と width の差を埋める）
        let current = acc + 1; // `…` は幅 1
        if current < width {
            result.push(StyledSpan {
                text: " ".repeat(width - current),
                style: Style::default(),
                url: None,
            });
        }
        result
    }
}

fn cell_to_plain_text(cell: &Cell) -> String {
    cell.spans
        .iter()
        .map(|s| s.text.as_str())
        .collect::<String>()
}


/// 各列の display width を、ヘッダと全行の最大値から決める（min/max でクランプ）。
fn compute_table_col_widths(header: &[Cell], rows: &[Vec<Cell>]) -> Vec<usize> {
    let cols = header.len();
    let mut widths = vec![TABLE_COL_MIN_WIDTH; cols];
    for (i, cell) in header.iter().enumerate() {
        let w = UnicodeWidthStr::width(cell_to_plain_text(cell).as_str());
        widths[i] = widths[i].max(w);
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i >= cols {
                break; // 行が header より長い異常ケースは切り捨て
            }
            let w = UnicodeWidthStr::width(cell_to_plain_text(cell).as_str());
            widths[i] = widths[i].max(w);
        }
    }
    for w in widths.iter_mut() {
        *w = (*w).clamp(TABLE_COL_MIN_WIDTH, TABLE_COL_MAX_WIDTH);
    }
    widths
}

fn render_rule(ctx: &mut RenderCtx, theme: &TuiTheme, indent: usize, quote_depth: usize) {
    let mut line: StyledLine = Vec::new();
    push_indent(&mut line, indent);
    push_quote_prefix(&mut line, theme, quote_depth);
    line.push(StyledSpan {
        text: "─".repeat(60),
        style: Style::default().fg(theme.rule),
        url: None,
    });
    ctx.lines.push(line);
}

// ===========================================================================
// プレフィックスユーティリティ
// ===========================================================================

fn push_indent(line: &mut StyledLine, indent: usize) {
    if indent > 0 {
        line.push(StyledSpan {
            text: "  ".repeat(indent),
            style: Style::default(),
            url: None,
        });
    }
}

fn push_quote_prefix(line: &mut StyledLine, theme: &TuiTheme, quote_depth: usize) {
    if quote_depth > 0 {
        line.push(StyledSpan {
            text: "│ ".repeat(quote_depth),
            style: Style::default()
                .fg(theme.quote_prefix)
                .add_modifier(Modifier::ITALIC),
            url: None,
        });
    }
}

fn push_empty_line(ctx: &mut RenderCtx, _indent: usize, _quote_depth: usize) {
    // 空行に prefix は付けない（見た目をすっきりさせる）
    ctx.lines.push(Vec::new());
}

// ===========================================================================
// テスト
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use mdview_core::parser::parse_markdown;

    fn render(md: &str) -> StyledOutput {
        let doc = parse_markdown(md);
        let hl = Highlighter::new();
        let theme = TuiTheme::default();
        convert_document(&doc, &hl, &theme)
    }

    fn line_to_plain(line: &StyledLine) -> String {
        line.iter().map(|s| s.text.as_str()).collect::<String>()
    }

    /// display width ベースで文字列を指定幅にパディング or 切り詰める（テスト専用）。
    fn pad_or_truncate(s: &str, width: usize) -> String {
        let current = UnicodeWidthStr::width(s);
        if current == width {
            return s.to_string();
        }
        if current < width {
            let mut out = String::from(s);
            for _ in 0..(width - current) {
                out.push(' ');
            }
            return out;
        }
        let mut out = String::new();
        let mut acc = 0usize;
        for ch in s.chars() {
            let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if acc + w > width {
                break;
            }
            out.push(ch);
            acc += w;
        }
        for _ in 0..(width - acc) {
            out.push(' ');
        }
        out
    }

    /// アラインメントに応じて文字列を指定幅にパディング or 切り詰める。
    /// `render_cell_to_spans` の純粋文字列版（幅計算テスト専用）。
    fn pad_or_truncate_aligned(s: &str, width: usize, align: Alignment) -> String {
        let current = UnicodeWidthStr::width(s);
        if current >= width {
            return pad_or_truncate(s, width);
        }
        let pad = width - current;
        match align {
            Alignment::Right => {
                let mut out = " ".repeat(pad);
                out.push_str(s);
                out
            }
            Alignment::Center => {
                let left = pad / 2;
                let right = pad - left;
                let mut out = " ".repeat(left);
                out.push_str(s);
                for _ in 0..right {
                    out.push(' ');
                }
                out
            }
            Alignment::Left | Alignment::None => {
                let mut out = String::from(s);
                for _ in 0..pad {
                    out.push(' ');
                }
                out
            }
        }
    }

    // =========================================================================
    // pad_or_truncate_aligned 単体テスト
    // =========================================================================

    #[test]
    fn pad_or_truncate_aligned_right_pads_left() {
        assert_eq!(pad_or_truncate_aligned("a", 3, Alignment::Right), "  a");
    }

    #[test]
    fn pad_or_truncate_aligned_left_pads_right() {
        assert_eq!(pad_or_truncate_aligned("a", 3, Alignment::Left), "a  ");
    }

    #[test]
    fn pad_or_truncate_aligned_center_pads_both() {
        // 幅 4 に "a"（1幅）: left=1, right=2
        assert_eq!(pad_or_truncate_aligned("a", 4, Alignment::Center), " a  ");
    }

    #[test]
    fn pad_or_truncate_aligned_truncates_when_over_width() {
        // "abc"（3幅）を 2 に切り詰め → "ab"
        assert_eq!(pad_or_truncate_aligned("abc", 2, Alignment::Left), "ab");
    }

    #[test]
    fn block_starts_for_simple_doc() {
        let out = render("# Title\n\nbody\n\n## Sub\n");
        // blocks: Heading(H1), Paragraph(body), Heading(H2)
        assert_eq!(out.block_starts.len(), 3);
        // Heading H1 は最初の行
        assert_eq!(out.block_starts[0], 0);
        // 各 block 開始行は単調増加（途中の空行を考慮）
        assert!(out.block_starts[0] < out.block_starts[1]);
        assert!(out.block_starts[1] < out.block_starts[2]);
    }

    #[test]
    fn table_renders_ascii_borders() {
        let out = render("| a | b |\n|---|---|\n| 1 | 2 |\n");
        // ヘッダ行 + セパレータ + データ行 が含まれていること
        let has_separator = out.lines.iter().any(|l| line_to_plain(l).contains("┼"));
        assert!(
            has_separator,
            "テーブルセパレータが描画されていない: {out:?}"
        );
        let has_pipe_in_header = out.lines.iter().any(|l| {
            let p = line_to_plain(l);
            p.contains("│") && p.contains('a') && p.contains('b')
        });
        assert!(
            has_pipe_in_header,
            "テーブルヘッダの │ 区切りが描画されていない"
        );
    }

    #[test]
    fn nested_list_indentation() {
        let out = render("- a\n  - b\n");
        // 子要素 b はインデントされた行に出る
        let b_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains('b'))
            .expect("b の行が見つからない");
        let plain = line_to_plain(b_line);
        // 親項目より深いインデント（先頭が複数スペース）
        assert!(
            plain.starts_with("    "),
            "ネスト List のインデントが不足: {plain:?}"
        );
    }

    #[test]
    fn block_starts_with_table_and_paragraph() {
        let md = "before\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nafter\n";
        let out = render(md);
        // blocks: Paragraph(before), Table, Paragraph(after)
        assert_eq!(out.block_starts.len(), 3);
        let l0 = line_to_plain(&out.lines[out.block_starts[0]]);
        assert!(l0.contains("before"));
        let l2 = line_to_plain(&out.lines[out.block_starts[2]]);
        assert!(l2.contains("after"));
    }

    #[test]
    fn heading_link_uses_underline_without_url_text() {
        let out = render("# [click](https://example.com)\n");
        let line = &out.lines[out.block_starts[0]];
        let plain = line_to_plain(line);
        // テキストは "click" を含み URL は描画されない
        assert!(plain.contains("click"));
        assert!(
            !plain.contains("https://"),
            "見出し内リンクで URL が描画されている: {plain:?}"
        );
        // リンク部分の Span に UNDERLINED が付与されている
        let link_span = line
            .iter()
            .find(|s| s.text == "click")
            .expect("click span が見つからない");
        assert!(
            link_span.style.add_modifier.contains(Modifier::UNDERLINED),
            "見出し内リンクに UNDERLINED が付いていない: {:?}",
            link_span.style
        );
    }

    #[test]
    fn table_renders_with_japanese_cells() {
        let md = "| 名前 | 役割 |\n|------|------|\n| 太郎 | 開発 |\n| 花子 | 設計 |\n";
        let out = render(md);
        let has_separator = out.lines.iter().any(|l| line_to_plain(l).contains("┼"));
        assert!(
            has_separator,
            "日本語テーブルのセパレータが描画されない: {out:?}"
        );
        // ヘッダ行と全データ行が存在
        let header_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains("名前"))
            .expect("ヘッダ行が見つからない");
        let taro_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains("太郎"))
            .expect("太郎行が見つからない");
        // display width が一致（矩形が崩れない）
        let header_width =
            unicode_width::UnicodeWidthStr::width(line_to_plain(header_line).as_str());
        let taro_width = unicode_width::UnicodeWidthStr::width(line_to_plain(taro_line).as_str());
        assert_eq!(
            header_width, taro_width,
            "ヘッダと行の display width が一致しない"
        );
    }

    #[test]
    fn table_renders_with_emoji_cells() {
        let md = "| Status | Count |\n|--------|-------|\n| 👍 OK  | 10    |\n| 🔥 NG  | 3     |\n";
        let out = render(md);
        let has_separator = out.lines.iter().any(|l| line_to_plain(l).contains("┼"));
        assert!(has_separator, "絵文字テーブルのセパレータが描画されない");
        // 絵文字が含まれていることを確認
        let ok_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains("👍"))
            .expect("絵文字行が見つからない");
        let ng_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains("🔥"))
            .expect("絵文字行が見つからない");
        let ok_width = unicode_width::UnicodeWidthStr::width(line_to_plain(ok_line).as_str());
        let ng_width = unicode_width::UnicodeWidthStr::width(line_to_plain(ng_line).as_str());
        assert_eq!(
            ok_width, ng_width,
            "絵文字を含む行同士の display width が一致しない"
        );
    }

    #[test]
    fn table_column_width_adapts_to_longest_cell() {
        // 1 列目のセル最大 display width は「長い日本語テキスト」の 16
        let md = "| 名前 | 値 |\n|------|----|\n| 長い日本語テキスト | 1 |\n| a | 2 |\n";
        let out = render(md);
        // ヘッダ行と最長行の display width が一致すること
        let lines_with_pipe: Vec<String> = out
            .lines
            .iter()
            .map(line_to_plain)
            .filter(|s| s.contains("│"))
            .collect();
        assert!(!lines_with_pipe.is_empty(), "テーブル行が見つからない");
        let widths: Vec<usize> = lines_with_pipe
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.as_str()))
            .collect();
        let first = widths[0];
        for w in &widths {
            assert_eq!(
                *w, first,
                "行ごとの display width がそろっていない: {widths:?}"
            );
        }
    }

    #[test]
    fn table_truncates_overlong_cell() {
        // 50 文字の a を含むセル → 40 文字にクランプされる
        let long = "a".repeat(50);
        let md = format!("| col |\n|-----|\n| {} |\n", long);
        let out = render(&md);
        let data_line = out
            .lines
            .iter()
            .find(|l| {
                let s = line_to_plain(l);
                s.contains('a') && !s.contains('─') && !s.contains("col")
            })
            .expect("データ行が見つからない");
        let plain = line_to_plain(data_line);
        // 40 文字のキャップ内に収まっている（a が 40 個以下）
        let a_count = plain.chars().filter(|c| *c == 'a').count();
        assert!(
            a_count <= 40,
            "セルが 40 文字以内にクランプされていない: {a_count}"
        );
    }

    #[test]
    fn emoji_width_measurement() {
        // 仮説A: unicode-width が 🎉 と 🚀 に対して何を返すか確認
        let party = "🎉";
        let rocket = "🚀";

        let party_width = party.width();
        let rocket_width = rocket.width();

        println!("🎉 (party) width: {}", party_width);
        println!("🚀 (rocket) width: {}", rocket_width);

        // 両方とも 2 であること
        assert_eq!(
            party_width, 2,
            "🎉 should have width 2 according to unicode-width"
        );
        assert_eq!(
            rocket_width, 2,
            "🚀 should have width 2 according to unicode-width"
        );
    }

    #[test]
    fn table_with_party_and_rocket_emoji() {
        // 実際のテストケース: 🎉 と 🚀 を含む Table
        let md = "| Emoji | Desc |\n|-------|------|\n| 🎉 | party |\n| 🚀 | rocket |\n";
        let out = render(md);

        // テーブルが正常にレンダリングされていることを確認
        let has_separator = out.lines.iter().any(|l| line_to_plain(l).contains("┼"));
        assert!(has_separator, "テーブルセパレータが見つからない");

        // party 行と rocket 行の display width が一致していること
        let party_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains("party"))
            .expect("party 行が見つからない");
        let rocket_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains("rocket"))
            .expect("rocket 行が見つからない");

        let party_text = line_to_plain(party_line);
        let rocket_text = line_to_plain(rocket_line);

        let party_width = party_text.width();
        let rocket_width = rocket_text.width();

        println!("party line: '{}' (width: {})", party_text, party_width);
        println!("rocket line: '{}' (width: {})", rocket_text, rocket_width);

        assert_eq!(
            party_width, rocket_width,
            "party と rocket 行の display width が一致していない。party={}, rocket={}",
            party_width, rocket_width
        );
    }

    #[test]
    fn table_separator_matches_data_row_width() {
        // ヘッダ行と区切り行の display width が完全一致すること。
        // render_table で区切り線を "─┼─"（3幅）で join するため、
        // データ行の " │ "（3幅）と揃う。
        let md = "| Name | Age |\n|------|-----|\n| Alice | 30 |\n";
        let out = render(md);

        let header_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains("Name"))
            .expect("ヘッダ行が見つからない");
        let sep_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains("┼"))
            .expect("区切り行が見つからない");

        let header_text = line_to_plain(header_line);
        let sep_text = line_to_plain(sep_line);

        let header_width = unicode_width::UnicodeWidthStr::width(header_text.as_str());
        let sep_width = unicode_width::UnicodeWidthStr::width(sep_text.as_str());

        assert_eq!(
            header_width, sep_width,
            "ヘッダ行と区切り行の display width が一致しない: header={header_width}, sep={sep_width}"
        );
    }

    // =========================================================================
    // Phase F1: Table 列アラインメントのテスト
    // =========================================================================

    #[test]
    fn table_alignment_left_pads_right() {
        // Left align: 内容 "a"（1幅）、列幅 = min 3 → "a  "（右に 2 スペース）
        let out = render("| col |\n|:---|\n| a |\n");
        let data_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains('a') && !line_to_plain(l).contains('─'))
            .expect("データ行が見つからない");
        // 先頭 span（indent/prefix を除いて最初のセルテキスト）が "a" で始まり右に空白が続く
        let spans_text: String = data_line.iter().map(|s| s.text.as_str()).collect();
        // TABLE_COL_MIN_WIDTH=3 のため内容幅 1 の列は必ず幅 3 になり "a  " が保証される
        assert!(
            spans_text.contains("a  "),
            "Left align でセルの右に 2 スペースが付いていない: {:?}",
            spans_text
        );
    }

    #[test]
    fn table_alignment_right_pads_left() {
        // Right align: 内容 "a"（1幅）、列幅 = min 3 → "  a"（左に 2 スペース）
        let out = render("| col |\n|---:|\n| a |\n");
        let data_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains('a') && !line_to_plain(l).contains('─'))
            .expect("データ行が見つからない");
        // left_pad 用の StyledSpan が存在し、"  " を持つこと
        let has_left_pad = data_line
            .iter()
            .any(|s| s.text.starts_with("  ") && s.url.is_none());
        assert!(
            has_left_pad,
            "Right align でセルの左にスペースが付いていない: {:?}",
            data_line
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn table_alignment_center_pads_both() {
        // Center align: 内容 "a"（1幅）、列幅 = min 3 → 左 1 スペース + "a" + 右 1 スペース
        let out = render("| col |\n|:---:|\n| a |\n");
        let data_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains('a') && !line_to_plain(l).contains('─'))
            .expect("データ行が見つからない");
        let spans_text: String = data_line.iter().map(|s| s.text.as_str()).collect();
        // " a " の形（左右にスペース）が含まれる
        assert!(
            spans_text.contains(" a "),
            "Center align でセルの両側にスペースが付いていない: {:?}",
            spans_text
        );
    }

    #[test]
    fn table_alignment_none_defaults_to_left() {
        // None（区切りなし）は Left と同じ挙動
        let out = render("| col |\n|---|\n| a |\n");
        let data_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains('a') && !line_to_plain(l).contains('─'))
            .expect("データ行が見つからない");
        let spans_text: String = data_line.iter().map(|s| s.text.as_str()).collect();
        // TABLE_COL_MIN_WIDTH=3 のため内容幅 1 の列は必ず幅 3 になり "a  " が保証される（Left fallback）
        assert!(
            spans_text.contains("a  "),
            "None align で Left fallback でセルの右に 2 スペースが付いていない: {:?}",
            spans_text
        );
    }

    // =========================================================================
    // Phase F2: テーブルセル内インライン書式のテスト
    // =========================================================================

    #[test]
    fn table_cell_preserves_bold() {
        // | **bold** | → セル内に BOLD modifier が付いた StyledSpan が存在する
        let out = render("| col |\n|---|\n| **bold** |\n");
        let data_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains("bold") && !line_to_plain(l).contains('─'))
            .expect("データ行が見つからない");
        let bold_span = data_line
            .iter()
            .find(|s| s.text.contains("bold") && s.style.add_modifier.contains(Modifier::BOLD));
        assert!(
            bold_span.is_some(),
            "セル内 **bold** に BOLD modifier が付いていない。spans: {:?}",
            data_line
                .iter()
                .map(|s| (s.text.as_str(), s.style.add_modifier))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn table_cell_preserves_link_url() {
        // | [text](https://example.com) | → セル内に url: Some("https://example.com") を持つ span が存在する
        let out = render("| col |\n|---|\n| [text](https://example.com) |\n");
        let data_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains("text") && !line_to_plain(l).contains('─'))
            .expect("データ行が見つからない");
        let link_span = data_line
            .iter()
            .find(|s| s.url.as_deref() == Some("https://example.com"));
        assert!(
            link_span.is_some(),
            "セル内リンクに url フィールドが設定されていない。spans: {:?}",
            data_line
                .iter()
                .map(|s| (s.text.as_str(), s.url.as_deref()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn table_cell_truncates_with_ellipsis() {
        // 50 文字を超えるセル → 末尾が `…` になる
        let long = "a".repeat(50);
        let out = render(&format!("| col |\n|---|\n| {} |\n", long));
        let data_line = out
            .lines
            .iter()
            .find(|l| line_to_plain(l).contains('a') && !line_to_plain(l).contains('─'))
            .expect("データ行が見つからない");
        let spans_text: String = data_line.iter().map(|s| s.text.as_str()).collect();
        assert!(
            spans_text.contains('…'),
            "長いセルに省略記号 `…` が付いていない: {:?}",
            spans_text
        );
        // 全体は MAX_WIDTH (40) 以内に収まっている
        // (セルコンテンツ部分の幅はヘッダ "col" の 3 文字に合わせて clamp される)
        let a_count = spans_text.chars().filter(|c| *c == 'a').count();
        assert!(
            a_count <= 40,
            "セルが 40 文字以内にクランプされていない: a_count={}",
            a_count
        );
    }
}
