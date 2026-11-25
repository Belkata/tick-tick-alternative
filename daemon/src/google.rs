//! The daemon's side of Google Calendar (plan/06, plan/19): sign-in, and fetching the
//! user's events into the local cache.
//!
//! `utre-google` does the network work. This file owns the state the app and `tt` ask about
//! (`GoogleStatus`), starts the browser sign-in, and polls every 5 minutes while signed in.
//! Events land in the cache tables through the daemon's own connection to the database file;
//! the DB thread notices those commits and tells subscribers, like any outside write.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use utre_api::GoogleStatus;
use utre_core::Store;
use utre_google::{ClientSecrets, FileTokenStore, Session, sync_calendars};

/// How often events are fetched while signed in (plan/06: desktop polls every 5 minutes).
const POLL: Duration = Duration::from_secs(5 * 60);
/// A short wait after startup, so the first fetch doesn't compete with startup work.
const FIRST_POLL: Duration = Duration::from_secs(20);
/// How long a sign-in request waits for the browser URL to be ready.
const URL_WAIT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct State {
    signing_in: bool,
    last_sync_ms: Option<i64>,
    last_error: Option<String>,
}

pub struct GoogleService {
    /// `None` when the OAuth client file is missing: Google is then simply unavailable.
    session: Option<Arc<Session>>,
    client_file: PathBuf,
    db_file: PathBuf,
    state: Mutex<State>,
    /// One fetch at a time (the poll, `tt google sync` and the app's button can overlap).
    busy: tokio::sync::Mutex<()>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl GoogleService {
    /// Tokens live next to the database (`google-token.json`), so a test database never
    /// touches the real sign-in. The OAuth client file is `utre_api::paths::google_client()`.
    pub fn new(db_file: PathBuf) -> Arc<GoogleService> {
        let client_file = utre_api::paths::google_client();
        let session = match ClientSecrets::load(&client_file) {
            Ok(secrets) => {
                let tokens = FileTokenStore::new(db_file.with_file_name("google-token.json"));
                Some(Arc::new(Session::new(secrets, Arc::new(tokens))))
            }
            Err(_) => None,
        };
        Arc::new(GoogleService {
            session,
            client_file,
            db_file,
            state: Mutex::new(State::default()),
            busy: tokio::sync::Mutex::new(()),
        })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn session(&self) -> Result<&Arc<Session>, String> {
        self.session.as_ref().ok_or_else(|| {
            format!(
                "Google isn't set up: put the OAuth client file at {}",
                self.client_file.display()
            )
        })
    }

    pub fn status(&self) -> GoogleStatus {
        let state = self.state();
        GoogleStatus {
            configured: self.session.is_some(),
            signed_in: self
                .session
                .as_ref()
                .is_some_and(|s| s.is_signed_in().unwrap_or(false)),
            signing_in: state.signing_in,
            last_sync_ms: state.last_sync_ms,
            last_error: state.last_error.clone(),
        }
    }

    /// Opens the browser sign-in and answers with its URL. The sign-in finishes in the
    /// background (up to 5 minutes), then fetches events.
    pub async fn sign_in(self: &Arc<Self>) -> Result<String, String> {
        let session = self.session()?.clone();
        {
            let mut state = self.state();
            if state.signing_in {
                return Err("a sign-in is already waiting in the browser".into());
            }
            state.signing_in = true;
            state.last_error = None;
        }
        let (url_tx, url_rx) = tokio::sync::oneshot::channel();
        let this = self.clone();
        tokio::spawn(async move {
            let result = session
                .sign_in(|url| {
                    open_url(url);
                    let _ = url_tx.send(url.to_string());
                })
                .await;
            this.state().signing_in = false;
            match result {
                Ok(()) => {
                    eprintln!("utre-daemon: signed in to Google");
                    let _ = this.sync_now().await;
                }
                Err(e) => {
                    eprintln!("utre-daemon: Google sign-in: {e}");
                    this.state().last_error = Some(e.to_string());
                }
            }
        });
        match tokio::time::timeout(URL_WAIT, url_rx).await {
            Ok(Ok(url)) => Ok(url),
            _ => Err(self
                .state()
                .last_error
                .clone()
                .unwrap_or_else(|| "the sign-in didn't start".into())),
        }
    }

    /// Forgets the tokens and everything cached from Google.
    pub async fn sign_out(&self) -> Result<(), String> {
        self.session()?.sign_out().map_err(|e| e.to_string())?;
        Store::open(&self.db_file)
            .and_then(|store| store.gcal_forget())
            .map_err(|e| e.to_string())?;
        *self.state() = State::default();
        Ok(())
    }

    /// Fetches the calendar list and the events of every selected calendar.
    pub async fn sync_now(&self) -> Result<(), String> {
        let session = self.session()?;
        if !session.is_signed_in().map_err(|e| e.to_string())? {
            return Err("not signed in to Google".into());
        }
        let _one_at_a_time = self.busy.lock().await;
        let mut store = Store::open(&self.db_file).map_err(|e| e.to_string())?;
        let result = sync_calendars(session, &mut store, now_ms()).await;
        let mut state = self.state();
        match result {
            Ok(()) => {
                state.last_sync_ms = Some(now_ms());
                state.last_error = None;
                Ok(())
            }
            Err(e) => {
                state.last_error = Some(e.to_string());
                Err(e.to_string())
            }
        }
    }

    /// Fetch every 5 minutes while signed in.
    pub fn spawn_poll(self: &Arc<Self>) {
        if self.session.is_none() {
            return;
        }
        let this = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(FIRST_POLL).await;
            loop {
                if this.status().signed_in
                    && let Err(e) = this.sync_now().await
                {
                    eprintln!("utre-daemon: Google: {e}");
                }
                tokio::time::sleep(POLL).await;
            }
        });
    }
}

/// Asks the desktop to open `url` in the user's browser. If it can't, the caller still has
/// the URL to show.
fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let opener = "/usr/bin/open";
    #[cfg(not(target_os = "macos"))]
    let opener = "xdg-open";
    match std::process::Command::new(opener).arg(url).spawn() {
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(e) => eprintln!("utre-daemon: could not open the browser ({e}); open this link:\n{url}"),
    }
}
