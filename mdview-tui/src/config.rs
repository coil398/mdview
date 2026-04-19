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
    /// スキーマバージョン。現行 Phase1 は 1。
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// テーマ ID（例: "vscode-dark"）。
    #[serde(default = "default_theme")]
    pub theme: String,
    // 将来拡張用: tui / electron キー等を追加するための余地。
    // `#[serde(default)]` により JSON に存在しなくても受け取れる。
}

fn default_schema_version() -> u32 {
    1
}

fn default_theme() -> String {
    "vscode-dark".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            theme: "vscode-dark".to_string(),
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
        assert_eq!(cfg.schema_version, 1);
    }

    #[test]
    fn load_valid_config() {
        let f = write_temp_config(r#"{"schema_version":1,"theme":"github-light"}"#);
        let cfg = Config::load_from_path(&f.path().to_path_buf());
        assert_eq!(cfg.theme, "github-light");
        assert_eq!(cfg.schema_version, 1);
    }

    #[test]
    fn load_broken_json_returns_default() {
        let f = write_temp_config("{");
        let cfg = Config::load_from_path(&f.path().to_path_buf());
        assert_eq!(cfg.theme, "vscode-dark");
    }

    #[test]
    fn load_missing_theme_field_returns_default() {
        let f = write_temp_config(r#"{"schema_version":1}"#);
        let cfg = Config::load_from_path(&f.path().to_path_buf());
        assert_eq!(cfg.theme, "vscode-dark");
    }

    #[test]
    fn load_unknown_theme_id_passes_through() {
        // unknown ID はそのまま返す。TuiTheme::from_id がフォールバックを担当
        let f = write_temp_config(r#"{"schema_version":1,"theme":"unknown-theme"}"#);
        let cfg = Config::load_from_path(&f.path().to_path_buf());
        assert_eq!(cfg.theme, "unknown-theme");
    }
}
