//! A Google session: sign in once, then ask for a valid access token whenever one is needed.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use url::Url;

use crate::oauth::{self, ClientSecrets, Endpoints, Loopback, Pkce};
use crate::tokens::TokenStore;
use crate::{Error, Result, now_ms};

/// Refresh this long before the access token actually expires.
const EXPIRY_MARGIN_MS: i64 = 60_000;
/// How long the user has to finish the sign-in page.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Tries per request when Google says "slow down" (429, rate-limit 403, 5xx).
const ATTEMPTS: u32 = 5;

pub struct Session {
    pub(crate) http: reqwest::Client,
    pub(crate) endpoints: Endpoints,
    secrets: ClientSecrets,
    store: Arc<dyn TokenStore>,
    /// One refresh at a time, so concurrent requests don't each spend the refresh token.
    refreshing: tokio::sync::Mutex<()>,
    /// First back-off delay; doubles on each retry. Tests shorten it.
    pub retry_base: Duration,
}

impl Session {
    pub fn new(secrets: ClientSecrets, store: Arc<dyn TokenStore>) -> Session {
        Session::with_endpoints(secrets, store, Endpoints::default())
    }

    pub fn with_endpoints(
        secrets: ClientSecrets,
        store: Arc<dyn TokenStore>,
        endpoints: Endpoints,
    ) -> Session {
        Session {
            http: reqwest::Client::new(),
            endpoints,
            secrets,
            store,
            refreshing: tokio::sync::Mutex::new(()),
            retry_base: Duration::from_secs(1),
        }
    }

    pub fn is_signed_in(&self) -> Result<bool> {
        Ok(self.store.load()?.is_some())
    }

    /// Forgets this device's tokens. (Revoking at Google is done in the user's account.)
    pub fn sign_out(&self) -> Result<()> {
        self.store.clear()
    }

    /// Runs the browser sign-in. `open_browser` gets the URL to show the user (the caller
    /// decides how: `xdg-open`, `open`, a QR code). Returns once the tokens are saved.
    pub async fn sign_in(&self, open_browser: impl FnOnce(&str)) -> Result<()> {
        let loopback = Loopback::bind().await?;
        let redirect_uri = loopback.redirect_uri()?;
        let state = oauth::random_state()?;
        let pkce = Pkce::generate()?;
        let url = oauth::auth_url(
            &self.endpoints,
            &self.secrets,
            &redirect_uri,
            &state,
            &pkce.challenge,
        )?;
        open_browser(&url);
        let code = tokio::time::timeout(SIGN_IN_TIMEOUT, loopback.wait_for_code(&state))
            .await
            .map_err(|_| Error::Timeout)??;
        let tokens = oauth::exchange_code(
            &self.http,
            &self.endpoints,
            &self.secrets,
            &code,
            &pkce.verifier,
            &redirect_uri,
        )
        .await?;
        self.store.save(&tokens)
    }

    /// A valid access token, refreshed if it is about to expire (or `force`d after a 401).
    /// Clears the stored tokens and answers [`Error::SignedOut`] when Google refuses the
    /// refresh token.
    pub async fn access_token(&self, force: bool) -> Result<String> {
        let _one_at_a_time = self.refreshing.lock().await;
        let Some(tokens) = self.store.load()? else {
            return Err(Error::SignedOut);
        };
        if !force && tokens.expires_at - EXPIRY_MARGIN_MS > now_ms() {
            return Ok(tokens.access_token);
        }
        match oauth::refresh(
            &self.http,
            &self.endpoints,
            &self.secrets,
            &tokens.refresh_token,
        )
        .await
        {
            Ok(fresh) => {
                self.store.save(&fresh)?;
                Ok(fresh.access_token)
            }
            Err(Error::SignedOut) => {
                self.store.clear()?;
                Err(Error::SignedOut)
            }
            Err(e) => Err(e),
        }
    }

    /// `GET {api_base}/{segments...}?{query}` with the bearer token. Retries a 401 once with
    /// a forced refresh, and rate-limit or server errors with exponential back-off.
    /// A 410 becomes [`Error::Gone`].
    pub(crate) async fn get_json(
        &self,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> Result<Value> {
        let mut url = Url::parse(&self.endpoints.api_base)
            .map_err(|e| Error::SignIn(format!("bad API base: {e}")))?;
        url.path_segments_mut()
            .map_err(|_| Error::SignIn("bad API base".into()))?
            .pop_if_empty()
            .extend(segments);
        let mut force = false;
        let mut delay = self.retry_base;
        for attempt in 1..=ATTEMPTS {
            let token = self.access_token(force).await?;
            let resp = self
                .http
                .get(url.clone())
                .query(query)
                .bearer_auth(token)
                .send()
                .await?;
            let status = resp.status().as_u16();
            let body = resp.text().await?;
            match status {
                200..=299 => return Ok(serde_json::from_str(&body)?),
                401 if !force => force = true,
                410 => return Err(Error::Gone),
                429 | 500..=599 if attempt < ATTEMPTS => {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
                403 if attempt < ATTEMPTS && body.contains("ateLimitExceeded") => {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
                _ => return Err(Error::Api { status, body }),
            }
        }
        unreachable!("the last attempt always returns")
    }
}
