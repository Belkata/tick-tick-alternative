//! Where the database and the daemon's socket live.

use std::path::{Path, PathBuf};

/// The desktop database: `$TT_DB`, else `<data dir>/tick-alt/tt.db`
/// (`~/.local/share/tick-alt/tt.db` on Linux, `~/Library/Application Support/tick-alt/tt.db` on macOS).
pub fn default_db() -> std::io::Result<PathBuf> {
    if let Some(p) = std::env::var_os("TT_DB") {
        return Ok(PathBuf::from(p));
    }
    let dir = dirs::data_dir()
        .ok_or_else(|| std::io::Error::other("no platform data directory"))?
        .join("tick-alt");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("tt.db"))
}

/// The Google OAuth client file (Google Cloud's "Desktop app" JSON): `$TT_GOOGLE_CLIENT`,
/// else `<config dir>/utre/google-client.json` (`~/.config/utre/` on Linux). It is never
/// kept in the repo.
pub fn google_client() -> PathBuf {
    if let Some(p) = std::env::var_os("TT_GOOGLE_CLIENT") {
        return PathBuf::from(p);
    }
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("utre")
        .join("google-client.json")
}

/// The daemon's socket: `$TT_SOCKET`, else `$XDG_RUNTIME_DIR/tick-alt.sock`, else
/// `tick-alt.sock` in the data dir (macOS has no runtime dir).
pub fn socket() -> PathBuf {
    if let Some(p) = std::env::var_os("TT_SOCKET") {
        return PathBuf::from(p);
    }
    match dirs::runtime_dir() {
        Some(dir) => dir.join("tick-alt.sock"),
        None => dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("tick-alt")
            .join("tick-alt.sock"),
    }
}

/// Two paths name the same database file (both canonicalised; a missing file is compared as given).
pub fn same_file(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}
