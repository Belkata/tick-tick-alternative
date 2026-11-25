//! OAuth 2.0 for a desktop app: authorization code + PKCE + a loopback redirect
//! (`http://127.0.0.1:<port>`). The user's browser does the sign-in; this module opens a
//! one-shot listener to catch the redirect and trades the code for tokens.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use crate::tokens::Tokens;
use crate::{Error, Result, now_ms};

/// Least privilege (plan/06): read events, and manage only the calendar Utre creates.
pub const SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/calendar.readonly",
    "https://www.googleapis.com/auth/calendar.app.created",
];

/// Google's URLs. Tests point these at a local fake.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub auth_url: String,
    pub token_url: String,
    /// Calendar API base, without a trailing slash.
    pub api_base: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            api_base: "https://www.googleapis.com/calendar/v3".into(),
        }
    }
}

/// The OAuth client downloaded from Google Cloud ("Desktop app"). A desktop client's secret
/// isn't confidential, but it stays out of the repo and is read from a file at run time.
#[derive(Clone, serde::Deserialize)]
pub struct ClientSecrets {
    pub client_id: String,
    pub client_secret: String,
}

impl std::fmt::Debug for ClientSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientSecrets")
            .field("client_id", &self.client_id)
            .finish_non_exhaustive()
    }
}

impl ClientSecrets {
    /// Reads Google's `client_secret_*.json` (`{"installed": {...}}`).
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        #[derive(serde::Deserialize)]
        struct File {
            installed: ClientSecrets,
        }
        let file: File = serde_json::from_slice(&std::fs::read(path)?)?;
        Ok(file.installed)
    }
}

/// A PKCE pair (RFC 7636): the random verifier and its S256 challenge.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn generate() -> Result<Pkce> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| Error::SignIn(format!("no randomness: {e}")))?;
        Ok(Pkce::from_verifier(URL_SAFE_NO_PAD.encode(bytes)))
    }

    pub fn from_verifier(verifier: String) -> Pkce {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Pkce {
            verifier,
            challenge,
        }
    }
}

pub(crate) fn random_state() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| Error::SignIn(format!("no randomness: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// The page the user opens in their browser. `access_type=offline` and `prompt=consent`
/// make Google hand out a refresh token.
pub fn auth_url(
    endpoints: &Endpoints,
    secrets: &ClientSecrets,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> Result<String> {
    let url = Url::parse_with_params(
        &endpoints.auth_url,
        &[
            ("client_id", secrets.client_id.as_str()),
            ("redirect_uri", redirect_uri),
            ("response_type", "code"),
            ("scope", &SCOPES.join(" ")),
            ("state", state),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ],
    )
    .map_err(|e| Error::SignIn(e.to_string()))?;
    Ok(url.into())
}

/// A one-shot HTTP listener on 127.0.0.1 that catches Google's redirect.
pub struct Loopback {
    listener: TcpListener,
}

impl Loopback {
    /// Binds a random free port on the loopback interface only.
    pub async fn bind() -> Result<Loopback> {
        Ok(Loopback {
            listener: TcpListener::bind(("127.0.0.1", 0)).await?,
        })
    }

    pub fn redirect_uri(&self) -> Result<String> {
        Ok(format!(
            "http://127.0.0.1:{}",
            self.listener.local_addr()?.port()
        ))
    }

    /// Waits for the redirect and returns the authorization code. Requests for other paths
    /// (a browser's favicon fetch) and ones with the wrong `state` are answered and ignored.
    pub async fn wait_for_code(self, state: &str) -> Result<String> {
        loop {
            let (mut stream, _) = self.listener.accept().await?;
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 8192 {
                let n = stream.read(&mut chunk).await?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let head = String::from_utf8_lossy(&buf);
            let target = head
                .lines()
                .next()
                .and_then(|l| l.split(' ').nth(1))
                .unwrap_or("/");
            let url = Url::parse(&format!("http://127.0.0.1{target}"))
                .map_err(|e| Error::SignIn(e.to_string()))?;
            let get = |key: &str| {
                url.query_pairs()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.into_owned())
            };
            if url.path() != "/" || (get("code").is_none() && get("error").is_none()) {
                respond(&mut stream, 404, "Not found").await;
                continue;
            }
            if get("state").as_deref() != Some(state) {
                respond(
                    &mut stream,
                    400,
                    "This sign-in link doesn't match. Start again in Utre.",
                )
                .await;
                continue;
            }
            if let Some(err) = get("error") {
                respond(
                    &mut stream,
                    200,
                    "Utre wasn't given access. You can close this tab.",
                )
                .await;
                return Err(Error::SignIn(err));
            }
            respond(
                &mut stream,
                200,
                "Signed in to Utre. You can close this tab.",
            )
            .await;
            return get("code").ok_or_else(|| Error::SignIn("no code".into()));
        }
    }
}

async fn respond(stream: &mut tokio::net::TcpStream, status: u16, message: &str) {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Utre</title>\
         <body style=\"font:18px system-ui;max-width:30em;margin:15vh auto;text-align:center\">\
         <h2>Utre</h2><p>{message}</p>"
    );
    let reply = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(reply.as_bytes()).await;
    let _ = stream.shutdown().await;
}

#[derive(serde::Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

async fn token_request(
    http: &reqwest::Client,
    endpoints: &Endpoints,
    form: &[(&str, &str)],
) -> Result<TokenReply> {
    let resp = http.post(&endpoints.token_url).form(form).send().await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        // `invalid_grant` on a refresh means the user revoked access (or the token expired).
        if body.contains("invalid_grant") {
            return Err(Error::SignedOut);
        }
        return Err(Error::Api {
            status: status.as_u16(),
            body,
        });
    }
    Ok(serde_json::from_str(&body)?)
}

/// Trades the authorization code for tokens.
pub async fn exchange_code(
    http: &reqwest::Client,
    endpoints: &Endpoints,
    secrets: &ClientSecrets,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<Tokens> {
    let reply = token_request(
        http,
        endpoints,
        &[
            ("client_id", &secrets.client_id),
            ("client_secret", &secrets.client_secret),
            ("code", code),
            ("code_verifier", verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
        ],
    )
    .await?;
    let refresh_token = reply
        .refresh_token
        .ok_or_else(|| Error::SignIn("Google sent no refresh token".into()))?;
    Ok(Tokens {
        access_token: reply.access_token,
        refresh_token,
        expires_at: now_ms() + reply.expires_in * 1000,
    })
}

/// Gets a fresh access token. Google keeps the same refresh token unless it sends a new one.
pub async fn refresh(
    http: &reqwest::Client,
    endpoints: &Endpoints,
    secrets: &ClientSecrets,
    refresh_token: &str,
) -> Result<Tokens> {
    let reply = token_request(
        http,
        endpoints,
        &[
            ("client_id", &secrets.client_id),
            ("client_secret", &secrets.client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ],
    )
    .await?;
    Ok(Tokens {
        access_token: reply.access_token,
        refresh_token: reply
            .refresh_token
            .unwrap_or_else(|| refresh_token.to_string()),
        expires_at: now_ms() + reply.expires_in * 1000,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_the_rfc_7636_example() {
        let p = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into());
        assert_eq!(p.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let a = Pkce::generate().unwrap();
        assert_eq!(a.verifier.len(), 43);
        assert_ne!(a.verifier, Pkce::generate().unwrap().verifier);
    }

    #[test]
    fn auth_url_carries_pkce_scopes_and_offline_access() {
        let secrets = ClientSecrets {
            client_id: "id".into(),
            client_secret: "s".into(),
        };
        let url = auth_url(
            &Endpoints::default(),
            &secrets,
            "http://127.0.0.1:9",
            "st",
            "ch",
        )
        .unwrap();
        let url = Url::parse(&url).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["code_challenge"], "ch");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["access_type"], "offline");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:9");
        assert!(
            q["scope"].contains("calendar.readonly") && q["scope"].contains("calendar.app.created")
        );
    }

    async fn browser_hits(uri: &str, path_and_query: &str) -> u16 {
        reqwest::get(format!("{uri}{path_and_query}"))
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    #[tokio::test]
    async fn loopback_catches_the_code_and_ignores_strays() {
        let lb = Loopback::bind().await.unwrap();
        let uri = lb.redirect_uri().unwrap();
        let waiter = tokio::spawn(lb.wait_for_code("good"));
        assert_eq!(browser_hits(&uri, "/favicon.ico").await, 404);
        assert_eq!(browser_hits(&uri, "/?code=evil&state=bad").await, 400);
        assert_eq!(browser_hits(&uri, "/?code=abc%2F123&state=good").await, 200);
        assert_eq!(waiter.await.unwrap().unwrap(), "abc/123");
    }

    #[tokio::test]
    async fn loopback_reports_a_refusal() {
        let lb = Loopback::bind().await.unwrap();
        let uri = lb.redirect_uri().unwrap();
        let waiter = tokio::spawn(lb.wait_for_code("s"));
        browser_hits(&uri, "/?error=access_denied&state=s").await;
        assert!(matches!(waiter.await.unwrap(), Err(Error::SignIn(e)) if e == "access_denied"));
    }

    #[test]
    fn secrets_debug_hides_the_secret() {
        let s = ClientSecrets {
            client_id: "id".into(),
            client_secret: "hunter2".into(),
        };
        assert!(!format!("{s:?}").contains("hunter2"));
    }
}
