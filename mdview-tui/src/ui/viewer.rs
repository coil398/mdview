use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::search::SearchMatch;
use crate::theme::TuiTheme;
use crate::types::{StyledLine, StyledSpan};

/// ビューアを描画し、wrap 後の推定行数を返す。
///
/// NOTE: `estimate_wrapped_line_count` は ratatui の `WordWrapper` と完全一致しないため、
/// word-boundary 差で数行のズレが発生しうる。スクロール上限計算には十分な精度。
#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    lines: &[StyledLine],
    scroll: usize,
    search_matches: &[SearchMatch],
    search_cursor: usize,
    theme: &TuiTheme,
) -> usize {
    let ratatui_lines: Vec<Line> = lines
        .iter()
        .enumerate()
        .map(|(line_idx, styled_line)| {
            // この行に該当する SearchMatch を収集する
            let matches_on_line: Vec<(&SearchMatch, bool)> = search_matches
                .iter()
                .filter(|m| m.line == line_idx)
                .map(|m| {
                    let is_current = search_matches
                        .get(search_cursor)
                        .map(|cur| cur == m)
                        .unwrap_or(false);
                    (m, is_current)
                })
                .collect();

            if matches_on_line.is_empty() {
                // マッチなし: そのまま変換
                let spans: Vec<Span> = styled_line
                    .iter()
                    .map(|span| Span::styled(span.text.clone(), span.style))
                    .collect();
                Line::from(spans)
            } else {
                // マッチあり: span 分割してハイライトを注入
                let highlighted = apply_search_highlight(styled_line, &matches_on_line, theme);
                let spans: Vec<Span> = highlighted
                    .iter()
                    .map(|span| Span::styled(span.text.clone(), span.style))
                    .collect();
                Line::from(spans)
            }
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

/// 行プレーンテキスト上のバイトオフセット `[byte_start, byte_end)` に対応する
/// `StyledSpan` リストを分割し、マッチ箇所にハイライト Style を適用して返す。
///
/// マッチが複数 span をまたぐケース（インラインコード境界等）にも対応する。
/// 行のプレーンテキストは各 span の text を連結した座標系を使う。
fn apply_search_highlight(
    line: &StyledLine,
    matches_on_line: &[(&SearchMatch, bool)],
    theme: &TuiTheme,
) -> Vec<StyledSpan> {
    if matches_on_line.is_empty() {
        return line.clone();
    }

    // マッチ区間リストを (byte_start, byte_end, is_current) で構築・ソート
    let mut intervals: Vec<(usize, usize, bool)> = matches_on_line
        .iter()
        .map(|(m, is_current)| (m.byte_start, m.byte_end, *is_current))
        .collect();
    intervals.sort_by_key(|&(start, _, _)| start);

    let mut result: Vec<StyledSpan> = Vec::new();
    let mut span_byte_offset = 0usize; // 行プレーンテキスト上での現在 span の先頭バイト位置

    for original_span in line.iter() {
        let span_len = original_span.text.len(); // バイト長
        let span_end = span_byte_offset + span_len;

        // この span に交差するマッチ区間を探す
        // span の範囲: [span_byte_offset, span_end)
        let overlapping: Vec<(usize, usize, bool)> = intervals
            .iter()
            .filter(|&&(ms, me, _)| ms < span_end && me > span_byte_offset)
            .copied()
            .collect();

        if overlapping.is_empty() {
            // この span にマッチなし: そのまま出力
            result.push(original_span.clone());
        } else {
            // span 内でマッチが重ならないよう処理する
            // span のバイト座標系に変換（span 内のローカルオフセット）
            let mut cursor = 0usize; // span 内バイト位置

            for (ms, me, is_current) in &overlapping {
                // span 内での相対座標
                let local_start = ms.saturating_sub(span_byte_offset);
                let local_end = me.saturating_sub(span_byte_offset).min(span_len);

                // span 先頭 ～ マッチ前
                if cursor < local_start {
                    let before_text = &original_span.text[cursor..local_start];
                    if !before_text.is_empty() {
                        result.push(StyledSpan {
                            text: before_text.to_string(),
                            style: original_span.style,
                            url: original_span.url.clone(),
                        });
                    }
                }

                // マッチ部分
                if local_start < local_end {
                    let match_text = &original_span.text[local_start..local_end];
                    if !match_text.is_empty() {
                        let highlight_style = if *is_current {
                            Style::default()
                                .fg(theme.search_current_fg)
                                .bg(theme.search_current_bg)
                        } else {
                            Style::default()
                                .fg(theme.search_match_fg)
                                .bg(theme.search_match_bg)
                        };
                        result.push(StyledSpan {
                            text: match_text.to_string(),
                            style: highlight_style,
                            url: None,
                        });
                    }
                }

                cursor = local_end;
            }

            // マッチ後の残り部分
            if cursor < span_len {
                let after_text = &original_span.text[cursor..];
                if !after_text.is_empty() {
                    result.push(StyledSpan {
                        text: after_text.to_string(),
                        style: original_span.style,
                        url: original_span.url.clone(),
                    });
                }
            }
        }

        span_byte_offset = span_end;
    }

    result
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
            url: None,
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
            url: None,
        }];
        let rocket_line = vec![StyledSpan {
            text: "🚀    │ rocket".to_string(),
            style: Style::default(),
            url: None,
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
