use ratatui::style::{Color, Modifier, Style};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::types::StyledLine;
use crate::types::StyledSpan;

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme: syntect::highlighting::Theme,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set.themes["base16-ocean.dark"].clone();
        Self { syntax_set, theme }
    }

    pub fn highlight_code(&self, code: &str, lang: &str) -> Vec<StyledLine> {
        let syntax = if lang.is_empty() {
            self.syntax_set.find_syntax_plain_text()
        } else {
            self.syntax_set
                .find_syntax_by_token(lang)
                .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
        };

        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut result: Vec<StyledLine> = Vec::new();

        // Ensure code ends with newline for LinesWithEndings
        let code_with_newline;
        let code_to_use = if code.ends_with('\n') {
            code
        } else {
            code_with_newline = format!("{}\n", code);
            &code_with_newline
        };

        for line in LinesWithEndings::from(code_to_use) {
            let ranges = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();

            let mut styled_line: StyledLine = Vec::new();
            for (style, text) in &ranges {
                // Strip trailing newline from last span
                let text = text.trim_end_matches('\n');
                if !text.is_empty() {
                    styled_line.push(StyledSpan {
                        text: text.to_string(),
                        style: syntect_style_to_ratatui(*style),
                    });
                }
            }
            result.push(styled_line);
        }

        // Remove trailing empty line if added by our newline
        if !code.ends_with('\n') {
            if let Some(last) = result.last() {
                if last.is_empty() {
                    result.pop();
                }
            }
        }

        result
    }
}

pub fn syntect_style_to_ratatui(s: SyntectStyle) -> Style {
    let fg = s.foreground;
    let color = Color::Rgb(fg.r, fg.g, fg.b);
    let mut style = Style::default().fg(color);

    // Background は設定しない（ターミナルデフォルト背景を壊さないため）

    use syntect::highlighting::FontStyle;
    if s.font_style.contains(FontStyle::BOLD) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if s.font_style.contains(FontStyle::ITALIC) {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if s.font_style.contains(FontStyle::UNDERLINE) {
        style = style.add_modifier(Modifier::UNDERLINED);
    }

    style
}
