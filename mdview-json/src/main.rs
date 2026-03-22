use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mdview-json", about = "Output parsed Markdown as JSON")]
struct Cli {
    file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let text = std::fs::read_to_string(&cli.file)?;
    let doc = mdview_core::parser::parse_markdown(&text);
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}
