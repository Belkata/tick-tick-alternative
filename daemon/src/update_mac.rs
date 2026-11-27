//! macOS: keeps the whole `Utre.app` up to date from GitHub releases (the daemon lives inside
//! the bundle it replaces and runs all day under the LaunchAgent).
//!
//! Every step is logged. Check 2 minutes after start and every 6 hours, or from the menu's
//! "Check for Updates". The zip is downloaded next to the app (same volume, so the renames are
//! atomic), its signature is verified BEFORE anything is extracted, and the new bundle's
//! Info.plist must name the right app and version. No sudo: if the folder isn't writable, the
//! menu says so. Afterwards launchd is asked to restart the daemon from the new bundle.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use semver::Version;
use tokio::sync::mpsc;

use crate::Cmd;
use crate::update::{self, APP_BUNDLE_ID, Release};

const FIRST_CHECK: Duration = Duration::from_secs(120);
const EVERY: Duration = Duration::from_secs(6 * 3600);
/// A release zip is about 120 MB; refuse anything absurd.
const MAX_DOWNLOAD: u64 = 600 * 1024 * 1024;
const LAUNCHD_LABEL: &str = "dev.utre.daemon";

/// A handle for "Check for Updates".
pub struct Updater {
    trigger: mpsc::UnboundedSender<()>,
}

impl Updater {
    /// Starts the schedule if the daemon runs inside the installed app and updates are on.
    pub fn start(tx: mpsc::UnboundedSender<Cmd>) -> Option<Updater> {
        if utre_core::envvar::var("AUTOUPDATE").as_deref() == Some("0") {
            eprintln!("utre-daemon: updates: off (UTRE_AUTOUPDATE=0)");
            return None;
        }
        let (bundle, installed) = match find_bundle() {
            Ok(Some(found)) => found,
            Ok(None) => return None, // not running from the app: a dev build never updates
            Err(e) => {
                eprintln!("utre-daemon: updates: {e:#}");
                return None;
            }
        };
        eprintln!(
            "utre-daemon: updates: {} is version {installed}",
            bundle.display()
        );
        let status = move |line: String| {
            let _ = tx.send(Cmd::UpdateStatus(line));
        };
        status(format!("Version {installed}"));
        let (trigger, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut wait = FIRST_CHECK;
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    Some(()) = rx.recv() => {}
                }
                wait = EVERY;
                status("Checking for updates…".into());
                match check(&bundle, &installed, &status).await {
                    Ok(None) => {
                        eprintln!("utre-daemon: updates: up to date ({installed})");
                        status(format!("Up to date ({installed})"));
                    }
                    Ok(Some(version)) => {
                        // The daemon is being restarted; this line is for the old process.
                        status(format!("Updated to {version}"));
                        return;
                    }
                    Err(e) => {
                        eprintln!("utre-daemon: updates: failed: {e:#}");
                        status(format!("Update failed: {e:#}"));
                    }
                }
            }
        });
        Some(Updater { trigger })
    }

    pub fn check_now(&self) {
        let _ = self.trigger.send(());
    }
}

/// The outer `*.app` (bundle id `dev.utre.utre`) this executable sits in, and its version.
fn find_bundle() -> Result<Option<(PathBuf, Version)>> {
    let exe = std::env::current_exe()?.canonicalize()?;
    for dir in exe.ancestors().skip(1) {
        if dir.extension().is_none_or(|e| e != "app") {
            continue;
        }
        let Ok(plist) = std::fs::read(dir.join("Contents/Info.plist")) else {
            continue;
        };
        if let Ok((id, version)) = update::parse_info_plist(&plist)
            && id == APP_BUNDLE_ID
        {
            return Ok(Some((dir.to_path_buf(), version)));
        }
    }
    Ok(None)
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent("utre-daemon")
        .timeout(Duration::from_secs(900))
        .build()?)
}

/// Looks for a newer release and installs it. `Some(version)` when the app was replaced.
async fn check(
    bundle: &Path,
    installed: &Version,
    status: &impl Fn(String),
) -> Result<Option<Version>> {
    let http = client()?;
    let json = http
        .get(update::RELEASES_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()
        .context("listing releases")?
        .text()
        .await?;
    let Some(release) = update::newer_than(update::pick_latest(&json)?, installed) else {
        return Ok(None);
    };
    let version = release.version.clone();
    eprintln!("utre-daemon: updates: {version} is available (installed {installed})");
    status(format!("Updating to {version}…"));
    install(&http, bundle, &release).await?;
    Ok(Some(version))
}

async fn download(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let resp = http.get(url).send().await?.error_for_status()?;
    if resp.content_length().is_some_and(|n| n > MAX_DOWNLOAD) {
        bail!("{url} is too large");
    }
    let bytes = resp.bytes().await?;
    if bytes.len() as u64 > MAX_DOWNLOAD {
        bail!("{url} is too large");
    }
    Ok(bytes.to_vec())
}

async fn install(http: &reqwest::Client, bundle: &Path, release: &Release) -> Result<()> {
    let version = &release.version;
    let parent = bundle.parent().context("the app has no parent folder")?;
    let name = bundle.file_name().context("the app has no name")?;
    // Next to the app, so the final renames stay on one volume.
    let work = parent.join(format!(".utre-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    if let Err(e) = std::fs::create_dir(&work) {
        eprintln!(
            "utre-daemon: updates: {} is not writable: {e}",
            parent.display()
        );
        bail!("can't write to {} ({e}); update by hand", parent.display());
    }
    // The old bundle is moved here by the swap and deleted only once everything worked.
    let backup = parent.join(format!(
        ".{}.old-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let result = install_in(http, bundle, release, &work, name, &backup).await;
    let _ = std::fs::remove_dir_all(&work);
    result?;
    eprintln!("utre-daemon: updates: {version} installed");
    notify_installed(version).await;
    if let Err(e) = std::fs::remove_dir_all(&backup) {
        eprintln!(
            "utre-daemon: updates: could not delete {}: {e}",
            backup.display()
        );
    }
    restart_daemon().await;
    Ok(())
}

async fn install_in(
    http: &reqwest::Client,
    bundle: &Path,
    release: &Release,
    work: &Path,
    name: &std::ffi::OsStr,
    backup: &Path,
) -> Result<()> {
    let version = &release.version;
    eprintln!("utre-daemon: updates: downloading {}", release.zip_url);
    let zip = download(http, &release.zip_url)
        .await
        .context("downloading the app")?;
    let sig = download(http, &release.sig_url)
        .await
        .context("downloading the signature")?;
    eprintln!(
        "utre-daemon: updates: verifying the signature ({} bytes)",
        zip.len()
    );
    let zip = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        update::verify_signature(&zip, &sig)?;
        Ok(zip)
    })
    .await??;
    let zip_path = work.join("update.zip");
    std::fs::write(&zip_path, &zip).context("saving the download")?;
    drop(zip);

    eprintln!("utre-daemon: updates: extracting");
    let out = work.join("new");
    let status = tokio::process::Command::new("/usr/bin/ditto")
        .args(["-x", "-k"])
        .arg(&zip_path)
        .arg(&out)
        .status()
        .await
        .context("running ditto")?;
    if !status.success() {
        bail!("ditto failed ({status})");
    }
    let new_app = out.join(name);
    let plist = std::fs::read(new_app.join("Contents/Info.plist"))
        .with_context(|| format!("the download has no {}", name.to_string_lossy()))?;
    let (id, found) = update::parse_info_plist(&plist)?;
    if id != APP_BUNDLE_ID || &found != version {
        bail!("the download is {id} {found}, expected {APP_BUNDLE_ID} {version}");
    }

    eprintln!("utre-daemon: updates: swapping in {version}");
    update::swap_dirs(bundle, &new_app, backup)?;
    Ok(())
}

async fn notify_installed(version: &Version) {
    use mac_usernotifications::Notification;
    if !crate::NOTIFICATIONS.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let sent = Notification::new()
        .title(format!("Utre {version} installed"))
        .message("Reopen the app to use it.")
        .send()
        .await;
    if let Err(e) = sent {
        eprintln!("utre-daemon: updates: notification failed: {e}");
    }
    // Let macOS take the banner before the process goes away.
    tokio::time::sleep(Duration::from_secs(2)).await;
}

/// Asks launchd to restart this daemon (the LaunchAgent's path now points into the new
/// bundle). Without launchd (started by hand) the new version runs from the next start.
async fn restart_daemon() {
    let uid = tokio::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .await
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());
    let Some(uid) = uid else {
        return eprintln!("utre-daemon: updates: no uid; restart the daemon by hand");
    };
    eprintln!("utre-daemon: updates: restarting through launchd");
    let status = tokio::process::Command::new("/bin/launchctl")
        .args(["kickstart", "-k", &format!("gui/{uid}/{LAUNCHD_LABEL}")])
        .status()
        .await;
    // On success launchd stops this process; getting here means it didn't.
    eprintln!(
        "utre-daemon: updates: kickstart returned {status:?}; the new version runs from the next start"
    );
}
