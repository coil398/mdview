use clap::Parser;
use std::path::PathBuf;

use mdview_tui::config::Config;
use mdview_tui::theme::TuiTheme;

#[derive(Parser)]
#[command(name = "mdview", about = "TUI Markdown Viewer")]
struct Cli {
    file: PathBuf,
    /// テーマ ID を指定して起動（config.json より優先）。
    /// 有効な値: vscode-light, vscode-dark, github-light, github-dark
    #[arg(long, value_name = "THEME_ID")]
    theme: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if !cli.file.exists() {
        eprintln!("File not found: {}", cli.file.display());
        std::process::exit(1);
    }

    // 優先順位: CLI --theme > config.json theme > default (vscode-dark)
    let config = Config::load();
    let theme_id = cli.theme.as_deref().unwrap_or(&config.theme).to_string();
    let theme = TuiTheme::from_id(&theme_id);

    let mut app = mdview_tui::app::App::new(cli.file, theme)?;
    app.run()
}
