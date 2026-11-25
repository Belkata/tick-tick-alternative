//! Google sign-in and Calendar access for Utre (plan/06 Google Calendar, plan/19).
//!
//! No backend: the desktop flow is OAuth 2.0 authorization code with PKCE and a loopback
//! redirect. This crate is only the network side. The event cache and the mappings live in
//! `tt-core`'s `gcal` module.
//!
//! - [`oauth`]: client secrets, PKCE, the loopback listener, the token endpoint.
//! - [`tokens`]: where a device keeps its tokens (never synced between devices).
//! - [`session`]: a signed-in session that keeps its access token fresh.
//! - [`calendar`]: listing calendars and the incremental events fetch.

pub mod calendar;
pub mod oauth;
pub mod session;
pub mod tokens;

#[cfg(test)]
mod testutil;

pub use calendar::{EventSync, sync_calendars};
pub use oauth::{ClientSecrets, Endpoints, SCOPES};
pub use session::Session;
pub use tokens::{FileTokenStore, TokenStore, Tokens};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Core(#[from] tt_core::Error),
    /// No usable tokens: the user never signed in, or Google revoked the access.
    #[error("not signed in to Google")]
    SignedOut,
    /// The user refused access in the browser, or the redirect didn't match this attempt.
    #[error("sign-in failed: {0}")]
    SignIn(String),
    /// The sign-in page wasn't completed in time.
    #[error("sign-in timed out")]
    Timeout,
    /// A sync token is no longer valid (HTTP 410). Handled inside the fetch.
    #[error("sync token expired")]
    Gone,
    #[error("Google answered {status}: {body}")]
    Api { status: u16, body: String },
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
