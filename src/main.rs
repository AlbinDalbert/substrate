mod config;
mod core;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(about = "Convenience verbs over plain markdown folders")]
struct Cli {
    /// The substrate to operate on. Default: the nearest one at or above the current folder
    #[arg(short, long, env = "SUBSTRATE")]
    substrate: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Make a folder a substrate (default: the current folder)
    Init { path: Option<PathBuf> },
    /// Add a new file to the substrate's inbox (text from args, or stdin if none)
    Inbox { text: Vec<String> },
    /// Print the path to today's scratch file, creating it if missing. One per day, never needs processing
    Scratch,
    /// List a folder
    List {
        dir: Option<String>,
        /// Show subfolders recursively, like `tree`
        #[arg(short, long)]
        tree: bool,
    },
    /// Print a file
    Read { path: String },
    /// Move a file or folder, into `to` if it's a folder. Never overwrites
    Move { from: String, to: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;

    if let Cmd::Init { path } = &cli.cmd {
        let base = cli.substrate.as_deref().unwrap_or(&cwd);
        let dir = base.join(path.as_deref().unwrap_or(Path::new(".")));
        if !dir.is_dir() {
            bail!("no such folder: {}", dir.display());
        }
        let tz = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into());
        core::init(&dir.canonicalize()?, &tz)?;
        println!("{}", dir.canonicalize()?.display());
        return Ok(());
    }

    // Substrate features act on the substrate. File operations take paths relative to
    // where you are (or the substrate's folder with -s), and can't leave the substrate.
    let (root, here) = match &cli.substrate {
        Some(s) => {
            let root = s.canonicalize().with_context(|| format!("no such folder: {}", s.display()))?;
            if core::find(&root) != Some(root.clone()) {
                bail!("not a substrate: {}", root.display());
            }
            (root, PathBuf::new())
        }
        None => {
            let cwd = cwd.canonicalize()?;
            let root = core::find(&cwd).context("not inside a substrate (use -s, or `substrate init`)")?;
            let here = cwd.strip_prefix(&root)?.to_path_buf();
            (root, here)
        }
    };
    let rel = |p: &str| here.join(p).to_string_lossy().into_owned();
    let cfg = config::Config::load(&root)?;

    match cli.cmd {
        Cmd::Init { .. } => unreachable!(),
        Cmd::Inbox { text } => {
            let text = if text.is_empty() {
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s)?;
                s
            } else {
                text.join(" ")
            };
            if text.trim().is_empty() {
                bail!("nothing to add");
            }
            println!("{}", core::inbox_add(&root, &cfg, &text, core::now(&cfg)?)?.display());
        }
        Cmd::Scratch => println!("{}", core::scratch_today(&root, &cfg, core::now(&cfg)?)?.display()),
        Cmd::List { dir, tree } => {
            let dir = rel(dir.as_deref().unwrap_or(""));
            if tree {
                println!(".");
            }
            let lines = if tree { core::tree(&root, &dir)? } else { core::list(&root, &dir)? };
            for line in lines {
                println!("{line}");
            }
        }
        Cmd::Read { path } => print!("{}", core::read(&root, &rel(&path))?),
        Cmd::Move { from, to } => println!("{}", core::move_path(&root, &rel(&from), &rel(&to))?.display()),
    }
    Ok(())
}
