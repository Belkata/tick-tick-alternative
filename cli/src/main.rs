//! `tt`: the Tick Alt command-line tool.

mod show;

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use chrono_tz::Tz;
use clap::{Args, Parser, Subcommand};
use show::{SHORT_ID_LEN, line, offset_label, parse_offset, priority_name, short_id, when};
use tt_api::Api;
use tt_core::{
    Filter, FilterRule, NewTask, ReminderKind, Repeat, RepeatMode, Schedule, Task, local_tz,
    parse_when,
};
use tt_sync::replicate::SyncEvent;
use tt_sync::{Relays, SyncNode};

#[derive(Parser)]
#[command(name = "tt", version, about = "Tick Alt from the terminal")]
struct Cli {
    /// Database file. Defaults to $TT_DB, then the platform data directory.
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

/// Dates accept `today`, `tomorrow`, `fri`, `+3d`, `2026-12-01`, optionally followed
/// by a time (`tomorrow 14:30`), or a bare time (`18:00`, today). Without a time the
/// date is all-day.
#[derive(Subcommand)]
enum Command {
    /// Add a task. With no list, tag or date it lands in Inbox.
    Add {
        /// Task title (words are joined with spaces).
        #[arg(required = true)]
        title: Vec<String>,
        #[command(flatten)]
        fields: TaskFields,
        /// Make it a subtask of this task id.
        #[arg(long)]
        parent: Option<String>,
    },
    /// Show Inbox (default), or tasks from a list, tag, filter or view.
    Ls(LsArgs),
    /// Show one task in full: dates, tags, checklist, reminders, subtasks.
    Show { id: String },
    /// Change a task's fields.
    Edit {
        id: String,
        /// New title.
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        fields: TaskFields,
        /// Remove the task from its list.
        #[arg(long, conflicts_with = "list")]
        no_list: bool,
        /// Clear start, due, and repeat.
        #[arg(long, conflicts_with_all = ["due", "start"])]
        no_date: bool,
        /// Make it a subtask of this task id.
        #[arg(long)]
        parent: Option<String>,
        /// Make it top-level again.
        #[arg(long, conflicts_with = "parent")]
        no_parent: bool,
        /// Show it on the calendar (`true`, the default) or keep it off (`false`).
        #[arg(long)]
        calendar: Option<bool>,
    },
    /// Mark a task done (a repeating one moves to its next date).
    Done { id: String },
    /// Undo completing a task.
    Reopen { id: String },
    /// Move a task (and its subtasks) to Trash.
    Rm { id: String },
    /// Bring a task back from Trash.
    Restore { id: String },
    /// Add tags to a task (missing tags are created), or remove them with --rm.
    Tag {
        id: String,
        /// Tag paths, e.g. `home/garden`.
        #[arg(required = true)]
        tags: Vec<String>,
        #[arg(long)]
        rm: bool,
    },
    /// List or manage tags.
    Tags {
        #[command(subcommand)]
        cmd: Option<TagsCmd>,
    },
    /// List or manage lists.
    Lists {
        #[command(subcommand)]
        cmd: Option<ListsCmd>,
    },
    /// Make a task repeat with an RRULE (e.g. `FREQ=WEEKLY;BYDAY=MO,TH`), or stop with --off.
    Repeat {
        id: String,
        rule: Option<String>,
        /// Count the next date from when it's completed, not from the schedule.
        #[arg(long)]
        after_completion: bool,
        #[arg(long, conflicts_with = "rule")]
        off: bool,
    },
    /// Add a reminder: an offset before the task's date (`15m`, `1h`, `0`) or a date and time.
    Remind {
        id: String,
        /// Offset or `"tomorrow 09:00"`. Omit to list the task's reminders.
        when: Option<String>,
        /// Remove the reminder with this number (from the listing).
        #[arg(long, conflicts_with = "when")]
        rm: Option<usize>,
    },
    /// Reminders due in the next hours.
    Reminders {
        #[arg(long, default_value_t = 24)]
        hours: i64,
    },
    /// Show a task's checklist, add an item, or tick / remove one by number.
    Check {
        id: String,
        /// Text of a new item.
        item: Vec<String>,
        /// Tick (or untick) item N.
        #[arg(long, conflicts_with = "item")]
        toggle: Option<usize>,
        /// Remove item N.
        #[arg(long, conflicts_with_all = ["item", "toggle"])]
        rm: Option<usize>,
    },
    /// List or manage saved filters.
    Filter {
        #[command(subcommand)]
        cmd: Option<FilterCmd>,
    },
    /// Recently completed tasks, newest first.
    Completed {
        #[arg(short, default_value_t = 20)]
        n: usize,
    },
    /// Import data from another app (one time, safe to repeat).
    Import {
        #[command(subcommand)]
        cmd: ImportCmd,
    },
    /// Spike 2: talk to another device over iroh.
    #[command(subcommand)]
    Sync(SyncCommand),
}

#[derive(Args)]
struct TaskFields {
    /// Put it in this list (name or id).
    #[arg(long, short)]
    list: Option<String>,
    /// Tag it (repeatable; missing tags are created).
    #[arg(long = "tag", short = 't')]
    tags: Vec<String>,
    /// Due date, e.g. `fri` or `tomorrow 14:30`.
    #[arg(long, short)]
    due: Option<String>,
    /// Start date.
    #[arg(long)]
    start: Option<String>,
    /// Length in minutes (calendar blocks).
    #[arg(long)]
    duration: Option<i64>,
    /// none, low, medium, high (or 0–3).
    #[arg(long, short)]
    priority: Option<String>,
    /// Notes (plain text).
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Args)]
struct LsArgs {
    /// Every open task.
    #[arg(long, group = "view")]
    all: bool,
    /// Tasks in a list.
    #[arg(long, short, group = "view")]
    list: Option<String>,
    /// Tasks with a tag (or a tag under it).
    #[arg(long, short = 't', group = "view")]
    tag: Option<String>,
    /// Run a saved filter.
    #[arg(long, short, group = "view")]
    filter: Option<String>,
    /// Run a filter rule given as JSON.
    #[arg(long, group = "view")]
    rule: Option<String>,
    /// Trash.
    #[arg(long, group = "view")]
    trash: bool,
    /// Include done tasks (list, tag and --all views).
    #[arg(long)]
    done: bool,
}

#[derive(Subcommand)]
enum TagsCmd {
    /// Create a tag path, e.g. `home/garden`.
    Add { path: String },
    /// Rename a tag (the last part of its path).
    Rename { path: String, name: String },
    /// Move a tag under another, or to the top without a parent.
    Mv {
        path: String,
        parent: Option<String>,
    },
    /// Arrange a tag among its siblings: before another one, or last without one.
    Order {
        path: String,
        #[arg(long)]
        before: Option<String>,
    },
    /// Delete a tag and the tags under it, removing them from tasks.
    Rm { path: String },
    /// Set a tag's colour (`#rrggbb`) and icon (an emoji); leave both out to clear them.
    Style {
        path: String,
        color: Option<String>,
        #[arg(long)]
        icon: Option<String>,
    },
}

#[derive(Subcommand)]
enum ListsCmd {
    Add {
        name: String,
    },
    Rename {
        name: String,
        new_name: String,
    },
    /// Hide a list without deleting it (--undo to bring it back).
    Archive {
        name: String,
        #[arg(long)]
        undo: bool,
    },
    /// Delete a list; its tasks stay, with no list.
    Rm {
        name: String,
    },
}

#[derive(Subcommand)]
enum FilterCmd {
    /// Save a filter: `tt filter add Today '{"field":"due","op":"on_or_before","value":"today"}'`.
    Add {
        name: String,
        rule: String,
        /// Sort keys, e.g. `--sort priority_desc --sort due_asc`.
        #[arg(long)]
        sort: Vec<String>,
        #[arg(long)]
        show_completed: bool,
    },
    /// Show a filter's rule as JSON.
    Show {
        name: String,
    },
    Rm {
        name: String,
    },
    /// Set a filter's icon (an emoji); leave it out to clear it.
    Icon {
        name: String,
        icon: Option<String>,
    },
}

#[derive(Subcommand)]
enum ImportCmd {
    /// Import a TickTick export file (JSON from the export step).
    Ticktick {
        file: PathBuf,
        /// Count and report what would happen, without touching the database.
        #[arg(long)]
        dry_run: bool,
    },
}

fn run_import(db: &std::path::Path, cmd: ImportCmd) -> Result<()> {
    let ImportCmd::Ticktick { file, dry_run } = cmd;
    let text =
        std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
    let export = tt_core::parse_export(&text)?;
    // A dry run imports into a throwaway in-memory database.
    let store = if dry_run {
        tt_core::Store::open_in_memory()?
    } else {
        tt_core::Store::open(db).with_context(|| format!("opening {}", db.display()))?
    };
    let r = store.import_ticktick(&export)?;
    println!(
        "{}",
        if dry_run {
            "Dry run (nothing written):"
        } else {
            "Imported:"
        }
    );
    println!("  lists {}, tags {}", r.lists, r.tags);
    println!(
        "  open tasks {}, done tasks {}, subtasks {}",
        r.open_tasks, r.done_tasks, r.subtasks
    );
    println!(
        "  checklist items {}, repeats {}, reminders {}",
        r.checklist_items, r.repeats, r.reminders
    );
    println!(
        "  already there (skipped) {}, kanban columns dropped {}",
        r.already_there, r.dropped_columns
    );
    for n in &r.notes {
        println!("  note: {n}");
    }
    Ok(())
}

#[derive(Subcommand)]
enum SyncCommand {
    /// Pair a new device: print a one-time ticket (valid 5 minutes), wait for it to join, sync.
    Pair {
        /// Don't use n0's relays (direct addresses only).
        #[arg(long)]
        no_relay: bool,
    },
    /// Join the device that printed this pairing ticket (`tt2…`), then sync with it.
    Join {
        ticket: String,
        #[arg(long)]
        no_relay: bool,
    },
    /// Sync now with every paired device that's online.
    Now {
        #[arg(long)]
        no_relay: bool,
    },
    /// List paired devices.
    Peers,
    /// Unpair a device (by name or the start of its node id).
    Forget { device: String },
    /// Drop superseded ops from the op log (tt-daemon also does this daily).
    Compact,
    /// Serve paired devices (and Spike 2 pulls) until Ctrl-C.
    Serve {
        /// Don't use n0's relays (direct addresses only).
        #[arg(long)]
        no_relay: bool,
    },
    /// Spike 2: fetch the open tasks of the device whose `tt1…` ticket is given.
    Pull {
        ticket: String,
        /// Don't use n0's relays (direct addresses only).
        #[arg(long)]
        no_relay: bool,
    },
}

fn main() -> Result<()> {
    // Rust ignores SIGPIPE, so printing into a closed pipe (`tt ls | head -1`) panics.
    // A CLI should just stop quietly, like other Unix tools: restore the default.
    #[cfg(unix)]
    // SAFETY: first thing in main, before any threads; only resets a signal disposition.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    let db = db_path(cli.db)?;
    if let Command::Import { cmd } = cli.command {
        return run_import(&db, cmd);
    }
    if let Command::Sync(cmd) = cli.command {
        // While tt-daemon serves this database it owns the device key: ask it to sync.
        if let Some(client) = tt_api::Client::for_db(&db) {
            return sync_via_daemon(Api::remote(client), cmd);
        }
        return run_sync(db, cmd);
    }
    // Through tt-daemon when it serves this database; else open the file directly
    // (the daemon notices those writes and tells the app).
    let store = Api::connect_or_open(&db).with_context(|| format!("opening {}", db.display()))?;
    let ctx = Ctx {
        store,
        tz: local_tz(),
        now: now_ms(),
    };
    ctx.run(cli.command)
}

struct Ctx {
    store: Api,
    tz: Tz,
    now: i64,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_priority(s: &str) -> Result<u8> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "0" | "none" => 0,
        "1" | "low" => 1,
        "2" | "med" | "medium" => 2,
        "3" | "high" => 3,
        other => bail!("unknown priority '{other}' (none, low, medium, high)"),
    })
}

impl Ctx {
    fn run(&self, command: Command) -> Result<()> {
        let s = &self.store;
        match command {
            Command::Add {
                title,
                fields,
                parent,
            } => {
                let mut new = NewTask {
                    title: title.join(" "),
                    parent_id: parent.map(|p| self.task_id(&p)).transpose()?,
                    ..Default::default()
                };
                if let Some(l) = &fields.list {
                    new.list_id = Some(self.list_id(l)?);
                }
                for t in &fields.tags {
                    new.tags.push(s.ensure_tag_path(t)?);
                }
                if let Some(p) = &fields.priority {
                    new.priority = parse_priority(p)?;
                }
                new.notes = fields.notes.clone().unwrap_or_default();
                new.schedule = self.schedule(&fields, None)?;
                let task = s.create_task(&new)?;
                let where_ = if s.inbox()?.iter().any(|t| t.id == task.id) {
                    " (Inbox)"
                } else {
                    ""
                };
                println!("Added {}  {}{where_}", short_id(&task.id), task.title);
            }
            Command::Ls(args) => self.ls(args)?,
            Command::Show { id } => self.show(&self.task_id(&id)?)?,
            Command::Edit {
                id,
                title,
                fields,
                no_list,
                no_date,
                parent,
                no_parent,
                calendar,
            } => {
                let id = self.task_id(&id)?;
                let task = s.get_task(&id)?.context("task vanished")?;
                if let Some(t) = title {
                    s.set_title(&id, &t)?;
                }
                if let Some(n) = &fields.notes {
                    s.set_notes(&id, n)?;
                }
                if let Some(p) = &fields.priority {
                    s.set_priority(&id, parse_priority(p)?)?;
                }
                if let Some(l) = &fields.list {
                    s.set_task_list(&id, Some(&self.list_id(l)?))?;
                } else if no_list {
                    s.set_task_list(&id, None::<&str>)?;
                }
                for t in &fields.tags {
                    s.tag_task(&id, &s.ensure_tag_path(t)?)?;
                }
                if no_date {
                    s.set_repeat(&id, None)?;
                    s.set_schedule(&id, None)?;
                } else if fields.due.is_some()
                    || fields.start.is_some()
                    || fields.duration.is_some()
                {
                    s.set_schedule(&id, Some(&self.schedule(&fields, Some(&task))?))?;
                }
                if let Some(show) = calendar {
                    s.set_in_calendar(&id, show)?;
                }
                if let Some(p) = parent {
                    s.set_parent(&id, Some(&self.task_id(&p)?))?;
                } else if no_parent {
                    s.set_parent(&id, None::<&str>)?;
                }
                println!(
                    "{}",
                    line(
                        s,
                        &s.get_task(&id)?.context("task vanished")?,
                        self.now,
                        self.tz,
                        0
                    )
                );
            }
            Command::Done { id } => {
                let id = self.task_id(&id)?;
                if !s.complete_task(&id)? {
                    bail!("task isn't open");
                }
                let t = s.get_task(&id)?.context("task vanished")?;
                match (t.status, t.due_at) {
                    (tt_core::Status::Open, Some(next)) => {
                        println!("Done. Next: {}", when(next, t.all_day, self.now, self.tz))
                    }
                    _ => println!("Done."),
                }
            }
            Command::Reopen { id } => {
                if !s.reopen_task(&self.task_id(&id)?)? {
                    bail!("task isn't done");
                }
                println!("Reopened.");
            }
            Command::Rm { id } => {
                if !s.trash_task(&self.task_id(&id)?)? {
                    bail!("task is already in Trash");
                }
                println!("Moved to Trash.");
            }
            Command::Restore { id } => {
                if !s.restore_task(&self.task_id(&id)?)? {
                    bail!("task isn't in Trash");
                }
                println!("Restored.");
            }
            Command::Tag { id, tags, rm } => {
                let id = self.task_id(&id)?;
                for t in tags {
                    if rm {
                        let tag = s.find_tag(&t)?.with_context(|| format!("no tag '{t}'"))?;
                        s.untag_task(&id, &tag.id)?;
                    } else {
                        s.tag_task(&id, &s.ensure_tag_path(&t)?)?;
                    }
                }
                println!(
                    "{}",
                    line(
                        s,
                        &s.get_task(&id)?.context("task vanished")?,
                        self.now,
                        self.tz,
                        0
                    )
                );
            }
            Command::Tags { cmd } => self.tags(cmd)?,
            Command::Lists { cmd } => self.lists(cmd)?,
            Command::Repeat {
                id,
                rule,
                after_completion,
                off,
            } => {
                let id = self.task_id(&id)?;
                if off {
                    s.set_repeat(&id, None)?;
                    println!("No longer repeats.");
                } else {
                    let rule = rule.context("give an RRULE like FREQ=WEEKLY;BYDAY=MO, or --off")?;
                    let mode = if after_completion {
                        RepeatMode::AfterCompletion
                    } else {
                        RepeatMode::Fixed
                    };
                    s.set_repeat(&id, Some(&Repeat { rule, mode }))?;
                    println!("Repeats.");
                }
            }
            Command::Remind { id, when: at, rm } => {
                let id = self.task_id(&id)?;
                let reminders = s.reminders(&id)?;
                if let Some(n) = rm {
                    let r = reminders
                        .get(n.wrapping_sub(1))
                        .context("no reminder with that number")?;
                    s.remove_reminder(&r.id)?;
                    println!("Removed.");
                } else if let Some(at) = at {
                    let kind = match parse_offset(&at) {
                        Some(offset_min) => ReminderKind::Relative { offset_min },
                        None => ReminderKind::Absolute {
                            at: parse_when(&at, self.now, self.tz)?.0,
                        },
                    };
                    s.add_reminder(&id, kind)?;
                    println!("Reminder added.");
                } else if reminders.is_empty() {
                    println!("No reminders.");
                } else {
                    for (i, r) in reminders.iter().enumerate() {
                        println!("{}. {}", i + 1, self.reminder_label(r.kind));
                    }
                }
            }
            Command::Reminders { hours } => {
                let fires =
                    s.upcoming_reminders(self.now, self.now + hours * 3_600_000, self.tz)?;
                if fires.is_empty() {
                    println!("No reminders in the next {hours} h.");
                }
                for f in fires {
                    println!(
                        "{}  {}  {}",
                        when(f.fire_at, false, self.now, self.tz),
                        short_id(&f.task_id),
                        f.title
                    );
                }
            }
            Command::Check {
                id,
                item,
                toggle,
                rm,
            } => {
                let id = self.task_id(&id)?;
                let items = s.checklist(&id)?;
                let pick = |n: usize| {
                    items
                        .get(n.wrapping_sub(1))
                        .context("no item with that number")
                };
                if !item.is_empty() {
                    s.add_checklist_item(&id, item.join(" "))?;
                } else if let Some(n) = toggle {
                    let it = pick(n)?;
                    s.set_checklist_done(&it.id, !it.done)?;
                } else if let Some(n) = rm {
                    s.remove_checklist_item(&pick(n)?.id)?;
                }
                for (i, it) in s.checklist(&id)?.iter().enumerate() {
                    println!(
                        "{}. [{}] {}",
                        i + 1,
                        if it.done { "x" } else { " " },
                        it.title
                    );
                }
            }
            Command::Filter { cmd } => self.filters(cmd)?,
            Command::Completed { n } => {
                for c in s.completed(n)? {
                    let mark = if c.repeating { "↻" } else { "✓" };
                    println!(
                        "{}  {mark} {}   done {}",
                        short_id(&c.task_id),
                        c.title,
                        when(c.completed_at, false, self.now, self.tz)
                    );
                }
            }
            Command::Sync(_) | Command::Import { .. } => unreachable!("handled in main"),
        }
        Ok(())
    }

    fn task_id(&self, short: &str) -> Result<String> {
        self.store
            .resolve_task_id(short)?
            .with_context(|| format!("no single task matches id '{short}'"))
    }

    fn list_id(&self, name: &str) -> Result<String> {
        Ok(self
            .store
            .find_list(name)?
            .with_context(|| format!("no list '{name}' (tt lists add …)"))?
            .id)
    }

    /// Dates from the flags, on top of `base` (the task being edited).
    fn schedule(&self, f: &TaskFields, base: Option<&Task>) -> Result<Schedule> {
        let mut s = Schedule {
            start_at: base.and_then(|t| t.start_at),
            due_at: base.and_then(|t| t.due_at),
            all_day: base.is_some_and(|t| t.all_day),
            tz: base.and_then(|t| t.tz.clone()),
            duration_min: base.and_then(|t| t.duration_min),
        };
        let mut all_day = None;
        if let Some(d) = &f.due {
            let (ms, ad) = parse_when(d, self.now, self.tz)?;
            s.due_at = Some(ms);
            all_day = Some(ad);
        }
        if let Some(st) = &f.start {
            let (ms, ad) = parse_when(st, self.now, self.tz)?;
            if all_day.is_some_and(|a| a != ad) {
                bail!("start and due must both have a time, or both be all-day");
            }
            s.start_at = Some(ms);
            all_day = Some(ad);
        }
        if let Some(ad) = all_day {
            if ad != s.all_day && base.is_some() {
                // Switching between all-day and timed: drop the other date rather than mix kinds.
                if f.due.is_none() {
                    s.due_at = None;
                }
                if f.start.is_none() {
                    s.start_at = None;
                }
            }
            s.all_day = ad;
        }
        s.tz = if s.all_day {
            None
        } else {
            Some(self.tz.name().to_string())
        };
        if f.duration.is_some() {
            s.duration_min = f.duration;
        }
        Ok(s)
    }

    fn print_tasks(&self, tasks: &[Task], empty: &str) {
        if tasks.is_empty() {
            println!("{empty}");
        }
        for t in tasks {
            println!("{}", line(&self.store, t, self.now, self.tz, 0));
        }
    }

    fn ls(&self, a: LsArgs) -> Result<()> {
        let s = &self.store;
        let run = |rule: &str| -> Result<Vec<Task>> {
            let rule: FilterRule = serde_json::from_str(rule)?;
            s.run_rule(&rule, &[], a.done, self.now, self.tz)
        };
        let cond = |field: &str, op: &str, value: &str| {
            serde_json::json!({"field": field, "op": op, "value": value}).to_string()
        };
        if a.all {
            self.print_tasks(&run(r#"{"all":[]}"#)?, "No open tasks.");
        } else if let Some(l) = &a.list {
            self.list_id(l)?;
            self.print_tasks(&run(&cond("list", "is", l))?, "No tasks in that list.");
        } else if let Some(t) = &a.tag {
            s.find_tag(t)?.with_context(|| format!("no tag '{t}'"))?;
            self.print_tasks(&run(&cond("tag", "has", t))?, "No tasks with that tag.");
        } else if let Some(f) = &a.filter {
            let f = s
                .find_filter(f)?
                .with_context(|| format!("no filter '{f}'"))?;
            self.print_tasks(&s.run_filter(&f.id, self.now, self.tz)?, "Nothing matches.");
        } else if let Some(rule) = &a.rule {
            self.print_tasks(&run(rule)?, "Nothing matches.");
        } else if a.trash {
            self.print_tasks(&s.trash()?, "Trash is empty.");
        } else {
            self.print_tasks(&s.inbox()?, "Inbox is empty.");
        }
        Ok(())
    }

    fn show(&self, id: &str) -> Result<()> {
        let s = &self.store;
        let t = s.get_task(id)?.context("task vanished")?;
        println!("{}  {}", short_id(&t.id), t.title);
        println!("  status    {}", t.status.as_str());
        if t.priority > 0 {
            println!("  priority  {}", priority_name(t.priority));
        }
        if let Some(l) = t
            .list_id
            .as_deref()
            .and_then(|l| s.find_list(l).ok().flatten())
        {
            println!("  list      {}", l.name);
        }
        let tags: Vec<String> = s.task_tags(id)?.into_iter().map(|g| g.path).collect();
        if !tags.is_empty() {
            println!("  tags      {}", tags.join(", "));
        }
        if let Some(st) = t.start_at {
            println!("  start     {}", when(st, t.all_day, self.now, self.tz));
        }
        if let Some(d) = t.due_at {
            println!("  due       {}", when(d, t.all_day, self.now, self.tz));
        }
        if let Some(m) = t.duration_min {
            println!("  duration  {m} min");
        }
        if !t.in_calendar {
            println!("  calendar  hidden");
        }
        if let Some(r) = &t.repeat_rule {
            println!(
                "  repeats   {r} ({})",
                t.repeat_mode.as_str().replace('_', " ")
            );
        }
        if let Some(p) = &t.parent_id {
            println!("  subtask of {}", short_id(p));
        }
        for r in s.reminders(id)? {
            println!("  reminder  {}", self.reminder_label(r.kind));
        }
        if !t.notes.is_empty() {
            println!("\n{}", t.notes);
        }
        let items = s.checklist(id)?;
        if !items.is_empty() {
            println!();
            for it in items {
                println!("  [{}] {}", if it.done { "x" } else { " " }, it.title);
            }
        }
        let subs = s.subtasks(id)?;
        if !subs.is_empty() {
            println!();
            for sub in subs {
                println!("{}", line(s, &sub, self.now, self.tz, 1));
            }
        }
        Ok(())
    }

    fn reminder_label(&self, kind: ReminderKind) -> String {
        match kind {
            ReminderKind::Relative { offset_min } => offset_label(offset_min),
            ReminderKind::Absolute { at } => when(at, false, self.now, self.tz),
        }
    }

    fn tags(&self, cmd: Option<TagsCmd>) -> Result<()> {
        let s = &self.store;
        let find = |p: &str| -> Result<String> {
            Ok(s.find_tag(p)?.with_context(|| format!("no tag '{p}'"))?.id)
        };
        match cmd {
            None => {
                let tags = s.tags()?;
                if tags.is_empty() {
                    println!("No tags.");
                }
                for t in tags {
                    let depth = t.path.matches('/').count();
                    let n = s
                        .run_rule(
                            &serde_json::from_value(serde_json::json!({
                                "field": "tag", "op": "has", "value": t.id, "include_children": false
                            }))?,
                            &[],
                            false,
                            self.now,
                            self.tz,
                        )?
                        .len();
                    println!("{}{}  ({n})", "  ".repeat(depth), t.name);
                }
            }
            Some(TagsCmd::Add { path }) => {
                s.ensure_tag_path(&path)?;
                println!("Tag {path} ready.");
            }
            Some(TagsCmd::Rename { path, name }) => s.rename_tag(&find(&path)?, &name)?,
            Some(TagsCmd::Mv { path, parent }) => {
                let parent = parent.map(|p| find(&p)).transpose()?;
                s.move_tag(&find(&path)?, parent.as_deref())?
            }
            Some(TagsCmd::Order { path, before }) => {
                let before = before.map(|b| find(&b)).transpose()?;
                s.place_tag(&find(&path)?, before.as_deref())?
            }
            Some(TagsCmd::Rm { path }) => {
                s.delete_tag(&find(&path)?)?;
                println!("Deleted {path}.");
            }
            Some(TagsCmd::Style { path, color, icon }) => {
                s.set_tag_style(&find(&path)?, color.as_deref(), icon.as_deref())?
            }
        }
        Ok(())
    }

    fn lists(&self, cmd: Option<ListsCmd>) -> Result<()> {
        let s = &self.store;
        match cmd {
            None => {
                let lists = s.lists(true)?;
                if lists.is_empty() {
                    println!("No lists.");
                }
                for l in lists {
                    let n = s
                        .run_rule(
                            &serde_json::from_value(
                                serde_json::json!({"field":"list","op":"is","value":l.id}),
                            )?,
                            &[],
                            false,
                            self.now,
                            self.tz,
                        )?
                        .len();
                    let archived = if l.archived { "  [archived]" } else { "" };
                    println!("{}  ({n}){archived}", l.name);
                }
            }
            Some(ListsCmd::Add { name }) => {
                s.create_list(&name)?;
                println!("List {name} created.");
            }
            Some(ListsCmd::Rename { name, new_name }) => {
                s.rename_list(&self.list_id(&name)?, &new_name)?
            }
            Some(ListsCmd::Archive { name, undo }) => {
                s.set_list_archived(&self.list_id(&name)?, !undo)?
            }
            Some(ListsCmd::Rm { name }) => {
                s.delete_list(&self.list_id(&name)?)?;
                println!("Deleted {name}; its tasks have no list now.");
            }
        }
        Ok(())
    }

    fn filters(&self, cmd: Option<FilterCmd>) -> Result<()> {
        let s = &self.store;
        let find = |n: &str| -> Result<tt_core::SavedFilter> {
            s.find_filter(n)?
                .with_context(|| format!("no filter '{n}'"))
        };
        match cmd {
            None => {
                let filters = s.filters()?;
                if filters.is_empty() {
                    println!("No filters. Add one with: tt filter add <name> '<rule JSON>'");
                }
                for f in filters {
                    let n = s.run_filter(&f.id, self.now, self.tz)?.len();
                    println!(
                        "{}{}  ({n})",
                        f.filter.icon.map(|i| format!("{i} ")).unwrap_or_default(),
                        f.filter.name
                    );
                }
            }
            Some(FilterCmd::Add {
                name,
                rule,
                sort,
                show_completed,
            }) => {
                let rule: FilterRule =
                    serde_json::from_str(&rule).context("rule is not valid JSON")?;
                let def = Filter {
                    name: name.clone(),
                    icon: None,
                    rule,
                    sort,
                    group: None,
                    show_completed,
                };
                match s.find_filter(&name)? {
                    Some(existing) => s.update_filter(&existing.id, &def)?,
                    None => {
                        s.save_filter(&def)?;
                    }
                }
                println!("Filter {name} saved.");
            }
            Some(FilterCmd::Show { name }) => {
                let f = find(&name)?;
                println!("{}", serde_json::to_string_pretty(&f.filter.rule)?);
            }
            Some(FilterCmd::Icon { name, icon }) => {
                let f = find(&name)?;
                let mut def = f.filter.clone();
                def.icon = icon.filter(|i| !i.trim().is_empty());
                s.update_filter(&f.id, &def)?;
                println!("Filter {name} icon updated.");
            }
            Some(FilterCmd::Rm { name }) => {
                s.delete_filter(&find(&name)?.id)?;
                println!("Deleted {name}.");
            }
        }
        Ok(())
    }
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
        SyncCommand::Pair { no_relay } => {
            let node = SyncNode::start(db, &key, device, relays(no_relay)).await?;
            if !no_relay && !node.wait_online(Duration::from_secs(10)).await {
                eprintln!("warning: no relay after 10 s; the ticket has direct addresses only");
            }
            let mut events = node.subscribe_sync();
            println!(
                "On the other device run:\n\n  tt sync join {}\n\nWaiting up to 5 minutes. Ctrl-C to cancel.",
                node.pairing_ticket()?
            );
            let deadline = tokio::time::sleep(tt_sync::replicate::PAIR_TTL);
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    _ = &mut deadline => { println!("The ticket expired."); break; }
                    _ = tokio::signal::ctrl_c() => break,
                    ev = events.recv() => match ev {
                        Ok(SyncEvent::Paired { name, .. }) => println!("Paired with {name}."),
                        Ok(SyncEvent::Synced(r)) => { print_report(&r); break; }
                        Ok(SyncEvent::Refused { reason, .. }) => println!("Refused a device: {reason}"),
                        Ok(SyncEvent::TrustChanged { .. }) | Err(_) => {}
                    },
                }
            }
            node.shutdown().await;
        }
        SyncCommand::Join { ticket, no_relay } => {
            let node = SyncNode::start(db, &key, device, relays(no_relay)).await?;
            let r = node.join(&ticket).await?;
            println!("Paired with {}.", r.peer_name);
            print_report(&r);
            node.shutdown().await;
        }
        SyncCommand::Now { no_relay } => {
            let node = SyncNode::start(db, &key, device, relays(no_relay)).await?;
            let results = node.sync_all().await;
            if results.is_empty() {
                println!("No paired devices. Pair one with `tt sync pair`.");
            }
            for (peer, r) in results {
                match r {
                    Ok(r) => print_report(&r),
                    Err(e) => println!("{}: {e:#}", peer.name),
                }
            }
            node.shutdown().await;
        }
        SyncCommand::Compact => compact(&Api::local(tt_core::Store::open(&db)?))?,
        SyncCommand::Peers => {
            let store = tt_core::Store::open(&db)?;
            let (now, tz) = (now_ms(), local_tz());
            let peers = store.peers()?;
            if peers.is_empty() {
                println!("No paired devices.");
            }
            for p in peers {
                let seen = p.last_seen.map_or("never synced".into(), |t| {
                    format!("synced {}", when(t, false, now, tz))
                });
                println!("{}  {}  ({seen})", &p.node_id[..10], p.name);
            }
        }
        SyncCommand::Forget { device: who } => {
            let store = tt_core::Store::open(&db)?;
            let matches: Vec<_> = store
                .peers()?
                .into_iter()
                .filter(|p| p.name.eq_ignore_ascii_case(&who) || p.node_id.starts_with(&who))
                .collect();
            match matches.as_slice() {
                [] => bail!("no paired device '{who}'"),
                [p] => {
                    store.remove_peer(&p.node_id)?;
                    println!("Forgot {}.", p.name);
                }
                _ => bail!("'{who}' matches several devices; use more of the node id"),
            }
        }
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
            let mut synced = node.subscribe_sync();
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => break,
                    ev = synced.recv() => match ev {
                        Ok(SyncEvent::Synced(r)) => print_report(&r),
                        Ok(SyncEvent::Refused { peer_id, reason }) => println!("refused {}: {reason}", &peer_id[..10]),
                        _ => {}
                    },
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

/// `tt sync` while the daemon runs: the same commands, carried out by its sync node.
fn sync_via_daemon(api: Api, cmd: SyncCommand) -> Result<()> {
    let outcome = |o: &tt_api::SyncOutcome| match &o.error {
        None => println!(
            "Synced with {}: sent {}, received {} ({} applied).",
            o.peer_name, o.sent, o.received, o.applied
        ),
        Some(e) => println!("{}: {e}", o.peer_name),
    };
    match cmd {
        SyncCommand::Pair { .. } => {
            println!(
                "On the other device run:\n\n  tt sync join {}\n\nThe ticket works once, for 5 minutes; tt-daemon answers it.",
                api.sync_pair()?
            );
        }
        SyncCommand::Join { ticket, .. } => {
            let o = api.sync_join(ticket)?;
            println!("Paired with {}.", o.peer_name);
            outcome(&o);
        }
        SyncCommand::Now { .. } => {
            let all = api.sync_now()?;
            if all.is_empty() {
                println!("No paired devices. Pair one with `tt sync pair`.");
            }
            all.iter().for_each(outcome);
        }
        SyncCommand::Peers => {
            let (now, tz) = (now_ms(), local_tz());
            let status = api.sync_status()?;
            println!("This device: {} (tt-daemon)", &status.node_id[..10]);
            let peers = api.peers()?;
            if peers.is_empty() {
                println!("No paired devices.");
            }
            for p in peers {
                let seen = p.last_seen.map_or("never synced".into(), |t| {
                    format!("synced {}", when(t, false, now, tz))
                });
                println!("{}  {}  ({seen})", &p.node_id[..10], p.name);
            }
        }
        SyncCommand::Forget { device: who } => {
            let matches: Vec<_> = api
                .peers()?
                .into_iter()
                .filter(|p| p.name.eq_ignore_ascii_case(&who) || p.node_id.starts_with(&who))
                .collect();
            match matches.as_slice() {
                [] => bail!("no paired device '{who}'"),
                [p] => {
                    api.remove_peer(&p.node_id)?;
                    println!("Forgot {}.", p.name);
                }
                _ => bail!("'{who}' matches several devices; use more of the node id"),
            }
        }
        SyncCommand::Compact => compact(&api)?,
        SyncCommand::Serve { .. } => bail!("tt-daemon already serves sync for this database"),
        SyncCommand::Pull { .. } => bail!(
            "stop tt-daemon to use the Spike 2 pull (it holds this device's key): systemctl --user stop tick-alt"
        ),
    }
    Ok(())
}

fn compact(api: &Api) -> Result<()> {
    let before = api.op_count()?;
    let dropped = api.compact()?;
    println!(
        "Dropped {dropped} superseded ops; {} remain.",
        before - dropped
    );
    Ok(())
}

fn print_report(r: &tt_sync::replicate::SyncReport) {
    println!(
        "Synced with {}: sent {}, received {} ({} applied).",
        r.peer_name, r.sent, r.received, r.applied
    );
    if !r.learned.is_empty() {
        println!(
            "Now also paired with {} (through {}).",
            r.learned.join(", "),
            r.peer_name
        );
    }
    if !r.dropped.is_empty() {
        println!(
            "Forgot {} ({} had unpaired them).",
            r.dropped.join(", "),
            r.peer_name
        );
    }
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
