//! TUI テーマ定義。
//!
//! `TuiTheme` は UI 全体の色定義を保持する。各テーマのコンストラクタが
//! ratatui `Color` を使って具体的な色値を返す。
//!
//! syntect テーマ名は `ThemeSet::load_defaults()` の BTreeMap キーと完全一致させること
//! （実測値: InspiredGitHub / Solarized (dark) / Solarized (light) /
//!  base16-eighties.dark / base16-mocha.dark / base16-ocean.dark / base16-ocean.light）。

use ratatui::style::Color;

/// TUI 全体の配色定義。
#[derive(Debug, Clone)]
pub struct TuiTheme {
    pub id: &'static str,
    /// syntect `ThemeSet::load_defaults()` の BTreeMap キー名（実測値）。
    pub syntect_theme: &'static str,

    // ── 見出し ──────────────────────────────────────
    pub heading1: Color,
    pub heading2: Color,
    pub heading3_plus: Color,

    // ── インライン ──────────────────────────────────
    pub code_inline: Color,
    pub link: Color,

    // ── ブロック装飾 ────────────────────────────────
    pub list_bullet: Color,
    pub code_badge_fg: Color,
    pub code_badge_bg: Color,
    pub table_border: Color,
    pub rule: Color,
    pub quote_prefix: Color,

    // ── UI ──────────────────────────────────────────
    pub statusbar_fg: Color,
    pub statusbar_bg: Color,
    pub statusbar_error_bg: Color,
    pub toc_highlight_fg: Color,
    pub toc_highlight_bg: Color,
}

impl TuiTheme {
    /// VS Code Dark テーマ（default）。
    /// syntect: base16-ocean.dark（現行維持）。
    pub fn vscode_dark() -> Self {
        Self {
            id: "vscode-dark",
            syntect_theme: "base16-ocean.dark",

            heading1: Color::Rgb(86, 156, 214), // #569cd6 VS Code blue
            heading2: Color::Rgb(78, 201, 176), // #4ec9b0 VS Code teal
            heading3_plus: Color::Rgb(197, 134, 192), // #c586c0 VS Code purple

            code_inline: Color::Rgb(206, 145, 120), // #ce9178 VS Code string orange
            link: Color::Rgb(86, 156, 214),         // #569cd6 VS Code blue

            list_bullet: Color::Rgb(86, 156, 214),   // #569cd6
            code_badge_fg: Color::Rgb(30, 30, 30),   // #1e1e1e dark bg
            code_badge_bg: Color::Rgb(78, 201, 176), // #4ec9b0 teal

            table_border: Color::DarkGray,
            rule: Color::DarkGray,
            quote_prefix: Color::DarkGray,

            statusbar_fg: Color::White,
            statusbar_bg: Color::Rgb(0, 120, 212), // #0078d4 VS Code activityBar blue
            statusbar_error_bg: Color::Red,

            toc_highlight_fg: Color::Black,
            toc_highlight_bg: Color::Rgb(86, 156, 214), // #569cd6
        }
    }

    /// VS Code Light テーマ。
    /// syntect: base16-ocean.light。
    pub fn vscode_light() -> Self {
        Self {
            id: "vscode-light",
            syntect_theme: "base16-ocean.light",

            heading1: Color::Rgb(0, 0, 128),        // #000080 navy
            heading2: Color::Rgb(0, 112, 192),      // #0070c0 blue
            heading3_plus: Color::Rgb(153, 0, 153), // #990099 purple

            code_inline: Color::Rgb(163, 21, 21), // #a31515 VS Code red (string)
            link: Color::Rgb(0, 0, 255),          // #0000ff blue

            list_bullet: Color::Rgb(0, 112, 192), // #0070c0
            code_badge_fg: Color::White,
            code_badge_bg: Color::Rgb(0, 112, 192), // #0070c0

            table_border: Color::Gray,
            rule: Color::Gray,
            quote_prefix: Color::Gray,

            statusbar_fg: Color::White,
            statusbar_bg: Color::Rgb(0, 120, 212), // #0078d4 VS Code blue
            statusbar_error_bg: Color::Red,

            toc_highlight_fg: Color::White,
            toc_highlight_bg: Color::Rgb(0, 112, 192), // #0070c0
        }
    }

    /// GitHub Dark テーマ。
    /// syntect: base16-eighties.dark（近似）。
    pub fn github_dark() -> Self {
        Self {
            id: "github-dark",
            syntect_theme: "base16-eighties.dark",

            heading1: Color::Rgb(88, 166, 255), // #58a6ff GitHub dark heading blue
            heading2: Color::Rgb(121, 192, 255), // #79c0ff
            heading3_plus: Color::Rgb(210, 168, 255), // #d2a8ff purple

            code_inline: Color::Rgb(255, 123, 114), // #ff7b72 GitHub dark red
            link: Color::Rgb(88, 166, 255),         // #58a6ff

            list_bullet: Color::Rgb(88, 166, 255),   // #58a6ff
            code_badge_fg: Color::Rgb(13, 17, 23),   // #0d1117 GitHub dark bg
            code_badge_bg: Color::Rgb(88, 166, 255), // #58a6ff

            table_border: Color::DarkGray,
            rule: Color::DarkGray,
            quote_prefix: Color::DarkGray,

            statusbar_fg: Color::White,
            statusbar_bg: Color::Rgb(33, 38, 45), // #21262d GitHub dark header
            statusbar_error_bg: Color::Red,

            toc_highlight_fg: Color::Black,
            toc_highlight_bg: Color::Rgb(88, 166, 255), // #58a6ff
        }
    }

    /// GitHub Light テーマ。
    /// syntect: InspiredGitHub。
    pub fn github_light() -> Self {
        Self {
            id: "github-light",
            syntect_theme: "InspiredGitHub",

            heading1: Color::Rgb(3, 47, 98), // #032f62 GitHub dark blue
            heading2: Color::Rgb(0, 92, 197), // #005cc5
            heading3_plus: Color::Rgb(111, 66, 193), // #6f42c1 purple

            code_inline: Color::Rgb(215, 58, 73), // #d73a49 GitHub red
            link: Color::Rgb(0, 92, 197),         // #005cc5

            list_bullet: Color::Rgb(0, 92, 197), // #005cc5
            code_badge_fg: Color::White,
            code_badge_bg: Color::Rgb(0, 92, 197), // #005cc5

            table_border: Color::Gray,
            rule: Color::Gray,
            quote_prefix: Color::Gray,

            statusbar_fg: Color::White,
            statusbar_bg: Color::Rgb(36, 41, 46), // #24292e GitHub dark header
            statusbar_error_bg: Color::Red,

            toc_highlight_fg: Color::White,
            toc_highlight_bg: Color::Rgb(0, 92, 197), // #005cc5
        }
    }

    /// Solarized Dark テーマ。
    /// syntect: "Solarized (dark)"（syntect 5.3.0 default-themes に同梱、空白・括弧含む）。
    /// 公式パレット: https://ethanschoonover.com/solarized/
    pub fn solarized_dark() -> Self {
        Self {
            id: "solarized-dark",
            syntect_theme: "Solarized (dark)",

            // 公式 Solarized パレットからの選定
            heading1: Color::Rgb(38, 139, 210), // #268bd2 blue
            heading2: Color::Rgb(42, 161, 152), // #2aa198 cyan
            heading3_plus: Color::Rgb(108, 113, 196), // #6c71c4 violet

            code_inline: Color::Rgb(203, 75, 22), // #cb4b16 orange
            link: Color::Rgb(38, 139, 210),       // #268bd2 blue

            list_bullet: Color::Rgb(133, 153, 0), // #859900 green
            code_badge_fg: Color::Rgb(0, 43, 54), // #002b36 base03 (dark bg)
            code_badge_bg: Color::Rgb(42, 161, 152), // #2aa198 cyan

            table_border: Color::Rgb(88, 110, 117), // #586e75 base01
            rule: Color::Rgb(88, 110, 117),         // #586e75 base01
            quote_prefix: Color::Rgb(88, 110, 117), // #586e75 base01

            statusbar_fg: Color::Rgb(253, 246, 227), // #fdf6e3 base3
            statusbar_bg: Color::Rgb(7, 54, 66),     // #073642 base02
            statusbar_error_bg: Color::Rgb(220, 50, 47), // #dc322f red

            toc_highlight_fg: Color::Rgb(0, 43, 54), // #002b36 base03
            toc_highlight_bg: Color::Rgb(181, 137, 0), // #b58900 yellow
        }
    }

    /// Solarized Light テーマ。
    /// syntect: "Solarized (light)"（syntect 5.3.0 default-themes に同梱）。
    pub fn solarized_light() -> Self {
        Self {
            id: "solarized-light",
            syntect_theme: "Solarized (light)",

            // 公式 Solarized パレットからの選定
            heading1: Color::Rgb(38, 139, 210), // #268bd2 blue
            heading2: Color::Rgb(42, 161, 152), // #2aa198 cyan
            heading3_plus: Color::Rgb(108, 113, 196), // #6c71c4 violet

            code_inline: Color::Rgb(203, 75, 22), // #cb4b16 orange
            link: Color::Rgb(38, 139, 210),       // #268bd2 blue

            list_bullet: Color::Rgb(133, 153, 0), // #859900 green
            code_badge_fg: Color::Rgb(253, 246, 227), // #fdf6e3 base3
            code_badge_bg: Color::Rgb(88, 110, 117), // #586e75 base01 (5.11:1 vs #fdf6e3 — WCAG AA)

            table_border: Color::Rgb(147, 161, 161), // #93a1a1 base1
            rule: Color::Rgb(147, 161, 161),         // #93a1a1 base1
            quote_prefix: Color::Rgb(147, 161, 161), // #93a1a1 base1

            statusbar_fg: Color::Rgb(253, 246, 227), // #fdf6e3 base3
            statusbar_bg: Color::Rgb(88, 110, 117),  // #586e75 base01 (5.11:1 vs #fdf6e3 — WCAG AA)
            statusbar_error_bg: Color::Rgb(220, 50, 47), // #dc322f red

            toc_highlight_fg: Color::Rgb(253, 246, 227), // #fdf6e3 base3
            toc_highlight_bg: Color::Rgb(88, 110, 117), // #586e75 base01 (5.11:1 vs #fdf6e3 — WCAG AA)
        }
    }

    /// テーマ ID 文字列から `TuiTheme` を返す。未知 ID は default（`vscode-dark`）。
    pub fn from_id(id: &str) -> Self {
        match id {
            "vscode-light" => Self::vscode_light(),
            "vscode-dark" => Self::vscode_dark(),
            "github-light" => Self::github_light(),
            "github-dark" => Self::github_dark(),
            "solarized-light" => Self::solarized_light(),
            "solarized-dark" => Self::solarized_dark(),
            _ => {
                eprintln!(
                    "mdview: unknown theme id {:?}, falling back to default (vscode-dark)",
                    id
                );
                Self::default()
            }
        }
    }
}

impl Default for TuiTheme {
    fn default() -> Self {
        Self::vscode_dark()
    }
}

// ===========================================================================
// テスト
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use syntect::highlighting::ThemeSet;

    #[test]
    fn from_id_vscode_dark_returns_correct_id() {
        let theme = TuiTheme::from_id("vscode-dark");
        assert_eq!(theme.id, "vscode-dark");
        assert_eq!(theme.syntect_theme, "base16-ocean.dark");
    }

    #[test]
    fn from_id_vscode_light_returns_correct_id() {
        let theme = TuiTheme::from_id("vscode-light");
        assert_eq!(theme.id, "vscode-light");
        assert_eq!(theme.syntect_theme, "base16-ocean.light");
    }

    #[test]
    fn from_id_github_dark_returns_correct_id() {
        let theme = TuiTheme::from_id("github-dark");
        assert_eq!(theme.id, "github-dark");
        assert_eq!(theme.syntect_theme, "base16-eighties.dark");
    }

    #[test]
    fn from_id_github_light_returns_correct_id() {
        let theme = TuiTheme::from_id("github-light");
        assert_eq!(theme.id, "github-light");
        assert_eq!(theme.syntect_theme, "InspiredGitHub");
    }

    #[test]
    fn from_id_unknown_falls_back_to_default() {
        let unknown = TuiTheme::from_id("nonexistent-theme");
        let default = TuiTheme::default();
        assert_eq!(unknown.id, default.id);
    }

    #[test]
    fn all_syntect_themes_exist_in_theme_set() {
        let ts = ThemeSet::load_defaults();
        let themes = [
            TuiTheme::vscode_dark(),
            TuiTheme::vscode_light(),
            TuiTheme::github_dark(),
            TuiTheme::github_light(),
            TuiTheme::solarized_dark(),
            TuiTheme::solarized_light(),
        ];
        for t in &themes {
            assert!(
                ts.themes.contains_key(t.syntect_theme),
                "syntect theme {:?} not found in ThemeSet::load_defaults(). id={}",
                t.syntect_theme,
                t.id
            );
        }
    }

    #[test]
    fn from_id_solarized_dark_returns_correct_id() {
        let theme = TuiTheme::from_id("solarized-dark");
        assert_eq!(theme.id, "solarized-dark");
        assert_eq!(theme.syntect_theme, "Solarized (dark)");
    }

    #[test]
    fn from_id_solarized_light_returns_correct_id() {
        let theme = TuiTheme::from_id("solarized-light");
        assert_eq!(theme.id, "solarized-light");
        assert_eq!(theme.syntect_theme, "Solarized (light)");
    }
}
