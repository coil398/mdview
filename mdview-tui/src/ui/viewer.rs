use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::types::StyledLine;

/// ビューアを描画し、wrap 後の推定行数を返す。
///
/// NOTE: `estimate_wrapped_line_count` は ratatui の `WordWrapper` と完全一致しないため、
/// word-boundary 差で数行のズレが発生しうる。スクロール上限計算には十分な精度。
pub fn render(frame: &mut Frame, area: Rect, lines: &[StyledLine], scroll: usize) -> usize {
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

    let wrapped_line_count = estimate_wrapped_line_count(lines, area.width);
    let text = Text::from(ratatui_lines);
    let scroll_y = u16::try_from(scroll).unwrap_or(u16::MAX);
    let paragraph = Paragraph::new(text)
        .scroll((scroll_y, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
    wrapped_line_count
}

/// display width ベースで wrap 後の行数を推定する。
/// ratatui の WordWrapper とは完全一致しないが、スクロール上限計算には十分な精度。
/// - width が 0 の場合はフォールバックで lines.len() を返す
/// - 各 StyledLine について、全 Span の text を連結した display width を計算
/// - 1 行が width を超える場合は ceil(display_width / width) として折り返し行数を加算
/// - 空行（display_width == 0）も 1 行としてカウント
fn estimate_wrapped_line_count(lines: &[StyledLine], width: u16) -> usize {
    if width == 0 {
        return lines.len().max(1);
    }
    let width = width as usize;
    let mut count = 0usize;
    for line in lines {
        let total_width: usize = line
            .iter()
            .map(|span| UnicodeWidthStr::width(span.text.as_str()))
            .sum();
        if total_width == 0 {
            count += 1;
        } else {
            count += total_width.div_ceil(width);
        }
    }
    count.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::StyledSpan;
    use ratatui::style::Style;

    fn span(text: &str) -> StyledSpan {
        StyledSpan {
            text: text.to_string(),
            style: Style::default(),
        }
    }

    #[test]
    fn wrapped_line_count_ascii_no_wrap() {
        let lines: Vec<StyledLine> = vec![vec![span("hello")], vec![span("world")]];
        // width=80 → wrap なし → 2 行
        assert_eq!(estimate_wrapped_line_count(&lines, 80), 2);
    }

    #[test]
    fn wrapped_line_count_ascii_wrapped() {
        // 20 文字の行 を width=10 で wrap → ceil(20/10) = 2 行
        let lines: Vec<StyledLine> = vec![vec![span(&"a".repeat(20))]];
        assert_eq!(estimate_wrapped_line_count(&lines, 10), 2);
    }

    #[test]
    fn wrapped_line_count_japanese_wrapped() {
        // "あ" = width 2 × 10 文字 = 20 display width、width=10 で wrap → 2 行
        let lines: Vec<StyledLine> = vec![vec![span(&"あ".repeat(10))]];
        assert_eq!(estimate_wrapped_line_count(&lines, 10), 2);
    }

    #[test]
    fn wrapped_line_count_empty_line_counts_as_one() {
        let lines: Vec<StyledLine> = vec![vec![span("")], vec![span("x")]];
        assert_eq!(estimate_wrapped_line_count(&lines, 80), 2);
    }

    #[test]
    fn wrapped_line_count_zero_width_falls_back() {
        let lines: Vec<StyledLine> = vec![vec![span("a")], vec![span("b")]];
        // width=0 → lines.len() = 2
        assert_eq!(estimate_wrapped_line_count(&lines, 0), 2);
    }

    #[test]
    fn emoji_wrapping_consistency() {
        // 🎉 と 🚀 の行がどう wrap されるか確認
        let party_line = vec![StyledSpan {
            text: "🎉    │ party ".to_string(),
            style: Style::default(),
        }];
        let rocket_line = vec![StyledSpan {
            text: "🚀    │ rocket".to_string(),
            style: Style::default(),
        }];

        // ターミナル幅 30 でテスト
        let lines_party = vec![party_line];
        let lines_rocket = vec![rocket_line];

        let party_wrapped = estimate_wrapped_line_count(&lines_party, 30);
        let rocket_wrapped = estimate_wrapped_line_count(&lines_rocket, 30);

        println!(
            "party line width: {} wrapped to {} lines",
            UnicodeWidthStr::width("🎉    │ party "),
            party_wrapped
        );
        println!(
            "rocket line width: {} wrapped to {} lines",
            UnicodeWidthStr::width("🚀    │ rocket"),
            rocket_wrapped
        );

        assert_eq!(
            party_wrapped, rocket_wrapped,
            "emoji wrapping inconsistency: party={}, rocket={}",
            party_wrapped, rocket_wrapped
        );
    }
}
