//! Android's way to Google (plan/19 slice 5): no browser, no refresh token.
//!
//! The phone asks Google Play services for an access token through Google Identity's
//! `AuthorizationClient` (Kotlin, `google/GoogleAuth.kt`). Play services keeps the grant and
//! hands out a fresh one-hour token whenever the app asks again, silently, until the user
//! revokes the access. This module is the Rust side: [`AccessTokenProvider`] is the
//! callback Kotlin implements, and [`AndroidAuthorizer`] caches the token in a
//! [`TokenStore`] (an owner-only file in the app's private directory) so most calls never
//! cross into Kotlin.
//!
//! Nothing here is Android-specific, so it is tested on any host with a fake provider.

use std::sync::Arc;

use crate::session::{Authorizer, BoxFuture};
use crate::tokens::{TokenStore, Tokens};
use crate::{Error, Result, now_ms};

/// Play services doesn't say when a token ends; they last an hour. Trust one for less.
const TOKEN_LIFETIME_MS: i64 = 50 * 60 * 1000;
/// Ask for a new token this long before the cached one is assumed to end.
const EXPIRY_MARGIN_MS: i64 = 60_000;

/// What the platform side answers: an access token without showing any UI.
pub trait AccessTokenProvider: Send + Sync {
    /// `stale` is a token Google just refused (HTTP 401); the platform drops it first so
    /// it can't hand it out again. `Ok(None)` means the user has to consent again (never
    /// signed in, or revoked the access). `Err` is a passing problem such as no network.
    fn access_token(&self, stale: Option<String>) -> std::result::Result<Option<String>, String>;
}

pub struct AndroidAuthorizer {
    provider: Arc<dyn AccessTokenProvider>,
    store: Arc<dyn TokenStore>,
    /// One ask at a time, so a burst of requests shares one trip into Kotlin.
    asking: tokio::sync::Mutex<()>,
}

impl AndroidAuthorizer {
    pub fn new(provider: Arc<dyn AccessTokenProvider>, store: Arc<dyn TokenStore>) -> Self {
        AndroidAuthorizer {
            provider,
            store,
            asking: tokio::sync::Mutex::new(()),
        }
    }

    /// The user just agreed on the consent screen: remember the first token. This is what
    /// marks the device as signed in.
    pub fn signed_in(&self, access_token: String) -> Result<()> {
        self.store.save(&Tokens {
            access_token,
            refresh_token: String::new(),
            expires_at: now_ms() + TOKEN_LIFETIME_MS,
        })
    }

    async fn token(&self, force: bool) -> Result<String> {
        let _one_at_a_time = self.asking.lock().await;
        let Some(tokens) = self.store.load()? else {
            return Err(Error::SignedOut);
        };
        if !force && tokens.expires_at - EXPIRY_MARGIN_MS > now_ms() {
            return Ok(tokens.access_token);
        }
        let provider = self.provider.clone();
        let stale = force.then(|| tokens.access_token.clone());
        // The provider blocks on Play services, so it must not run on an async worker.
        let answer = tokio::task::spawn_blocking(move || provider.access_token(stale))
            .await
            .map_err(|e| Error::SignIn(format!("the token request stopped: {e}")))?;
        match answer {
            Ok(Some(access_token)) => {
                self.signed_in(access_token.clone())?;
                Ok(access_token)
            }
            Ok(None) => {
                self.store.clear()?;
                Err(Error::SignedOut)
            }
            Err(e) => Err(Error::SignIn(e)),
        }
    }
}

impl Authorizer for AndroidAuthorizer {
    fn is_signed_in(&self) -> Result<bool> {
        Ok(self.store.load()?.is_some())
    }

    fn sign_out(&self) -> Result<()> {
        self.store.clear()
    }

    fn access_token(&self, force: bool) -> BoxFuture<'_, Result<String>> {
        Box::pin(self.token(force))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::tokens::FileTokenStore;

    /// Answers from a script and remembers what it was asked.
    struct Fake {
        answers: Mutex<Vec<std::result::Result<Option<String>, String>>>,
        asked: Mutex<Vec<Option<String>>>,
    }

    impl Fake {
        fn new(answers: Vec<std::result::Result<Option<String>, String>>) -> Arc<Fake> {
            Arc::new(Fake {
                answers: Mutex::new(answers),
                asked: Mutex::default(),
            })
        }
    }

    impl AccessTokenProvider for Fake {
        fn access_token(&self, stale: Option<String>) -> std::result::Result<Option<String>, String> {
            self.asked.lock().unwrap().push(stale);
            self.answers.lock().unwrap().remove(0)
        }
    }

    fn store(name: &str) -> (Arc<FileTokenStore>, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("utre-android-{name}-{}", std::process::id()));
        (Arc::new(FileTokenStore::new(dir.join("google-token.json"))), dir)
    }

    #[tokio::test]
    async fn signed_out_until_the_consent_screen_returns() {
        let (store, dir) = store("out");
        let auth = AndroidAuthorizer::new(Fake::new(vec![]), store);
        assert!(!auth.is_signed_in().unwrap());
        assert!(matches!(auth.access_token(false).await, Err(Error::SignedOut)));
        auth.signed_in("t1".into()).unwrap();
        assert!(auth.is_signed_in().unwrap());
        auth.sign_out().unwrap();
        assert!(!auth.is_signed_in().unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn serves_the_cached_token_without_asking_the_platform() {
        let (store, dir) = store("cache");
        let fake = Fake::new(vec![]);
        let auth = AndroidAuthorizer::new(fake.clone(), store);
        auth.signed_in("t1".into()).unwrap();
        assert_eq!(auth.access_token(false).await.unwrap(), "t1");
        assert_eq!(auth.access_token(false).await.unwrap(), "t1");
        assert!(fake.asked.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_forced_refresh_hands_back_the_refused_token() {
        let (store, dir) = store("force");
        let fake = Fake::new(vec![Ok(Some("t2".into()))]);
        let auth = AndroidAuthorizer::new(fake.clone(), store);
        auth.signed_in("t1".into()).unwrap();
        assert_eq!(auth.access_token(true).await.unwrap(), "t2");
        assert_eq!(*fake.asked.lock().unwrap(), vec![Some("t1".to_string())]);
        // The new token is cached.
        assert_eq!(auth.access_token(false).await.unwrap(), "t2");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_session_recovers_from_a_401_through_the_platform() {
        use crate::oauth::Endpoints;
        use crate::session::Session;
        use crate::testutil::{Route, serve};

        let (store, dir) = store("session");
        let fake_google = serve(vec![
            Route::new("/calendar/v3/users/me/calendarList", 401, "{}").times(1),
            Route::new("/calendar/v3/users/me/calendarList", 200, r#"{"items":[]}"#),
        ])
        .await;
        let provider = Fake::new(vec![Ok(Some("t2".into()))]);
        let auth = Arc::new(AndroidAuthorizer::new(provider.clone(), store.clone()));
        auth.signed_in("t1".into()).unwrap();
        let endpoints = Endpoints {
            auth_url: String::new(),
            token_url: String::new(),
            api_base: format!("{}/calendar/v3", fake_google.base),
        };
        let mut session = Session::with_authorizer(auth, endpoints);
        session.retry_base = std::time::Duration::from_millis(1);
        session.list_calendars().await.unwrap();
        assert_eq!(*provider.asked.lock().unwrap(), vec![Some("t1".to_string())]);
        assert_eq!(store.load().unwrap().unwrap().access_token, "t2");
        // The browser sign-in isn't offered on this device.
        assert!(session.sign_in(|_| {}).await.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn revoked_access_signs_the_device_out_but_a_passing_error_does_not() {
        let (store, dir) = store("revoked");
        let fake = Fake::new(vec![Err("no network".into()), Ok(None)]);
        let auth = AndroidAuthorizer::new(fake, store);
        auth.signed_in("t1".into()).unwrap();
        assert!(matches!(auth.access_token(true).await, Err(Error::SignIn(m)) if m == "no network"));
        assert!(auth.is_signed_in().unwrap());
        assert!(matches!(auth.access_token(true).await, Err(Error::SignedOut)));
        assert!(!auth.is_signed_in().unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }
}
