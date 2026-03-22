use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mdview", about = "TUI Markdown Viewer")]
struct Cli {
    file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if !cli.file.exists() {
        eprintln!("File not found: {}", cli.file.display());
        std::process::exit(1);
    }
    let mut app = mdview_tui::app::App::new(cli.file)?;
    app.run()
}
