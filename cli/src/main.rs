//! `tt`: the Tick Alt command-line tool.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use tt_core::{Store, Task};

/// Characters of the task id that `tt` prints and accepts.
const SHORT_ID_LEN: usize = 8;

#[derive(Parser)]
#[command(name = "tt", version, about = "Tick Alt from the terminal")]
struct Cli {
    /// Database file. Defaults to $TT_DB, then the platform data directory.
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add a task to Inbox.
    Add {
        /// Task title (words are joined with spaces).
        #[arg(required = true)]
        title: Vec<String>,
    },
    /// Show Inbox (default) or all open tasks.
    Ls {
        /// Show every open task, not just Inbox.
        #[arg(long)]
        all: bool,
    },
    /// Mark a task done by its short id.
    Done { id: String },
    /// Move a task to Trash by its short id.
    Rm { id: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = open_store(cli.db)?;

    match cli.command {
        Command::Add { title } => {
            let task = store.add_task(&title.join(" "))?;
            println!("Added {}  {}", short_id(&task), task.title);
        }
        Command::Ls { all } => {
            let tasks = if all { store.open_tasks()? } else { store.inbox()? };
            if tasks.is_empty() {
                println!("{}", if all { "No open tasks." } else { "Inbox is empty." });
            }
            for t in &tasks {
                println!("{}  {}", short_id(t), t.title);
            }
        }
        Command::Done { id } => {
            let id = resolve(&store, &id)?;
            if !store.complete_task(&id)? {
                bail!("task is already done");
            }
            println!("Done.");
        }
        Command::Rm { id } => {
            let id = resolve(&store, &id)?;
            if !store.trash_task(&id)? {
                bail!("task is already in Trash");
            }
            println!("Moved to Trash.");
        }
    }
    Ok(())
}

fn open_store(flag: Option<PathBuf>) -> Result<Store> {
    let path = match flag.or_else(|| std::env::var_os("TT_DB").map(PathBuf::from)) {
        Some(p) => p,
        None => {
            let dir = dirs::data_dir()
                .context("no platform data directory; pass --db")?
                .join("tick-alt");
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("creating {}", dir.display()))?;
            dir.join("tt.db")
        }
    };
    Store::open(&path).with_context(|| format!("opening {}", path.display()))
}

/// UUIDv7 starts with a timestamp, so the random tail is the part that tells tasks apart.
fn short_id(task: &Task) -> &str {
    &task.id[task.id.len() - SHORT_ID_LEN..]
}

fn resolve(store: &Store, short: &str) -> Result<String> {
    store
        .resolve_task_id(short)?
        .with_context(|| format!("no single task matches id '{short}'"))
}
