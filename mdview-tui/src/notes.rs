//! `~/.config/mdview/notes.json` の読み書き基盤。
//!
//! GUI（Electron）と完全互換の JSON スキーマを使用する。
//! - ファイルパスをキーとした `notes_by_file` オブジェクト。
//! - atomic write（tmp → rename）により書き込み途中のクラッシュを防ぐ。
//!
//! XDG パス解決ロジックは [`config.rs`] と同一（共通化は将来課題）。

use std::collections::BTreeMap;
use std::path::PathBuf;

use mdview_core::AnchorKey;
use serde::{Deserialize, Serialize};

pub const NOTES_SCHEMA_VERSION: u32 = 1;

/// 1 見出し分のメモエントリ。GUI スキーマと完全互換のフィールド名。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteEntry {
    pub heading_text: String,
    pub heading_level: u8,
    pub occurrence_index: u32,
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub updated_at: Option<String>,
}

/// ファイル単位のメモバケット。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesBucket {
    /// バケットの最終更新時刻（ISO-8601 文字列）。
    pub updated_at: String,
    pub entries: Vec<NoteEntry>,
}

/// `notes.json` 全体を表す構造体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesStore {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// キー = ファイル絶対パス。`BTreeMap` により決定的な順序を保証する。
    #[serde(default)]
    pub notes_by_file: BTreeMap<String, NotesBucket>,
}

fn default_schema_version() -> u32 {
    NOTES_SCHEMA_VERSION
}

impl Default for NotesStore {
    fn default() -> Self {
        Self {
            schema_version: NOTES_SCHEMA_VERSION,
            notes_by_file: BTreeMap::new(),
        }
    }
}

// ===========================================================================
// パス解決
// ===========================================================================

/// `~/.config/mdview/notes.json` を XDG 準拠で解決する。
///
/// config.rs の `config_path()` と同一ロジック（ファイル名のみ異なる）。
/// 将来的に `mdview-tui/src/paths.rs` で共通化する候補。
pub fn notes_path() -> PathBuf {
    // 1. $XDG_CONFIG_HOME
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("mdview").join("notes.json");
        }
    }
    // 2. $HOME/.config
    if let Some(home) = dirs::home_dir() {
        return home.join(".config").join("mdview").join("notes.json");
    }
    // 3. フォールバック
    PathBuf::from("~/.config/mdview/notes.json")
}

// ===========================================================================
// 読み込み
// ===========================================================================

/// デフォルトパス（`~/.config/mdview/notes.json`）から notes を読み込む。
///
/// - ファイルが存在しない → 空の `NotesStore`
/// - JSON パース失敗 → 空の `NotesStore` + stderr warn
pub fn load() -> NotesStore {
    let path = notes_path();
    load_from_path(&path)
}

/// 任意パスから読み込む（テスト用に公開）。
pub fn load_from_path(path: &PathBuf) -> NotesStore {
    if !path.exists() {
        return NotesStore::default();
    }
    match std::fs::read_to_string(path) {
        Err(e) => {
            eprintln!(
                "mdview: failed to read notes {:?}: {}. using empty store.",
                path, e
            );
            NotesStore::default()
        }
        Ok(text) => match serde_json::from_str::<NotesStore>(&text) {
            Err(e) => {
                eprintln!(
                    "mdview: failed to parse notes {:?}: {}. using empty store.",
                    path, e
                );
                NotesStore::default()
            }
            Ok(store) => {
                if store.schema_version != NOTES_SCHEMA_VERSION {
                    eprintln!(
                        "mdview: notes schema_version mismatch (got {}, expected {}). coercing.",
                        store.schema_version, NOTES_SCHEMA_VERSION
                    );
                }
                store
            }
        },
    }
}

// ===========================================================================
// 書き込み（atomic write）
// ===========================================================================

/// `~/.config/mdview/notes.json` に atomic write する。
///
/// 手順:
/// 1. 親ディレクトリを `create_dir_all`
/// 2. tmp ファイル（`.notes.json.tmp-{pid}-{rand6}`）に書き込む
/// 3. `fs::rename` で本体ファイルに差し替える
pub fn save(store: &NotesStore) -> std::io::Result<()> {
    let path = notes_path();
    save_to_path(store, &path)
}

/// 任意パスに atomic write（テスト用に公開）。
pub fn save_to_path(store: &NotesStore, path: &PathBuf) -> std::io::Result<()> {
    // 親ディレクトリを作成
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // tmp ファイル名
    let suffix = random_suffix();
    let tmp_name = format!(".notes.json.tmp-{}-{}", std::process::id(), suffix);
    let tmp_path = path
        .parent()
        .map(|p| p.join(&tmp_name))
        .unwrap_or_else(|| PathBuf::from(&tmp_name));

    // JSON シリアライズ（2 スペースインデント。GUI 側 JSON.stringify(_, null, 2) と揃える）
    let json = serde_json::to_string_pretty(store).map_err(std::io::Error::other)?;

    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, path)?;

    Ok(())
}

/// `SystemTime::now()` を使って 6 文字のランダムサフィックスを生成する。
/// `rand` クレート不要で外部依存を増やさない実装。
fn random_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    // base36 風に変換（0〜35 の文字）
    let chars: Vec<char> = "0123456789abcdefghijklmnopqrstuvwxyz".chars().collect();
    let mut n = nanos as u64;
    let mut s = String::with_capacity(6);
    for _ in 0..6 {
        s.push(chars[(n % 36) as usize]);
        n /= 36;
    }
    s
}

// ===========================================================================
// ストア操作ヘルパー
// ===========================================================================

/// 指定ファイルのメモエントリ配列を返す。バケット未存在なら空スライス。
pub fn get_entries_for<'a>(store: &'a NotesStore, file_abs_path: &str) -> &'a [NoteEntry] {
    store
        .notes_by_file
        .get(file_abs_path)
        .map(|b| b.entries.as_slice())
        .unwrap_or(&[])
}

/// 指定ファイルのメモエントリ配列を設定する。
///
/// `entries` が空なら該当バケットを削除（バケット肥大化防止）。
/// そうでなければ新しい `NotesBucket` で上書き。
pub fn set_entries_for(
    store: &mut NotesStore,
    file_abs_path: &str,
    entries: Vec<NoteEntry>,
    now_iso: &str,
) {
    if entries.is_empty() {
        store.notes_by_file.remove(file_abs_path);
    } else {
        store.notes_by_file.insert(
            file_abs_path.to_string(),
            NotesBucket {
                updated_at: now_iso.to_string(),
                entries,
            },
        );
    }
}

/// 指定 anchor にマッチするエントリを不変参照で返す。
/// 3 フィールド完全一致。
pub fn find_entry<'a>(entries: &'a [NoteEntry], anchor: &AnchorKey) -> Option<&'a NoteEntry> {
    entries.iter().find(|e| {
        e.heading_text == anchor.heading_text
            && e.heading_level == anchor.heading_level
            && e.occurrence_index == anchor.occurrence_index
    })
}

/// 指定 anchor にマッチするエントリを可変参照で返す。
/// 3 フィールド完全一致。
pub fn find_entry_mut<'a>(
    entries: &'a mut [NoteEntry],
    anchor: &AnchorKey,
) -> Option<&'a mut NoteEntry> {
    entries.iter_mut().find(|e| {
        e.heading_text == anchor.heading_text
            && e.heading_level == anchor.heading_level
            && e.occurrence_index == anchor.occurrence_index
    })
}

/// `SystemTime` から ISO-8601 形式の文字列を生成する（秒精度）。
///
/// `chrono` / `time` クレートを使わない自前実装。
/// GUI 側は ms 精度だが、validation はフォーマットを検査しないため秒精度で互換。
pub fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Unix timestamp → (year, month, day, hour, min, sec) に変換
    let (y, mo, d, h, mi, s) = unix_secs_to_datetime(secs);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, mi, s)
}

/// Unix timestamp（秒）をグレゴリオ暦 (year, month, day, hour, min, sec) に変換する。
/// UTC のみ対応（タイムゾーンオフセットなし）。
fn unix_secs_to_datetime(secs: u64) -> (u32, u8, u8, u8, u8, u8) {
    let s = secs % 60;
    let total_minutes = secs / 60;
    let mi = total_minutes % 60;
    let total_hours = total_minutes / 60;
    let h = total_hours % 24;
    let mut days = total_hours / 24; // 1970-01-01 からの日数

    // グレゴリオ暦への変換（ユリウス通算日ベース）
    // days は 1970-01-01 = day 0 から
    let mut year = 1970u32;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let leap = is_leap_year(year);
    let month_days: [u64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u8;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }

    (year, month, (days + 1) as u8, h as u8, mi as u8, s as u8)
}

fn is_leap_year(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

// ===========================================================================
// テスト
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_temp_notes_path() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("notes.json");
        (dir, path)
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let path = PathBuf::from("/tmp/mdview_test_nonexistent_notes_99999.json");
        let store = load_from_path(&path);
        assert_eq!(store.schema_version, NOTES_SCHEMA_VERSION);
        assert!(store.notes_by_file.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let (_dir, path) = make_temp_notes_path();
        let mut store = NotesStore::default();
        set_entries_for(
            &mut store,
            "/tmp/test.md",
            vec![NoteEntry {
                heading_text: "Section 1".to_string(),
                heading_level: 2,
                occurrence_index: 0,
                note: "my note".to_string(),
                created_at: Some("2026-01-01T00:00:00Z".to_string()),
                updated_at: Some("2026-01-02T00:00:00Z".to_string()),
            }],
            "2026-01-02T00:00:00Z",
        );

        save_to_path(&store, &path).unwrap();
        let loaded = load_from_path(&path);

        assert_eq!(loaded.schema_version, NOTES_SCHEMA_VERSION);
        assert!(loaded.notes_by_file.contains_key("/tmp/test.md"));
        let entries = get_entries_for(&loaded, "/tmp/test.md");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].note, "my note");
        assert_eq!(entries[0].heading_text, "Section 1");
    }

    #[test]
    fn save_uses_atomic_rename() {
        let (_dir, path) = make_temp_notes_path();
        let store = NotesStore::default();
        save_to_path(&store, &path).unwrap();

        // tmp ファイルが残っていないこと
        let parent = path.parent().unwrap();
        let tmp_files: Vec<_> = std::fs::read_dir(parent)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".notes.json.tmp-")
            })
            .collect();
        assert!(
            tmp_files.is_empty(),
            "tmp ファイルが残っています: {:?}",
            tmp_files
        );

        // 本体ファイルは存在する
        assert!(path.exists());
    }

    #[test]
    fn set_entries_empty_deletes_bucket() {
        let mut store = NotesStore::default();
        set_entries_for(
            &mut store,
            "/foo.md",
            vec![NoteEntry {
                heading_text: "H".to_string(),
                heading_level: 1,
                occurrence_index: 0,
                note: "x".to_string(),
                created_at: None,
                updated_at: None,
            }],
            "now",
        );
        assert!(store.notes_by_file.contains_key("/foo.md"));

        // 空エントリで set → バケット削除
        set_entries_for(&mut store, "/foo.md", vec![], "now");
        assert!(!store.notes_by_file.contains_key("/foo.md"));
    }

    #[test]
    fn load_gui_compatible_json() {
        let (_dir, path) = make_temp_notes_path();

        // GUI が書く形式の JSON
        let json = r#"{
  "schema_version": 1,
  "notes_by_file": {
    "/path/to/file.md": {
      "updated_at": "2026-05-17T10:00:00Z",
      "entries": [
        {
          "heading_text": "Introduction",
          "heading_level": 2,
          "occurrence_index": 0,
          "note": "GUI で書いたメモ",
          "created_at": "2026-05-17T10:00:00Z",
          "updated_at": "2026-05-17T10:00:00Z"
        }
      ]
    }
  }
}"#;
        std::fs::write(&path, json).unwrap();

        let store = load_from_path(&path);
        let entries = get_entries_for(&store, "/path/to/file.md");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].note, "GUI で書いたメモ");
        assert_eq!(entries[0].heading_text, "Introduction");
        assert_eq!(entries[0].heading_level, 2);
        assert_eq!(entries[0].occurrence_index, 0);
    }

    #[test]
    fn now_iso_format() {
        let s = now_iso();
        // "YYYY-MM-DDTHH:MM:SSZ" の形式チェック
        assert_eq!(s.len(), 20, "ISO フォーマットの長さが違う: {}", s);
        assert!(s.ends_with('Z'), "Z 終端でない: {}", s);
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
        assert_eq!(&s[10..11], "T");
        assert_eq!(&s[13..14], ":");
        assert_eq!(&s[16..17], ":");
    }

    #[test]
    fn find_entry_found() {
        let anchor = AnchorKey {
            heading_text: "Test".to_string(),
            heading_level: 1,
            occurrence_index: 0,
        };
        let entries = vec![
            NoteEntry {
                heading_text: "Other".to_string(),
                heading_level: 1,
                occurrence_index: 0,
                note: "other".to_string(),
                created_at: None,
                updated_at: None,
            },
            NoteEntry {
                heading_text: "Test".to_string(),
                heading_level: 1,
                occurrence_index: 0,
                note: "found".to_string(),
                created_at: None,
                updated_at: None,
            },
        ];
        let found = find_entry(&entries, &anchor).unwrap();
        assert_eq!(found.note, "found");
    }

    #[test]
    fn find_entry_not_found() {
        let anchor = AnchorKey {
            heading_text: "Missing".to_string(),
            heading_level: 1,
            occurrence_index: 0,
        };
        let entries: Vec<NoteEntry> = vec![];
        assert!(find_entry(&entries, &anchor).is_none());
    }

    #[test]
    fn find_entry_mut_found() {
        let anchor = AnchorKey {
            heading_text: "Test".to_string(),
            heading_level: 1,
            occurrence_index: 0,
        };
        let mut entries = vec![
            NoteEntry {
                heading_text: "Other".to_string(),
                heading_level: 1,
                occurrence_index: 0,
                note: "other".to_string(),
                created_at: None,
                updated_at: None,
            },
            NoteEntry {
                heading_text: "Test".to_string(),
                heading_level: 1,
                occurrence_index: 0,
                note: "original".to_string(),
                created_at: None,
                updated_at: None,
            },
        ];
        let found = find_entry_mut(&mut entries, &anchor).unwrap();
        found.note = "updated".to_string();
        assert_eq!(entries[1].note, "updated");
    }

    #[test]
    fn find_entry_mut_not_found() {
        let anchor = AnchorKey {
            heading_text: "Missing".to_string(),
            heading_level: 1,
            occurrence_index: 0,
        };
        let mut entries: Vec<NoteEntry> = vec![];
        assert!(find_entry_mut(&mut entries, &anchor).is_none());
    }
}
