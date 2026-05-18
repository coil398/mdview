//! `~/.config/mdview/config.json` 読み込み基盤。
//!
//! XDG パス解決ロジック:
//!   1. `$XDG_CONFIG_HOME/mdview/config.json`
//!   2. `$HOME/.config/mdview/config.json`（`dirs::home_dir()` 使用）
//!   3. `~/.config/mdview/config.json`（フォールバック）
//!
//! `dirs::config_dir()` は macOS で `~/Library/Application Support` を返すため
//! **使わない**（Node 側の `os.homedir()/.config` と食い違うため）。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// config.json のスキーマ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// スキーマバージョン。現行 Phase2 は 2。
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// テーマ ID（例: "vscode-dark"）。
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 見出しメモ機能の設定。
    /// v1 JSON に存在しない場合は `#[serde(default)]` により `NotesConfig::default()` が入る。
    #[serde(default)]
    pub notes: NotesConfig,
}

/// 見出しメモ機能の設定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesConfig {
    /// 起動時にメモパネルを開いた状態にするか。
    #[serde(default = "default_notes_panel_open")]
    pub panel_open: bool,
}

fn default_schema_version() -> u32 {
    2
}

fn default_theme() -> String {
    "vscode-dark".to_string()
}

fn default_notes_panel_open() -> bool {
    true
}

impl Default for NotesConfig {
    fn default() -> Self {
        Self { panel_open: true }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 2,
            theme: "vscode-dark".to_string(),
            notes: NotesConfig::default(),
        }
    }
}

impl Config {
    /// XDG パスを解決して config.json を読み込む。
    /// - ファイルが存在しない → `Config::default()`（warn なし）
    /// - JSON パース失敗 → `Config::default()` + stderr warn
    /// - 未知テーマ ID → `Config::default()` の theme にはせず、そのまま返す
    ///   （`TuiTheme::from_id` がフォールバックを担当する）
    pub fn load() -> Self {
        let path = Self::config_path();
        Self::load_from_path(&path)
    }

    /// 任意パスから読み込む（テスト用）。
    pub fn load_from_path(path: &PathBuf) -> Self {
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(path) {
            Err(e) => {
                eprintln!(
                    "mdview: failed to read config {:?}: {}. using default.",
                    path, e
                );
                Self::default()
            }
            Ok(text) => match serde_json::from_str::<Self>(&text) {
                Err(e) => {
                    eprintln!(
                        "mdview: failed to parse config {:?}: {}. using default.",
                        path, e
                    );
                    Self::default()
                }
                Ok(cfg) => cfg,
            },
        }
    }

    /// デフォルトパス（`~/.config/mdview/config.json`）に atomic write する。
    ///
    /// 手順:
    /// 1. 親ディレクトリを `create_dir_all`
    /// 2. tmp ファイル（`.config.json.tmp-{pid}-{rand6}`）に書き込む
    /// 3. `fs::rename` で本体ファイルに差し替える
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        self.save_to_path(&path)
    }

    /// 任意パスに atomic write（テスト用に公開）。
    pub fn save_to_path(&self, path: &PathBuf) -> std::io::Result<()> {
        // 親ディレクトリを取得。ルート直下等で parent が None の場合はエラー
        let parent = path.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no parent")
        })?;
        std::fs::create_dir_all(parent)?;

        // tmp ファイル名
        let suffix = crate::notes::random_suffix();
        let tmp_name = format!(".config.json.tmp-{}-{}", std::process::id(), suffix);
        let tmp_path = parent.join(&tmp_name);

        // JSON シリアライズ（2 スペースインデント。notes.rs と同じ形式）
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;

        std::fs::write(&tmp_path, json)?;
        std::fs::rename(&tmp_path, path)?;

        Ok(())
    }

    /// `~/.config/mdview/config.json` を XDG 準拠で解決する。
    pub fn config_path() -> PathBuf {
        // 1. $XDG_CONFIG_HOME
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("mdview").join("config.json");
            }
        }
        // 2. $HOME/.config
        if let Some(home) = dirs::home_dir() {
            return home.join(".config").join("mdview").join("config.json");
        }
        // 3. フォールバック（home が取れない極端なケース）
        PathBuf::from("~/.config/mdview/config.json")
    }
}

// ===========================================================================
// テスト
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_config(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{}", content).unwrap();
        f
    }

    #[test]
    fn load_from_nonexistent_path_returns_default() {
        let path = PathBuf::from("/tmp/mdview_test_nonexistent_config_12345.json");
        let cfg = Config::load_from_path(&path);
        assert_eq!(cfg.theme, "vscode-dark");
        assert_eq!(cfg.schema_version, 2);
    }

    #[test]
    fn load_valid_config() {
        let f = write_temp_config(r#"{"schema_version":2,"theme":"github-light"}"#);
        let cfg = Config::load_from_path(&f.path().to_path_buf());
        assert_eq!(cfg.theme, "github-light");
        assert_eq!(cfg.schema_version, 2);
    }

    #[test]
    fn load_broken_json_returns_default() {
        let f = write_temp_config("{");
        let cfg = Config::load_from_path(&f.path().to_path_buf());
        assert_eq!(cfg.theme, "vscode-dark");
    }

    #[test]
    fn load_missing_theme_field_returns_default() {
        let f = write_temp_config(r#"{"schema_version":2}"#);
        let cfg = Config::load_from_path(&f.path().to_path_buf());
        assert_eq!(cfg.theme, "vscode-dark");
    }

    #[test]
    fn load_unknown_theme_id_passes_through() {
        // unknown ID はそのまま返す。TuiTheme::from_id がフォールバックを担当
        let f = write_temp_config(r#"{"schema_version":2,"theme":"unknown-theme"}"#);
        let cfg = Config::load_from_path(&f.path().to_path_buf());
        assert_eq!(cfg.theme, "unknown-theme");
    }

    #[test]
    fn load_v1_json_fills_default_notes_panel_open() {
        // v1 JSON（notes フィールドなし）を読んで notes.panel_open が true になること
        let f = write_temp_config(r#"{"schema_version":1,"theme":"github-light"}"#);
        let cfg = Config::load_from_path(&f.path().to_path_buf());
        assert_eq!(cfg.theme, "github-light");
        assert!(
            cfg.notes.panel_open,
            "v1 JSON からの読み込みで notes.panel_open が true になること"
        );
    }

    #[test]
    fn load_v2_with_notes_panel_open_false() {
        // v2 JSON で panel_open: false が読めること
        let f = write_temp_config(
            r#"{"schema_version":2,"theme":"vscode-dark","notes":{"panel_open":false}}"#,
        );
        let cfg = Config::load_from_path(&f.path().to_path_buf());
        assert!(!cfg.notes.panel_open);
    }

    // =========================================================================
    // Phase F3: save / save_to_path のテスト
    // =========================================================================

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.json");

        let cfg = Config {
            schema_version: 2,
            theme: "github-dark".to_string(),
            notes: NotesConfig { panel_open: false },
        };
        cfg.save_to_path(&path).unwrap();

        let loaded = Config::load_from_path(&path);
        assert_eq!(loaded.schema_version, 2);
        assert_eq!(loaded.theme, "github-dark");
        assert!(!loaded.notes.panel_open);
    }

    #[test]
    fn save_uses_atomic_rename() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.json");

        let cfg = Config::default();
        cfg.save_to_path(&path).unwrap();

        // tmp ファイルが残っていないこと
        let tmp_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".config.json.tmp-")
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
    fn save_creates_parent_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        // 存在しないサブディレクトリに保存
        let path = dir.path().join("subdir").join("config.json");
        assert!(!path.parent().unwrap().exists());

        let cfg = Config::default();
        cfg.save_to_path(&path).unwrap();
        assert!(path.exists());
    }
}
