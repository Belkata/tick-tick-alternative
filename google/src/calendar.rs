//! Listing the user's calendars and the incremental events fetch (plan/06 "Events in").
//!
//! The first run asks for a window (-30 to +365 days) with `singleEvents=true`. Later runs
//! send the saved `syncToken` and get only changes and deletions. A 410 means the token
//! is no longer valid: the cache is dropped and refilled with a full fetch.

use chrono::{DateTime, SecondsFormat};
use utre_core::{GcalCalendar, GcalEventUpdate, Store};

use crate::session::Session;
use crate::{Error, Result};

const DAY_MS: i64 = 86_400_000;
const PAST_DAYS: i64 = 30;
const FUTURE_DAYS: i64 = 365;

/// What one calendar's fetch means for the cache, ready for `Store::gcal_apply`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSync {
    /// A full fetch: the cached events of this calendar must be cleared first.
    pub full: bool,
    pub updates: Vec<GcalEventUpdate>,
    /// Save this for the next incremental fetch.
    pub next_sync_token: Option<String>,
}

fn rfc3339(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_default()
}

impl Session {
    /// The calendars the user can read, with Google's `selected` flag (the default for
    /// visibility, see `Store::gcal_set_calendars`).
    pub async fn list_calendars(&self) -> Result<Vec<GcalCalendar>> {
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let mut query = vec![("minAccessRole", "reader"), ("maxResults", "250")];
            if let Some(p) = &page {
                query.push(("pageToken", p));
            }
            let reply = self
                .get_json(&["users", "me", "calendarList"], &query)
                .await?;
            for item in reply["items"].as_array().into_iter().flatten() {
                let Some(id) = item["id"].as_str() else {
                    continue;
                };
                let name = item["summaryOverride"]
                    .as_str()
                    .or_else(|| item["summary"].as_str())
                    .unwrap_or(id);
                out.push(GcalCalendar {
                    id: id.to_string(),
                    summary: name.to_string(),
                    color: item["backgroundColor"].as_str().map(str::to_string),
                    // Google's own flag: the default until the user chooses on this device.
                    selected: item["selected"].as_bool().unwrap_or(false),
                    primary: item["primary"].as_bool().unwrap_or(false),
                    access_role: item["accessRole"].as_str().map(str::to_string),
                });
            }
            page = reply["nextPageToken"].as_str().map(str::to_string);
            if page.is_none() {
                return Ok(out);
            }
        }
    }

    /// Fetches one calendar's events: incrementally with `sync_token`, else (or when Google
    /// answers 410) as a full window around `now_ms`.
    pub async fn sync_events(
        &self,
        calendar_id: &str,
        sync_token: Option<&str>,
        now_ms: i64,
    ) -> Result<EventSync> {
        if let Some(token) = sync_token {
            match self.fetch_events(calendar_id, Some(token), now_ms).await {
                Err(Error::Gone) => {}
                other => return other,
            }
        }
        self.fetch_events(calendar_id, None, now_ms).await
    }

    async fn fetch_events(
        &self,
        calendar_id: &str,
        sync_token: Option<&str>,
        now_ms: i64,
    ) -> Result<EventSync> {
        let (time_min, time_max) = (
            rfc3339(now_ms - PAST_DAYS * DAY_MS),
            rfc3339(now_ms + FUTURE_DAYS * DAY_MS),
        );
        let mut updates = Vec::new();
        let mut page: Option<String> = None;
        loop {
            // With a syncToken Google forbids timeMin/timeMax; the rest must match the
            // first fetch, so `singleEvents` is always on.
            let mut query = vec![("singleEvents", "true"), ("maxResults", "250")];
            match sync_token {
                Some(t) => query.push(("syncToken", t)),
                None => {
                    query.push(("timeMin", &time_min));
                    query.push(("timeMax", &time_max));
                }
            }
            if let Some(p) = &page {
                query.push(("pageToken", p));
            }
            let reply = self
                .get_json(&["calendars", calendar_id, "events"], &query)
                .await?;
            for item in reply["items"].as_array().into_iter().flatten() {
                updates.push(GcalEventUpdate::parse(calendar_id, item)?);
            }
            page = reply["nextPageToken"].as_str().map(str::to_string);
            if page.is_none() {
                return Ok(EventSync {
                    full: sync_token.is_none(),
                    updates,
                    next_sync_token: reply["nextSyncToken"].as_str().map(str::to_string),
                });
            }
        }
    }
}

/// One full round: refresh the calendar list, then fetch every selected calendar and apply
/// the changes to the cache. A calendar that fails doesn't stop the others; the first
/// error is returned at the end.
pub async fn sync_calendars(session: &Session, store: &mut Store, now_ms: i64) -> Result<()> {
    let calendars = session.list_calendars().await?;
    store.gcal_set_calendars(&calendars)?;
    let mut first_error = None;
    for calendar in store.gcal_calendars()?.into_iter().filter(|c| c.selected) {
        let token = store.gcal_sync_token(&calendar.id)?;
        match session
            .sync_events(&calendar.id, token.as_deref(), now_ms)
            .await
        {
            Ok(sync) => store.gcal_apply(
                &calendar.id,
                sync.full,
                &sync.updates,
                sync.next_sync_token.as_deref(),
            )?,
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::oauth::{ClientSecrets, Endpoints};
    use crate::testutil::{Fake, Route, serve};
    use crate::tokens::{FileTokenStore, TokenStore, Tokens};

    const NOW: i64 = 1_796_000_000_000;
    const API: &str = "/calendar/v3";

    struct Rig {
        session: Session,
        fake: Fake,
        tokens: Arc<FileTokenStore>,
        dir: std::path::PathBuf,
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// A session signed in against a fake Google. `expires_at` is relative to the real clock.
    async fn rig(routes: Vec<Route>, expires_in_ms: i64) -> Rig {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "utre-google-rig-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let tokens = Arc::new(FileTokenStore::new(dir.join("token.json")));
        tokens
            .save(&Tokens {
                access_token: "old".into(),
                refresh_token: "refresh".into(),
                expires_at: crate::now_ms() + expires_in_ms,
            })
            .unwrap();
        let fake = serve(routes).await;
        let endpoints = Endpoints {
            auth_url: format!("{}/auth", fake.base),
            token_url: format!("{}/token", fake.base),
            api_base: format!("{}{API}", fake.base),
        };
        let secrets = ClientSecrets {
            client_id: "id".into(),
            client_secret: "secret".into(),
        };
        let mut session = Session::with_endpoints(secrets, tokens.clone(), endpoints);
        session.retry_base = Duration::from_millis(1);
        Rig {
            session,
            fake,
            tokens,
            dir,
        }
    }

    const HOUR: i64 = 3_600_000;

    fn requests(r: &Rig) -> Vec<String> {
        r.fake.requests.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn lists_calendars_across_pages() {
        let r = rig(
            vec![
                Route::new("/calendar/v3/users/me/calendarList", 200,
                    r##"{"items":[{"id":"b@x","summary":"Work","backgroundColor":"#16a765"}]}"##)
                    .with(&["pageToken=P2"]),
                Route::new("/calendar/v3/users/me/calendarList", 200,
                    r##"{"nextPageToken":"P2","items":[
                        {"id":"a@x","summary":"Raw","summaryOverride":"Me","backgroundColor":"#9fe1e7"}]}"##),
            ],
            HOUR,
        )
        .await;
        let cals = r.session.list_calendars().await.unwrap();
        assert_eq!(cals.len(), 2);
        assert_eq!(
            (cals[0].id.as_str(), cals[0].summary.as_str()),
            ("a@x", "Me")
        );
        assert_eq!(cals[1].color.as_deref(), Some("#16a765"));
        // Google's own flags come through (none were set here).
        assert!(cals.iter().all(|c| !c.selected && !c.primary));
        assert!(requests(&r)[0].contains("minAccessRole=reader"));
    }

    #[tokio::test]
    async fn first_fetch_pages_through_a_window_and_returns_the_token() {
        let r = rig(
            vec![
                Route::new(
                    "/calendar/v3/calendars/a%23b/events",
                    200,
                    r#"{"nextSyncToken":"SYNC1","items":[
                        {"id":"t","summary":"Call","start":{"dateTime":"2026-12-01T09:00:00Z"},
                         "htmlLink":"https://www.google.com/calendar/event?eid=dA","location":"Room 4",
                         "description":"Agenda","end":{"dateTime":"2026-12-01T09:30:00Z"}},
                        {"id":"gone","status":"cancelled"}]}"#,
                )
                .with(&["pageToken=E2"]),
                Route::new(
                    "/calendar/v3/calendars/a%23b/events",
                    200,
                    r#"{"nextPageToken":"E2","items":[
                        {"id":"d","summary":"Holiday","start":{"date":"2026-12-24"},
                         "end":{"date":"2026-12-25"}}]}"#,
                ),
            ],
            HOUR,
        )
        .await;
        let sync = r.session.sync_events("a#b", None, NOW).await.unwrap();
        assert!(sync.full);
        assert_eq!(sync.next_sync_token.as_deref(), Some("SYNC1"));
        assert_eq!(sync.updates.len(), 3);
        assert!(
            matches!(&sync.updates[1], GcalEventUpdate::Upsert(e) if e.summary == "Call"
            && e.html_link.as_deref() == Some("https://www.google.com/calendar/event?eid=dA")
            && e.location.as_deref() == Some("Room 4")
            && e.description.as_deref() == Some("Agenda"))
        );
        assert!(matches!(&sync.updates[2], GcalEventUpdate::Remove { id } if id == "gone"));
        let first = &requests(&r)[0];
        assert!(first.contains("timeMin=") && first.contains("timeMax="));
        assert!(first.contains("singleEvents=true") && !first.contains("syncToken"));
    }

    #[tokio::test]
    async fn later_fetch_sends_only_the_sync_token() {
        let r = rig(
            vec![
                Route::new(
                    "/calendar/v3/calendars/primary/events",
                    200,
                    r#"{"nextSyncToken":"T2","items":[{"id":"x","status":"cancelled"}]}"#,
                )
                .with(&["syncToken=T1"]),
            ],
            HOUR,
        )
        .await;
        let sync = r
            .session
            .sync_events("primary", Some("T1"), NOW)
            .await
            .unwrap();
        assert!(!sync.full);
        assert_eq!(sync.next_sync_token.as_deref(), Some("T2"));
        let first = &requests(&r)[0];
        assert!(!first.contains("timeMin") && first.contains("singleEvents=true"));
    }

    #[tokio::test]
    async fn gone_falls_back_to_a_full_fetch() {
        let r = rig(
            vec![
                Route::new(
                    "/calendar/v3/calendars/primary/events",
                    410,
                    r#"{"error":{"code":410}}"#,
                )
                .with(&["syncToken=OLD"]),
                Route::new(
                    "/calendar/v3/calendars/primary/events",
                    200,
                    r#"{"nextSyncToken":"NEW","items":[]}"#,
                ),
            ],
            HOUR,
        )
        .await;
        let sync = r
            .session
            .sync_events("primary", Some("OLD"), NOW)
            .await
            .unwrap();
        assert!(sync.full);
        assert_eq!(sync.next_sync_token.as_deref(), Some("NEW"));
        assert_eq!(requests(&r).len(), 2);
    }

    #[tokio::test]
    async fn rate_limits_are_retried_with_backoff() {
        let r = rig(
            vec![
                Route::new("/calendar/v3/users/me/calendarList", 429, "{}").times(1),
                Route::new(
                    "/calendar/v3/users/me/calendarList",
                    403,
                    r#"{"error":{"errors":[{"reason":"rateLimitExceeded"}]}}"#,
                )
                .times(1),
                Route::new("/calendar/v3/users/me/calendarList", 200, r#"{"items":[]}"#),
            ],
            HOUR,
        )
        .await;
        assert!(r.session.list_calendars().await.unwrap().is_empty());
        assert_eq!(requests(&r).len(), 3);
    }

    #[tokio::test]
    async fn other_errors_are_not_retried() {
        let r = rig(
            vec![Route::new(
                "/calendar/v3/users/me/calendarList",
                403,
                r#"{"error":{"errors":[{"reason":"forbidden"}]}}"#,
            )],
            HOUR,
        )
        .await;
        assert!(matches!(
            r.session.list_calendars().await,
            Err(Error::Api { status: 403, .. })
        ));
        assert_eq!(requests(&r).len(), 1);
    }

    #[tokio::test]
    async fn an_expired_token_is_refreshed_and_saved() {
        let r = rig(
            vec![
                Route::new("/token", 200, r#"{"access_token":"new","expires_in":3600}"#),
                Route::new("/calendar/v3/users/me/calendarList", 200, r#"{"items":[]}"#),
            ],
            -HOUR,
        )
        .await;
        r.session.list_calendars().await.unwrap();
        let saved = r.tokens.load().unwrap().unwrap();
        assert_eq!(
            (saved.access_token.as_str(), saved.refresh_token.as_str()),
            ("new", "refresh")
        );
        let all = requests(&r);
        assert!(
            all[0].contains("grant_type=refresh_token") && all[0].contains("refresh_token=refresh")
        );
        assert!(all[1].starts_with("GET /calendar/v3/users/me/calendarList"));
    }

    #[tokio::test]
    async fn a_revoked_refresh_token_signs_out() {
        let r = rig(
            vec![Route::new("/token", 400, r#"{"error":"invalid_grant"}"#)],
            -HOUR,
        )
        .await;
        assert!(matches!(
            r.session.list_calendars().await,
            Err(Error::SignedOut)
        ));
        assert!(!r.session.is_signed_in().unwrap());
    }

    #[tokio::test]
    async fn a_401_forces_one_refresh_then_retries() {
        let r = rig(
            vec![
                Route::new("/calendar/v3/users/me/calendarList", 401, "{}").times(1),
                Route::new(
                    "/token",
                    200,
                    r#"{"access_token":"fresh","expires_in":3600}"#,
                ),
                Route::new("/calendar/v3/users/me/calendarList", 200, r#"{"items":[]}"#),
            ],
            HOUR,
        )
        .await;
        r.session.list_calendars().await.unwrap();
        assert_eq!(r.tokens.load().unwrap().unwrap().access_token, "fresh");
    }

    #[tokio::test]
    async fn sign_in_catches_the_redirect_and_saves_tokens() {
        let r = rig(
            vec![Route::new(
                "/token",
                200,
                r#"{"access_token":"A","refresh_token":"R","expires_in":3600}"#,
            )],
            HOUR,
        )
        .await;
        r.tokens.clear().unwrap();
        r.session
            .sign_in(|url| {
                // Play the browser: read redirect_uri and state, then "approve".
                let url = url::Url::parse(url).unwrap();
                let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
                let back = format!("{}/?code=CODE&state={}", q["redirect_uri"], q["state"]);
                tokio::spawn(async move { reqwest::get(back).await });
            })
            .await
            .unwrap();
        let saved = r.tokens.load().unwrap().unwrap();
        assert_eq!(
            (saved.access_token.as_str(), saved.refresh_token.as_str()),
            ("A", "R")
        );
        let form = &requests(&r)[0];
        assert!(form.contains("grant_type=authorization_code") && form.contains("code=CODE"));
        assert!(form.contains("code_verifier=") && form.contains("client_secret=secret"));
    }

    #[tokio::test]
    async fn a_full_round_fills_the_cache_for_selected_calendars_only() {
        let r = rig(
            vec![
                Route::new(
                    "/calendar/v3/users/me/calendarList",
                    200,
                    r##"{"items":[{"id":"a","summary":"A","backgroundColor":"#111111"},
                                  {"id":"b","summary":"B"}]}"##,
                ),
                Route::new(
                    "/calendar/v3/calendars/a/events",
                    200,
                    r#"{"nextSyncToken":"TA","items":[{"id":"e","summary":"Lunch",
                        "start":{"dateTime":"2026-12-01T12:00:00Z"},
                        "end":{"dateTime":"2026-12-01T13:00:00Z"}}]}"#,
                ),
            ],
            HOUR,
        )
        .await;
        let mut store = Store::open_in_memory().unwrap();
        store
            .gcal_set_calendars(&[GcalCalendar {
                id: "a".into(),
                summary: "A".into(),
                color: None,
                selected: false,
                primary: false,
                access_role: None,
            }])
            .unwrap();
        store.gcal_select("a", true).unwrap();
        sync_calendars(&r.session, &mut store, NOW).await.unwrap();
        assert_eq!(store.gcal_sync_token("a").unwrap().as_deref(), Some("TA"));
        assert_eq!(store.gcal_calendars().unwrap().len(), 2);
        let day = 1_796_000_000_000 - 20 * DAY_MS..1_796_000_000_000 + 40 * DAY_MS;
        let events = store
            .gcal_events(day.start, day.end, chrono_tz::UTC)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Lunch");
        assert!(requests(&r).iter().all(|q| !q.contains("/calendars/b/")));
    }

    #[tokio::test]
    async fn google_flags_set_the_default_and_a_choice_wins_and_hidden_ones_are_not_fetched() {
        let r = rig(
            vec![
                Route::new(
                    "/calendar/v3/users/me/calendarList",
                    200,
                    r##"{"items":[
                        {"id":"me","summary":"Me","primary":true,"selected":true,"accessRole":"owner"},
                        {"id":"fam","summary":"Family","selected":true,"accessRole":"writer"},
                        {"id":"hol","summary":"Holidays","accessRole":"reader"}]}"##,
                ),
                Route::new(
                    "/calendar/v3/calendars/me/events",
                    200,
                    r#"{"nextSyncToken":"T","items":[]}"#,
                ),
                Route::new(
                    "/calendar/v3/calendars/fam/events",
                    200,
                    r#"{"nextSyncToken":"T","items":[]}"#,
                ),
                Route::new(
                    "/calendar/v3/calendars/hol/events",
                    200,
                    r#"{"nextSyncToken":"T","items":[]}"#,
                ),
            ],
            HOUR,
        )
        .await;
        let mut store = Store::open_in_memory().unwrap();
        // First round: nothing chosen, so Google's flags decide; the primary one comes first.
        sync_calendars(&r.session, &mut store, NOW).await.unwrap();
        let cals = store.gcal_calendars().unwrap();
        let view: Vec<_> = cals
            .iter()
            .map(|c| {
                (
                    c.id.as_str(),
                    c.selected,
                    c.primary,
                    c.access_role.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            view,
            [
                ("me", true, true, Some("owner")),
                ("fam", true, false, Some("writer")),
                ("hol", false, false, Some("reader")),
            ]
        );
        let fetched = |r: &Rig, id: &str| {
            requests(r)
                .iter()
                .filter(|q| q.contains(&format!("/calendars/{id}/events")))
                .count()
        };
        assert_eq!(
            (fetched(&r, "me"), fetched(&r, "fam"), fetched(&r, "hol")),
            (1, 1, 0)
        );
        // The user hides Family and shows Holidays; the next round keeps both choices
        // and fetches Holidays (no token yet) but not Family.
        store.gcal_select("fam", false).unwrap();
        store.gcal_select("hol", true).unwrap();
        sync_calendars(&r.session, &mut store, NOW).await.unwrap();
        assert_eq!(
            (fetched(&r, "me"), fetched(&r, "fam"), fetched(&r, "hol")),
            (2, 1, 1)
        );
        let shown: Vec<_> = store
            .gcal_calendars()
            .unwrap()
            .into_iter()
            .filter(|c| c.selected)
            .map(|c| c.id)
            .collect();
        assert_eq!(shown, ["me", "hol"]);
    }
}
