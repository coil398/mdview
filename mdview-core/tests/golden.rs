//! ゴールデンテスト。
//!
//! 各 `tests/fixtures/<name>.md` を `parse_markdown` に通し、その結果を
//! `serde_json::to_string_pretty` でシリアライズしたものが `<name>.expected.json`
//! と一致することを確認する。
//!
//! `UPDATE_GOLDEN=1 cargo test -p mdview-core --test golden` で
//! `expected.json` を再生成する。

use std::path::PathBuf;

use mdview_core::parser::parse_markdown;
use mdview_core::Document;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn run_case(name: &str) {
    let dir = fixtures_dir();
    let md_path = dir.join(format!("{name}.md"));
    let expected_path = dir.join(format!("{name}.expected.json"));

    let md = std::fs::read_to_string(&md_path)
        .unwrap_or_else(|e| panic!("入力 Markdown が読めません {md_path:?}: {e}"));
    let doc: Document = parse_markdown(&md);
    let actual = serde_json::to_string_pretty(&doc).expect("シリアライズ失敗") + "\n";

    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(&expected_path, &actual).expect("expected.json 書き込み失敗");
        eprintln!("updated golden: {}", expected_path.display());
        return;
    }

    let expected = std::fs::read_to_string(&expected_path).unwrap_or_else(|e| {
        panic!(
            "ゴールデン {} が読めません: {e}\nUPDATE_GOLDEN=1 で生成できます",
            expected_path.display()
        )
    });

    if actual != expected {
        // 差分検出時は最初の不一致行とフルテキストを両方表示
        let mismatch_line = actual
            .lines()
            .zip(expected.lines())
            .enumerate()
            .find(|(_, (a, e))| a != e)
            .map(|(i, (a, e))| format!("行 {}: actual={a:?}\n         expected={e:?}", i + 1))
            .unwrap_or_else(|| "（行数差のみ）".to_string());
        panic!(
            "ゴールデン不一致 ({name})\n--- 不一致箇所 ---\n{mismatch_line}\n--- actual 全体 ---\n{actual}\n--- expected 全体 ---\n{expected}"
        );
    }
}

#[test]
fn complex_doc() {
    run_case("complex_doc");
}

#[test]
fn table_with_alignment() {
    run_case("table_with_alignment");
}

#[test]
fn nested_quote_list() {
    run_case("nested_quote_list");
}

#[test]
fn paragraph_hardbreak() {
    run_case("paragraph_hardbreak");
}
