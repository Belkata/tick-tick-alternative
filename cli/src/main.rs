//! `tt`: the Tick Alt command-line tool.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use tt_core::{Store, Task};
use tt_sync::{Relays, SyncNode};

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
    /// Spike 2: talk to another device over iroh.
    #[command(subcommand)]
    Sync(SyncCommand),
}

#[derive(Subcommand)]
enum SyncCommand {
    /// Print this device's ticket and serve open tasks until Ctrl-C.
    Serve {
        /// Don't use n0's relays (direct addresses only).
        #[arg(long)]
        no_relay: bool,
    },
    /// Fetch the open tasks of the device whose ticket is given.
    Pull {
        ticket: String,
        /// Don't use n0's relays (direct addresses only).
        #[arg(long)]
        no_relay: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = db_path(cli.db)?;
    if let Command::Sync(cmd) = cli.command {
        return run_sync(db, cmd);
    }
    let store = Store::open(&db).with_context(|| format!("opening {}", db.display()))?;

    match cli.command {
        Command::Add { title } => {
            let task = store.add_task(&title.join(" "))?;
            println!("Added {}  {}", short_id(&task), task.title);
        }
        Command::Ls { all } => {
            let tasks = if all {
                store.open_tasks()?
            } else {
                store.inbox()?
            };
            if tasks.is_empty() {
                println!(
                    "{}",
                    if all {
                        "No open tasks."
                    } else {
                        "Inbox is empty."
                    }
                );
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
        Command::Sync(_) => unreachable!("handled above"),
    }
    Ok(())
}

fn db_path(flag: Option<PathBuf>) -> Result<PathBuf> {
    Ok(
        match flag.or_else(|| std::env::var_os("TT_DB").map(PathBuf::from)) {
            Some(p) => p,
            None => {
                let dir = dirs::data_dir()
                    .context("no platform data directory; pass --db")?
                    .join("tick-alt");
                std::fs::create_dir_all(&dir)
                    .with_context(|| format!("creating {}", dir.display()))?;
                dir.join("tt.db")
            }
        },
    )
}

#[tokio::main]
async fn run_sync(db: PathBuf, cmd: SyncCommand) -> Result<()> {
    // The node key lives next to the database: tt.db -> tt.key.
    let key = db.with_extension("key");
    let device = hostname();
    match cmd {
        SyncCommand::Serve { no_relay } => {
            let node = SyncNode::start(db, &key, device, relays(no_relay)).await?;
            if !no_relay && !node.wait_online(Duration::from_secs(10)).await {
                eprintln!("warning: no relay after 10 s; the ticket has direct addresses only");
            }
            println!("Node id: {}", node.id());
            for a in node.addrs() {
                println!("  {a}");
            }
            println!(
                "\nTicket:\n{}\n\nServing open tasks. Ctrl-C to stop.",
                node.ticket()?
            );
            let mut served = node.subscribe_served();
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => break,
                    s = served.recv() => if let Ok(s) = s {
                        println!(
                            "served {} tasks to {} ({})",
                            s.task_count,
                            s.peer_device,
                            &s.peer_id[..10]
                        );
                    }
                }
            }
            node.shutdown().await;
        }
        SyncCommand::Pull { ticket, no_relay } => {
            let node = SyncNode::start(db, &key, device, relays(no_relay)).await?;
            let r = node.pull(&ticket).await?;
            println!(
                "{} tasks from {} ({}) in {} ms",
                r.tasks.len(),
                r.peer_device,
                &r.peer_id[..10],
                r.elapsed.as_millis()
            );
            println!(
                "path: {} at reply, {} after hole punching; rtt {} ms",
                r.first_path.as_str(),
                r.final_path.as_str(),
                r.rtt.map_or("?".into(), |d| d.as_millis().to_string())
            );
            for p in &r.paths {
                println!("  {p}");
            }
            for t in &r.tasks {
                println!("{}  {}", &t.id[t.id.len() - SHORT_ID_LEN..], t.title);
            }
            node.shutdown().await;
        }
    }
    Ok(())
}

fn relays(no_relay: bool) -> Relays {
    if no_relay { Relays::Off } else { Relays::N0 }
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "desktop".into())
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
