//! メモパネルの描画。
//!
//! 閲覧モード（Normal）と編集モード（Insert）を切り替えて表示する。
//! - 閲覧モード: 現在のメモを `Paragraph` で表示
//! - 編集モード: `TextArea` ウィジェットを表示（ボーダー色を変化させて編集中を示す）

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;
use ratatui_textarea::TextArea;

use mdview_core::AnchorKey;

use crate::theme::TuiTheme;

/// メモパネルを描画する。
///
/// # 引数
/// - `frame`: ratatui フレーム
/// - `area`: 描画領域
/// - `current_anchor`: 現在フォーカスしている見出しの `AnchorKey`。`None` のときは案内文を表示
/// - `current_note`: 現在の見出しに対応するメモ本文。`None` または空文字列なら「no note」表示
/// - `edit_mode`: 編集モード（Insert）かどうか
/// - `textarea`: `ratatui-textarea` の `TextArea` ウィジェット
/// - `theme`: TUI テーマ
pub fn render(
    frame: &mut Frame,
    area: Rect,
    current_anchor: Option<&AnchorKey>,
    current_note: Option<&str>,
    edit_mode: bool,
    textarea: &TextArea<'_>,
    theme: &TuiTheme,
) {
    // タイトル文字列
    let title = if let Some(anchor) = current_anchor {
        format!(" \u{1f4dd} {} ", anchor.heading_text)
    } else {
        " \u{1f4dd} (scroll to a heading) ".to_string()
    };

    // ボーダースタイル（編集モード時は toc_highlight_bg 色で強調）
    let border_style = if edit_mode {
        Style::default()
            .fg(theme.toc_highlight_bg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let outer_block = Block::bordered()
        .title(Line::from(title))
        .border_style(border_style);

    let inner_area = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    if edit_mode {
        // 編集モード: TextArea を内側の領域に描画
        // TextArea 自体が Border を持つと二重ボーダーになるため、外枠は上記で描画済み
        frame.render_widget(textarea, inner_area);
    } else {
        // 閲覧モード
        let note_text = current_note.unwrap_or("");
        if note_text.is_empty() {
            // メモなし: 薄いヒント文字を表示
            let hint = Paragraph::new(Line::from(vec![Span::styled(
                "(no note. press 'i' to edit)",
                Style::default().fg(theme.quote_prefix),
            )]))
            .wrap(Wrap { trim: false });
            frame.render_widget(hint, inner_area);
        } else {
            // メモあり: テキストを折り返して表示
            let para = Paragraph::new(note_text).wrap(Wrap { trim: false });
            frame.render_widget(para, inner_area);
        }
    }
}
