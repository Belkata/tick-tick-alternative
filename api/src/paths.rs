//! Where the database and the daemon's socket live.

use std::path::{Path, PathBuf};

/// The desktop database: `$UTRE_DB` (the old `$TT_DB` still works), else `<data dir>/utre/utre.db`
/// (`~/.local/share/utre/utre.db` on Linux, `~/Library/Application Support/utre/utre.db` on macOS).
///
/// The first call after the rename from Tick Alt moves the old data directory over
/// ([`migrate_legacy_data`]), so nothing opens an empty database by mistake.
pub fn default_db() -> std::io::Result<PathBuf> {
    if let Some(p) = utre_core::envvar::var_os("DB") {
        return Ok(PathBuf::from(p));
    }
    let data =
        dirs::data_dir().ok_or_else(|| std::io::Error::other("no platform data directory"))?;
    let (old, dir) = (data.join(LEGACY_DIR), data.join("utre"));
    match migrate_data_dir(&old, &dir, legacy_daemon_running) {
        Ok(true) => eprintln!(
            "utre: moved the Tick Alt data {} -> {} (tt.db -> utre.db, tt.key -> utre.key)",
            old.display(),
            dir.display()
        ),
        Ok(false) => {}
        // Better to stop than to start on an empty database next to the real one.
        Err(e) => return Err(e),
    }
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("utre.db"))
}

/// The data directory's name before the rename to Utre.
const LEGACY_DIR: &str = "tick-alt";

/// Whether the pre-rename daemon (`tick-alt.service`) still answers on its old socket.
fn legacy_daemon_running() -> bool {
    #[cfg(unix)]
    {
        let sock = match dirs::runtime_dir() {
            Some(dir) => dir.join("tick-alt.sock"),
            None => match dirs::data_dir() {
                Some(d) => d.join(LEGACY_DIR).join("tick-alt.sock"),
                None => return false,
            },
        };
        std::os::unix::net::UnixStream::connect(sock).is_ok()
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Move the Tick Alt data directory `old` to `new` when `new` doesn't exist yet, and rename
/// its `tt.db`, `tt.db-wal`, `tt.db-shm` and `tt.key` to `utre.*`. Everything else (`desk.json`,
/// Google tokens, `backups/`) keeps its name. Returns whether anything moved. Refuses, with an
/// error, while the old daemon still runs (it would keep writing into the moved files).
pub fn migrate_data_dir(
    old: &Path,
    new: &Path,
    old_daemon_running: impl Fn() -> bool,
) -> std::io::Result<bool> {
    if new.exists() || !old.is_dir() {
        return Ok(false);
    }
    if old_daemon_running() {
        return Err(std::io::Error::other(
            "the old Tick Alt daemon (tick-alt.service) is still running: stop it, \
             then start utre.service so the data can move to the Utre directory",
        ));
    }
    std::fs::rename(old, new)?;
    for (from, to) in [
        ("tt.db", "utre.db"),
        ("tt.db-wal", "utre.db-wal"),
        ("tt.db-shm", "utre.db-shm"),
        ("tt.key", "utre.key"),
    ] {
        let src = new.join(from);
        if src.exists() {
            std::fs::rename(src, new.join(to))?;
        }
    }
    Ok(true)
}

/// The Google OAuth client file (Google Cloud's "Desktop app" JSON): `$UTRE_GOOGLE_CLIENT`,
/// else `<config dir>/utre/google-client.json` (`~/.config/utre/` on Linux). It is never
/// kept in the repo.
pub fn google_client() -> PathBuf {
    if let Some(p) = utre_core::envvar::var_os("GOOGLE_CLIENT") {
        return PathBuf::from(p);
    }
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("utre")
        .join("google-client.json")
}

/// The daemon's socket: `$UTRE_SOCKET`, else `$XDG_RUNTIME_DIR/utre.sock`, else
/// `utre.sock` in the data dir (macOS has no runtime dir).
pub fn socket() -> PathBuf {
    if let Some(p) = utre_core::envvar::var_os("SOCKET") {
        return PathBuf::from(p);
    }
    match dirs::runtime_dir() {
        Some(dir) => dir.join("utre.sock"),
        None => dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("utre")
            .join("utre.sock"),
    }
}

/// Two paths name the same database file (both canonicalised; a missing file is compared as given).
pub fn same_file(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("utre-mig-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn moves_the_old_directory_and_renames_the_db_files() {
        let root = scratch("move");
        let (old, new) = (root.join("tick-alt"), root.join("utre"));
        std::fs::create_dir_all(old.join("backups")).unwrap();
        for f in [
            "tt.db",
            "tt.db-wal",
            "tt.key",
            "desk.json",
            "backups/tt-v6.db",
        ] {
            std::fs::write(old.join(f), f).unwrap();
        }
        assert!(migrate_data_dir(&old, &new, || false).unwrap());
        assert!(!old.exists());
        for (f, content) in [
            ("utre.db", "tt.db"),
            ("utre.db-wal", "tt.db-wal"),
            ("utre.key", "tt.key"),
            ("desk.json", "desk.json"),
            ("backups/tt-v6.db", "backups/tt-v6.db"),
        ] {
            assert_eq!(
                std::fs::read_to_string(new.join(f)).unwrap(),
                content,
                "{f}"
            );
        }
        assert!(!new.join("tt.db").exists());
        // A second start does nothing.
        assert!(!migrate_data_dir(&old, &new, || false).unwrap());
    }

    #[test]
    fn leaves_an_existing_new_directory_alone() {
        let root = scratch("keep");
        let (old, new) = (root.join("tick-alt"), root.join("utre"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("tt.db"), "old").unwrap();
        std::fs::write(new.join("utre.db"), "new").unwrap();
        assert!(!migrate_data_dir(&old, &new, || false).unwrap());
        assert_eq!(std::fs::read_to_string(old.join("tt.db")).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(new.join("utre.db")).unwrap(), "new");
    }

    #[test]
    fn nothing_to_migrate_on_a_fresh_machine() {
        let root = scratch("fresh");
        assert!(!migrate_data_dir(&root.join("tick-alt"), &root.join("utre"), || false).unwrap());
        assert!(!root.join("utre").exists());
    }

    #[test]
    fn refuses_while_the_old_daemon_runs() {
        let root = scratch("busy");
        let (old, new) = (root.join("tick-alt"), root.join("utre"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("tt.db"), "old").unwrap();
        assert!(migrate_data_dir(&old, &new, || true).is_err());
        assert!(old.join("tt.db").exists() && !new.exists());
    }
}
