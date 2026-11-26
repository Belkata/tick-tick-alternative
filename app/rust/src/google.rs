//! Google Calendar on the phone (M5b slice 5, plan/19): Android has no utre-daemon, so the
//! app fetches events itself, in this process, from the Flutter side (`TaskDb`), from the
//! 15-minute poll below and from the WorkManager job (`kotlin::background_sync`).
//!
//! Sign-in is Kotlin's job (Google Identity `AuthorizationClient`, `google/GoogleAuth.kt`):
//! it needs an Activity and the consent screen. Kotlin hands over the first access token
//! ([`PhoneGoogle::signed_in`]); after that this module asks Kotlin for a fresh token
//! whenever the cached one runs out ([`utre_google::AndroidAuthorizer`]). Outside `api/` so
//! flutter_rust_bridge doesn't scan it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use utre_api::GoogleStatus;
use utre_core::Store;
use utre_google::{AccessTokenProvider, AndroidAuthorizer, Endpoints, FileTokenStore, Session, sync_calendars};

use crate::changes::Changes;

/// How often events are fetched while signed in and the app process is alive (plan/06).
const POLL: Duration = Duration::from_secs(15 * 60);
/// After startup, so the first fetch doesn't compete with the first frame and the sync node.
const FIRST_POLL: Duration = Duration::from_secs(30);

/// What Kotlin answers when asked for an access token (no UI).
#[derive(uniffi::Enum)]
pub enum TokenAnswer {
    Token { value: String },
    /// The user has to consent again: never signed in, or revoked the access.
    ConsentNeeded,
    /// A passing problem (no network, Play services busy). Stay signed in.
    Unavailable { message: String },
}

/// Implemented in Kotlin: Google Play services' `AuthorizationClient`, silently.
#[uniffi::export(with_foreign)]
pub trait AccessTokenSource: Send + Sync {
    /// `stale` is a token Google just refused; drop it before asking for a new one.
    fn access_token(&self, stale: Option<String>) -> TokenAnswer;
}

struct Provider(Arc<dyn AccessTokenSource>);

impl AccessTokenProvider for Provider {
    fn access_token(&self, stale: Option<String>) -> Result<Option<String>, String> {
        match self.0.access_token(stale) {
            TokenAnswer::Token { value } => Ok(Some(value)),
            TokenAnswer::ConsentNeeded => Ok(None),
            TokenAnswer::Unavailable { message } => Err(message),
        }
    }
}

#[derive(Default)]
struct State {
    last_sync_ms: Option<i64>,
    last_error: Option<String>,
}

pub struct PhoneGoogle {
    session: Arc<Session>,
    auth: Arc<AndroidAuthorizer>,
    /// The build knows its Android OAuth client (see `app/android/app/build.gradle.kts`).
    configured: bool,
    db_file: PathBuf,
    state: Mutex<State>,
    /// One fetch at a time (the poll, the job and the page's button can overlap).
    busy: tokio::sync::Mutex<()>,
    changes: Mutex<Option<Arc<Changes>>>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Google's calls are few and short; a small runtime of our own keeps this independent of
/// whether the sync node has started.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("utre-google")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

fn slot() -> &'static Mutex<Option<Arc<PhoneGoogle>>> {
    static SLOT: OnceLock<Mutex<Option<Arc<PhoneGoogle>>>> = OnceLock::new();
    SLOT.get_or_init(Mutex::default)
}

/// The phone's Google service, once Kotlin has installed it. `None` on desktop and in tests.
pub fn get() -> Option<Arc<PhoneGoogle>> {
    slot().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Install the service (the first call wins; later ones return it). Tokens live next to the
/// database in `google-token.json`, in the app's private directory, owner-only.
pub fn install(db: &Path, source: Arc<dyn AccessTokenSource>, configured: bool) -> Arc<PhoneGoogle> {
    let mut slot = slot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(g) = &*slot {
        return g.clone();
    }
    let g = PhoneGoogle::new(
        db,
        Arc::new(Provider(source)),
        configured,
        Endpoints::default(),
    );
    g.spawn_poll();
    *slot = Some(g.clone());
    g
}

impl PhoneGoogle {
    pub fn new(
        db: &Path,
        provider: Arc<dyn AccessTokenProvider>,
        configured: bool,
        endpoints: Endpoints,
    ) -> Arc<PhoneGoogle> {
        let tokens = Arc::new(FileTokenStore::new(db.with_file_name("google-token.json")));
        let auth = Arc::new(AndroidAuthorizer::new(provider, tokens));
        let session = Arc::new(Session::with_authorizer(auth.clone(), endpoints));
        Arc::new(PhoneGoogle {
            session,
            auth,
            configured,
            db_file: db.to_path_buf(),
            state: Mutex::default(),
            busy: tokio::sync::Mutex::new(()),
            changes: Mutex::default(),
        })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Tell the Flutter change stream when a fetch writes events.
    pub(crate) fn watch(&self, changes: Arc<Changes>) {
        *self.changes.lock().unwrap_or_else(|e| e.into_inner()) = Some(changes);
    }

    fn changed(&self) {
        if let Some(c) = &*self.changes.lock().unwrap_or_else(|e| e.into_inner()) {
            c.tick();
        }
    }

    pub fn status(&self) -> GoogleStatus {
        let state = self.state();
        GoogleStatus {
            configured: self.configured,
            signed_in: self.session.is_signed_in().unwrap_or(false),
            signing_in: false,
            last_sync_ms: state.last_sync_ms,
            last_error: state.last_error.clone(),
        }
    }

    /// The consent screen succeeded with this first token.
    pub fn signed_in(&self, access_token: String) -> Result<(), String> {
        self.auth.signed_in(access_token).map_err(|e| e.to_string())?;
        self.state().last_error = None;
        self.changed();
        Ok(())
    }

    /// Forgets the tokens and everything cached from Google.
    pub fn sign_out(&self) -> Result<(), String> {
        self.session.sign_out().map_err(|e| e.to_string())?;
        Store::open(&self.db_file)
            .and_then(|store| store.gcal_forget())
            .map_err(|e| e.to_string())?;
        *self.state() = State::default();
        self.changed();
        Ok(())
    }

    /// Fetches the calendar list and the events of every selected calendar.
    pub async fn sync_now(&self) -> Result<(), String> {
        if !self.configured {
            return Err("Google isn't set up in this build".into());
        }
        if !self.session.is_signed_in().map_err(|e| e.to_string())? {
            return Err("not signed in to Google".into());
        }
        let _one_at_a_time = self.busy.lock().await;
        let mut store = Store::open(&self.db_file).map_err(|e| e.to_string())?;
        let result = sync_calendars(&self.session, &mut store, now_ms()).await;
        let outcome = {
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
        };
        self.changed();
        outcome
    }

    /// A fetch unless one succeeded less than `max_age` ago (or nobody is signed in).
    /// Returns whether it fetched.
    pub async fn sync_if_stale(&self, max_age: Duration) -> Result<bool, String> {
        if !self.configured || !self.session.is_signed_in().unwrap_or(false) {
            return Ok(false);
        }
        let fresh = self
            .state()
            .last_sync_ms
            .is_some_and(|t| now_ms() - t < max_age.as_millis() as i64);
        if fresh {
            return Ok(false);
        }
        self.sync_now().await.map(|()| true)
    }

    /// Fetch every 15 minutes while signed in and the process is alive. (WorkManager covers
    /// the time the app is closed.)
    fn spawn_poll(self: &Arc<Self>) {
        let this = self.clone();
        runtime().spawn(async move {
            tokio::time::sleep(FIRST_POLL).await;
            loop {
                if let Err(e) = this.sync_if_stale(POLL - Duration::from_secs(60)).await {
                    eprintln!("utre: Google: {e}");
                }
                tokio::time::sleep(POLL).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utre_google::Error;

    struct Never;
    impl AccessTokenProvider for Never {
        fn access_token(&self, _: Option<String>) -> Result<Option<String>, String> {
            Ok(None)
        }
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("utre-phone-google-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn status_walks_from_not_set_up_to_signed_in_and_back() {
        let d = dir("status");
        let db = d.join("utre.db");
        Store::open(&db).unwrap();
        let unset = PhoneGoogle::new(&db, Arc::new(Never), false, Endpoints::default());
        let s = unset.status();
        assert!(!s.configured && !s.signed_in && !s.signing_in);
        assert!(runtime().block_on(unset.sync_now()).unwrap_err().contains("isn't set up"));

        let g = PhoneGoogle::new(&db, Arc::new(Never), true, Endpoints::default());
        assert!(g.status().configured && !g.status().signed_in);
        assert!(runtime().block_on(g.sync_now()).unwrap_err().contains("not signed in"));
        assert!(!runtime().block_on(g.sync_if_stale(Duration::ZERO)).unwrap());

        g.signed_in("t1".into()).unwrap();
        assert!(g.status().signed_in);
        let token_file = d.join("google-token.json");
        assert!(token_file.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&token_file).unwrap().permissions().mode() & 0o077, 0);
        }

        g.sign_out().unwrap();
        assert!(!g.status().signed_in);
        assert!(!token_file.exists());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_revoked_grant_signs_out_on_the_next_fetch() {
        let d = dir("revoked");
        let db = d.join("utre.db");
        Store::open(&db).unwrap();
        let g = PhoneGoogle::new(&db, Arc::new(Never), true, Endpoints::default());
        g.signed_in("t1".into()).unwrap();
        // A forced token refresh (what a 401 causes) reaches Kotlin, which says consent is gone.
        let err = runtime()
            .block_on(g.session.access_token(true))
            .unwrap_err();
        assert!(matches!(err, Error::SignedOut));
        assert!(!g.status().signed_in);
        let _ = std::fs::remove_dir_all(d);
    }
}
