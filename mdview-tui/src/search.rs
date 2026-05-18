//! テキスト検索コアモジュール。
//!
//! smartcase・正規表現マッチ・`StyledLine` 上でのバイトオフセット管理を提供する。
//! 検索ロジックは TUI 層に閉じており、`mdview-core` は変更しない。

use crate::types::StyledLine;

/// 1 つの検索マッチを表す。
///
/// `line` は `App::lines` のインデックス（wrap 前の行番号）。
/// `byte_start` / `byte_end` は行プレーンテキスト上のバイトオフセット（`[byte_start, byte_end)` 半開区間）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    /// `App::lines` における行インデックス（wrap 前）。
    pub line: usize,
    /// 行プレーンテキスト上のバイト開始オフセット（inclusive）。
    pub byte_start: usize,
    /// 行プレーンテキスト上のバイト終了オフセット（exclusive）。
    pub byte_end: usize,
}

/// smartcase 判定: クエリに大文字が含まれる場合は case-sensitive。
///
/// vim の `smartcase` と同じ仕様。
pub fn is_case_sensitive(query: &str) -> bool {
    query.chars().any(|c| c.is_uppercase())
}

/// クエリから正規表現を構築する。
///
/// smartcase に基づいて `case_insensitive` フラグを設定する。
/// 不正な正規表現の場合は `Err` を返す（パニックしない）。
pub fn build_regex(query: &str) -> Result<regex::Regex, regex::Error> {
    regex::RegexBuilder::new(query)
        .case_insensitive(!is_case_sensitive(query))
        .build()
}

/// `StyledLine` 1 行をプレーンテキストに変換する（style 情報を除去）。
fn line_to_plain(line: &StyledLine) -> String {
    line.iter().map(|s| s.text.as_str()).collect::<String>()
}

/// 全行からクエリに一致するマッチを収集して返す。
///
/// `query` が空文字列の場合は空 `Vec` を返す（正規表現を構築しない）。
/// 不正な正規表現の場合は `Err` を返す。
pub fn find_matches(lines: &[StyledLine], query: &str) -> Result<Vec<SearchMatch>, regex::Error> {
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let re = build_regex(query)?;
    let mut matches = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        let plain = line_to_plain(line);
        for m in re.find_iter(&plain) {
            matches.push(SearchMatch {
                line: line_idx,
                byte_start: m.start(),
                byte_end: m.end(),
            });
        }
    }

    Ok(matches)
}

// ===========================================================================
// テスト
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::StyledSpan;
    use ratatui::style::Style;

    fn make_span(text: &str) -> StyledSpan {
        StyledSpan {
            text: text.to_string(),
            style: Style::default(),
            url: None,
        }
    }

    fn make_line(spans: &[&str]) -> StyledLine {
        spans.iter().map(|s| make_span(s)).collect()
    }

    // ── is_case_sensitive ──────────────────────────────────────────────────

    #[test]
    fn is_case_sensitive_lowercase_only_returns_false() {
        assert!(!is_case_sensitive("hello"));
        assert!(!is_case_sensitive("hello world"));
        assert!(!is_case_sensitive("123 abc"));
    }

    #[test]
    fn is_case_sensitive_with_uppercase_returns_true() {
        assert!(is_case_sensitive("Hello"));
        assert!(is_case_sensitive("hEllo"));
        assert!(is_case_sensitive("HELLO"));
    }

    #[test]
    fn is_case_sensitive_empty_returns_false() {
        assert!(!is_case_sensitive(""));
    }

    // ── build_regex ────────────────────────────────────────────────────────

    #[test]
    fn build_regex_valid_pattern_succeeds() {
        let re = build_regex("hello");
        assert!(re.is_ok());
    }

    #[test]
    fn build_regex_invalid_pattern_returns_err() {
        // 不正な正規表現: 閉じていないブラケット
        let re = build_regex("[");
        assert!(re.is_err(), "invalid regex should return Err, not panic");
    }

    #[test]
    fn build_regex_invalid_pattern_unclosed_paren() {
        let re = build_regex("(");
        assert!(re.is_err());
    }

    #[test]
    fn build_regex_case_insensitive_for_lowercase_query() {
        // 小文字クエリ → case-insensitive → 大文字にもマッチ
        let re = build_regex("hello").unwrap();
        assert!(re.is_match("HELLO"));
        assert!(re.is_match("Hello"));
    }

    #[test]
    fn build_regex_case_sensitive_for_uppercase_query() {
        // 大文字クエリ → case-sensitive → 大文字のみマッチ
        let re = build_regex("Hello").unwrap();
        assert!(re.is_match("Hello"));
        assert!(!re.is_match("hello"));
        assert!(!re.is_match("HELLO"));
    }

    // ── find_matches ───────────────────────────────────────────────────────

    #[test]
    fn find_matches_empty_query_returns_empty() {
        let lines = vec![make_line(&["hello world"])];
        let result = find_matches(&lines, "").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn find_matches_single_line_single_match() {
        let lines = vec![make_line(&["hello world"])];
        let result = find_matches(&lines, "world").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 0);
        assert_eq!(result[0].byte_start, 6);
        assert_eq!(result[0].byte_end, 11);
    }

    #[test]
    fn find_matches_multiple_lines_multiple_matches() {
        let lines = vec![
            make_line(&["foo bar baz"]),
            make_line(&["qux foo quux"]),
            make_line(&["no match here"]),
            make_line(&["foo and foo again"]),
        ];
        let result = find_matches(&lines, "foo").unwrap();
        assert_eq!(result.len(), 4);
        // line 0: 1 match
        assert_eq!(result[0].line, 0);
        assert_eq!(result[0].byte_start, 0);
        // line 1: 1 match
        assert_eq!(result[1].line, 1);
        // line 3: 2 matches
        assert_eq!(result[2].line, 3);
        assert_eq!(result[3].line, 3);
    }

    #[test]
    fn find_matches_smartcase_lowercase_query_matches_uppercase() {
        let lines = vec![
            make_line(&["This is RUST"]),
            make_line(&["rust is great"]),
            make_line(&["Rust programming"]),
        ];
        // 小文字クエリ → case-insensitive → 全行にマッチ
        let result = find_matches(&lines, "rust").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].line, 0);
        assert_eq!(result[1].line, 1);
        assert_eq!(result[2].line, 2);
    }

    #[test]
    fn find_matches_smartcase_uppercase_query_case_sensitive() {
        let lines = vec![
            make_line(&["This is RUST"]),
            make_line(&["rust is great"]),
            make_line(&["Rust programming"]),
        ];
        // 大文字クエリ → case-sensitive → "Rust" のみ（大文字 R）
        let result = find_matches(&lines, "Rust").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].line, 2);
    }

    #[test]
    fn find_matches_match_spanning_multiple_spans() {
        // マッチが複数 span にまたがるケース
        // "hello" が "hel" span と "lo" span に分かれている
        let line = vec![make_span("hel"), make_span("lo world")];
        let lines = vec![line];
        // プレーンテキストは "hello world" → "hello" のマッチは byte 0..5
        let result = find_matches(&lines, "hello").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].byte_start, 0);
        assert_eq!(result[0].byte_end, 5);
    }

    #[test]
    fn find_matches_invalid_regex_returns_err() {
        let lines = vec![make_line(&["hello"])];
        let result = find_matches(&lines, "[");
        assert!(result.is_err());
    }

    #[test]
    fn find_matches_utf8_multibyte_characters() {
        // 日本語テキストでのマッチ
        let lines = vec![make_line(&["こんにちは世界"]), make_line(&["hello 世界"])];
        let result = find_matches(&lines, "世界").unwrap();
        assert_eq!(result.len(), 2);
        // "こんにちは" は UTF-8 で 5 × 3 = 15 bytes
        assert_eq!(result[0].byte_start, 15);
        // "hello " は 6 bytes
        assert_eq!(result[1].byte_start, 6);
    }

    #[test]
    fn find_matches_no_match_returns_empty() {
        let lines = vec![
            make_line(&["hello world"]),
            make_line(&["rust programming"]),
        ];
        let result = find_matches(&lines, "python").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn find_matches_regex_pattern() {
        let lines = vec![
            make_line(&["foo123"]),
            make_line(&["bar456"]),
            make_line(&["baz"]),
        ];
        // 正規表現: 数字にマッチ
        let result = find_matches(&lines, r"\d+").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].line, 0);
        assert_eq!(result[1].line, 1);
    }
}
