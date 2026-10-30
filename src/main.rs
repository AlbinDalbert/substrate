mod config;
mod core;

use anyhow::Result;
use chrono::Local;
use clap::{Parser, Subcommand};
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Convenience verbs over a plain markdown folder")]
struct Cli {
    /// The data folder to operate on
    #[arg(long, env = "SUBSTRATE_ROOT", default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Add a new file to the inbox (text from args, or stdin if none)
    Inbox { text: Vec<String> },
    /// Print the path to today's daily file, creating it if missing
    Daily,
    /// List a folder
    List { dir: Option<String> },
    /// Print a file
    Read { path: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root.canonicalize()?;
    let cfg = config::Config::load(&root)?;

    match cli.cmd {
        Cmd::Inbox { text } => {
            let text = if text.is_empty() {
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s)?;
                s
            } else {
                text.join(" ")
            };
            if text.trim().is_empty() {
                anyhow::bail!("nothing to add");
            }
            println!("{}", core::inbox_add(&root, &cfg, &text, Local::now())?.display());
        }
        Cmd::Daily => println!("{}", core::daily_today(&root, &cfg, Local::now())?.display()),
        Cmd::List { dir } => {
            for entry in core::list(&root, dir.as_deref().unwrap_or(""))? {
                println!("{entry}");
            }
        }
        Cmd::Read { path } => print!("{}", core::read(&root, &path)?),
    }
    Ok(())
}
