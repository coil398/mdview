use clap::Parser;
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mdview-json", about = "Output parsed Markdown as JSON")]
struct Cli {
    /// Markdown ファイルパス。省略時は stdin から読む
    file: Option<PathBuf>,

    /// JSON を compact 形式（1 行）で出力（デフォルトは pretty）
    #[arg(long)]
    compact: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let text = match cli.file {
        Some(path) => std::fs::read_to_string(&path)?,
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
    };

    let doc = mdview_core::parser::parse_markdown(&text);
    let json = if cli.compact {
        serde_json::to_string(&doc)?
    } else {
        serde_json::to_string_pretty(&doc)?
    };
    println!("{}", json);
    Ok(())
}
