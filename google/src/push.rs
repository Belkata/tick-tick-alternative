//! Tasks out (plan/06 "Tasks out", plan/19 slice 4): keep a dedicated "Tasks" calendar on
//! Google in step with the tasks that have a date.
//!
//! The desired state is [`Store::gcal_push_plan`] (events with deterministic ids). The
//! actual state is one `events.list` of the Tasks calendar. A push is the difference:
//!
//! - missing on Google, or its `tt_hash` differs: create (`POST` with our id; if Google
//!   says 409 the id exists, even as a cancelled event, so `PUT` it instead);
//! - on Google but not in the plan (done long ago, trashed, unscheduled, hidden from the
//!   calendar): `DELETE`;
//! - same hash: nothing.
//!
//! Nothing is remembered locally about what was pushed, so a second signed-in device, a
//! restored backup or a half-finished earlier push all converge by the same rule. Events
//! the user added to the Tasks calendar by hand (no `tt_task_id`) are left alone.
//!
//! **Two signed-in devices.** Both compute the same plan from synced tasks, so their
//! bodies and hashes are identical: whoever pushes second sees matching hashes and does
//! nothing, and a race on the same id ends in one 409 and an identical `PUT`. While the
//! devices have not synced yet they may push different versions of one task; the event id
//! stays the same, so Google keeps one event, and the next push after they converge
//! overwrites it. We don't send `If-Match`: a 412 would only say "another device wrote
//! first", and the next reconcile reaches the same end state anyway.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use reqwest::Method;
use serde_json::{Value, json};
use url::Url;
use utre_core::Store;

use crate::session::Session;
use crate::{Error, Result};

/// The calendar's name on Google.
pub const TASKS_CALENDAR_NAME: &str = "Tasks";
/// Put in its description, so a second device finds the calendar the first one made.
const MARKER: &str = "Created by Utre. Edits made here are overwritten.";
const ATTEMPTS: u32 = 5;

/// What one push did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PushReport {
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub unchanged: usize,
    /// Events that Google refused (the others went through); `first_error` says why.
    pub failed: usize,
    pub first_error: Option<String>,
}

impl PushReport {
    pub fn changed(&self) -> usize {
        self.created + self.updated + self.deleted
    }
}

impl Session {
    /// Like `get_json`, for any method and an optional JSON body: one forced refresh on a
    /// 401, exponential back-off on 429, rate-limit 403 and 5xx. Other statuses come back
    /// as [`Error::Api`] so the caller can look at them (409 and 404 mean things here).
    /// An empty reply (DELETE) is `Value::Null`.
    async fn send_json(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value> {
        let mut url = Url::parse(&self.endpoints.api_base)
            .map_err(|e| Error::SignIn(format!("bad API base: {e}")))?;
        url.path_segments_mut()
            .map_err(|_| Error::SignIn("bad API base".into()))?
            .pop_if_empty()
            .extend(segments);
        let mut force = false;
        let mut delay: Duration = self.retry_base;
        for attempt in 1..=ATTEMPTS {
            let token = self.access_token(force).await?;
            let mut req = self
                .http
                .request(method.clone(), url.clone())
                .query(query)
                .bearer_auth(token);
            if let Some(b) = body {
                req = req.json(b);
            }
            let resp = req.send().await?;
            let status = resp.status().as_u16();
            let text = resp.text().await?;
            match status {
                200..=299 if text.trim().is_empty() => return Ok(Value::Null),
                200..=299 => return Ok(serde_json::from_str(&text)?),
                401 if !force => force = true,
                429 | 500..=599 if attempt < ATTEMPTS => {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
                403 if attempt < ATTEMPTS && text.contains("ateLimitExceeded") => {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
                _ => return Err(Error::Api { status, body: text }),
            }
        }
        unreachable!("the last attempt always returns")
    }

    /// The id of the "Tasks" calendar: an existing one with our marker, else a new one.
    async fn find_or_create_tasks_calendar(&self) -> Result<String> {
        let mut page: Option<String> = None;
        loop {
            let mut query = vec![("minAccessRole", "owner"), ("maxResults", "250")];
            if let Some(p) = &page {
                query.push(("pageToken", p));
            }
            let reply = self
                .get_json(&["users", "me", "calendarList"], &query)
                .await?;
            for item in reply["items"].as_array().into_iter().flatten() {
                let name = item["summaryOverride"]
                    .as_str()
                    .or_else(|| item["summary"].as_str());
                if name == Some(TASKS_CALENDAR_NAME)
                    && item["description"].as_str() == Some(MARKER)
                    && let Some(id) = item["id"].as_str()
                {
                    return Ok(id.to_string());
                }
            }
            page = reply["nextPageToken"].as_str().map(str::to_string);
            if page.is_none() {
                break;
            }
        }
        let made = self
            .send_json(
                Method::POST,
                &["calendars"],
                &[],
                Some(&json!({
                    "summary": TASKS_CALENDAR_NAME,
                    "description": MARKER,
                    "timeZone": "UTC",
                })),
            )
            .await?;
        made["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| Error::Api {
                status: 200,
                body: "calendar created without an id".into(),
            })
    }

    /// Our events on the calendar: id to `tt_hash` (empty when it has none).
    async fn list_pushed(&self, calendar: &str) -> Result<HashMap<String, String>> {
        let mut out = HashMap::new();
        let mut page: Option<String> = None;
        loop {
            let mut query = vec![("maxResults", "2500")];
            if let Some(p) = &page {
                query.push(("pageToken", p));
            }
            let reply = self
                .get_json(&["calendars", calendar, "events"], &query)
                .await?;
            for item in reply["items"].as_array().into_iter().flatten() {
                let private = &item["extendedProperties"]["private"];
                if item["status"] == "cancelled" || !private["tt_task_id"].is_string() {
                    continue;
                }
                if let Some(id) = item["id"].as_str() {
                    out.insert(
                        id.to_string(),
                        private["tt_hash"].as_str().unwrap_or_default().to_string(),
                    );
                }
            }
            page = reply["nextPageToken"].as_str().map(str::to_string);
            if page.is_none() {
                return Ok(out);
            }
        }
    }
}

/// Brings the Tasks calendar in line with the tasks. A no-op, with no network traffic,
/// when pushing is off here or nobody is signed in. `&mut` keeps the future `Send`; the
/// store itself is only read for the plan and written for the calendar id.
pub async fn push_tasks(session: &Session, store: &mut Store, now_ms: i64) -> Result<PushReport> {
    if !store.gcal_push_enabled()? || !session.is_signed_in()? {
        return Ok(PushReport::default());
    }
    let plan = store.gcal_push_plan(now_ms)?;
    let mut calendar = match store.gcal_tasks_calendar()? {
        Some(id) => id,
        None => {
            let id = session.find_or_create_tasks_calendar().await?;
            store.gcal_set_tasks_calendar(Some(&id))?;
            id
        }
    };
    let remote = match session.list_pushed(&calendar).await {
        Ok(r) => r,
        // The remembered calendar is gone (deleted on Google): make a fresh one.
        Err(Error::Api { status: 404, .. }) => {
            calendar = session.find_or_create_tasks_calendar().await?;
            store.gcal_set_tasks_calendar(Some(&calendar))?;
            session.list_pushed(&calendar).await?
        }
        Err(e) => return Err(e),
    };

    let mut report = PushReport::default();
    fn fail(report: &mut PushReport, e: Error) {
        report.failed += 1;
        report.first_error.get_or_insert(e.to_string());
    }
    let wanted: HashSet<&str> = plan.iter().map(|e| e.id.as_str()).collect();
    for event in &plan {
        let hash = event.body["extendedProperties"]["private"]["tt_hash"]
            .as_str()
            .unwrap_or_default();
        let known = remote.get(&event.id);
        if known.is_some_and(|h| h == hash) {
            report.unchanged += 1;
            continue;
        }
        let mut body = event.body.clone();
        body["status"] = json!("confirmed");
        match session
            .upsert_event(&calendar, &event.id, &body, known.is_some())
            .await
        {
            Ok(created) if created => report.created += 1,
            Ok(_) => report.updated += 1,
            Err(Error::SignedOut) => return Err(Error::SignedOut),
            Err(e) => fail(&mut report, e),
        }
    }
    for id in remote.keys().filter(|id| !wanted.contains(id.as_str())) {
        match session
            .send_json(
                Method::DELETE,
                &["calendars", &calendar, "events", id],
                &[],
                None,
            )
            .await
        {
            Ok(_)
            | Err(Error::Api {
                status: 404 | 410, ..
            }) => report.deleted += 1,
            Err(Error::SignedOut) => return Err(Error::SignedOut),
            Err(e) => fail(&mut report, e),
        }
    }
    Ok(report)
}

impl Session {
    /// Create-or-update by id. Answers whether it created. `exists`: the id was seen on
    /// Google already, so go straight to `PUT`.
    async fn upsert_event(
        &self,
        calendar: &str,
        id: &str,
        body: &Value,
        exists: bool,
    ) -> Result<bool> {
        if !exists {
            match self
                .send_json(
                    Method::POST,
                    &["calendars", calendar, "events"],
                    &[],
                    Some(body),
                )
                .await
            {
                Ok(_) => return Ok(true),
                // The id is taken (another device was first, or it was deleted on Google
                // and Google still remembers it): update it instead.
                Err(Error::Api { status: 409, .. }) => {}
                Err(e) => return Err(e),
            }
        }
        self.send_json(
            Method::PUT,
            &["calendars", calendar, "events", id],
            &[],
            Some(body),
        )
        .await?;
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use utre_core::{NewTask, Repeat, RepeatMode, Schedule, Task};

    use super::*;
    use crate::oauth::{ClientSecrets, Endpoints};
    use crate::testutil::{Fake, Route, serve};
    use crate::tokens::{FileTokenStore, TokenStore, Tokens};

    const NOW: i64 = 1_796_000_000_000;
    const DAY: i64 = 86_400_000;
    const API: &str = "/calendar/v3";
    const CAL: &str = "/calendar/v3/calendars/tasks-cal/events";

    struct Rig {
        session: Session,
        fake: Fake,
        dir: std::path::PathBuf,
        store: Store,
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// A signed-in (unless `signed_in` is false) session against a fake Google, and a store
    /// with pushing on and the calendar already known.
    async fn rig(routes: Vec<Route>, signed_in: bool) -> Rig {
        rig_with(Store::open_in_memory().unwrap(), routes, signed_in).await
    }

    async fn rig_with(store: Store, routes: Vec<Route>, signed_in: bool) -> Rig {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "utre-push-rig-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let tokens = Arc::new(FileTokenStore::new(dir.join("token.json")));
        if signed_in {
            tokens
                .save(&Tokens {
                    access_token: "tok".into(),
                    refresh_token: "refresh".into(),
                    expires_at: crate::now_ms() + 3_600_000,
                })
                .unwrap();
        }
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
        let mut session = Session::with_endpoints(secrets, tokens, endpoints);
        session.retry_base = Duration::from_millis(1);
        store.gcal_set_push(true).unwrap();
        store.gcal_set_tasks_calendar(Some("tasks-cal")).unwrap();
        Rig {
            session,
            fake,
            dir,
            store,
        }
    }

    fn requests(r: &Rig) -> Vec<String> {
        r.fake.requests.lock().unwrap().clone()
    }

    fn calls(r: &Rig, method: &str) -> Vec<String> {
        requests(r)
            .into_iter()
            .filter(|q| q.starts_with(method))
            .collect()
    }

    fn task(s: &Store, title: &str, start: i64) -> Task {
        s.create_task(&NewTask {
            title: title.into(),
            schedule: Schedule {
                start_at: Some(start),
                due_at: Some(start),
                tz: Some("Europe/Sofia".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap()
    }

    /// What Google answers to the events list for the given (id, hash) pairs.
    fn listing(pairs: &[(&str, &str)]) -> String {
        let items: Vec<Value> = pairs
            .iter()
            .map(|(id, hash)| {
                json!({"id": id, "status": "confirmed", "extendedProperties":
                    {"private": {"tt_task_id": "x", "tt_hash": hash}}})
            })
            .collect();
        json!({ "items": items }).to_string()
    }

    fn hash_of(s: &Store, task: &Task) -> String {
        let plan = s.gcal_push_plan(NOW).unwrap();
        plan.iter()
            .find(|e| e.task_id == task.id)
            .unwrap()
            .body["extendedProperties"]["private"]["tt_hash"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn list_route(body: &str) -> Route {
        Route::new(CAL, 200, body).method("GET")
    }

    #[tokio::test]
    async fn creates_a_missing_event_with_the_deterministic_id() {
        let mut r = rig(
            vec![
                list_route(&listing(&[])),
                Route::new(CAL, 200, "{}").method("POST"),
            ],
            true,
        )
        .await;
        let t = task(&r.store, "Dentist", NOW + DAY);
        let report = push_tasks(&r.session, &mut r.store, NOW).await.unwrap();
        assert_eq!((report.created, report.updated, report.deleted), (1, 0, 0));
        let posts = calls(&r, "POST");
        assert_eq!(posts.len(), 1);
        let id = utre_core::task_event_id(&t.id).unwrap();
        assert!(
            posts[0].contains(&format!("\"id\":\"{id}\"")),
            "{}",
            posts[0]
        );
        assert!(posts[0].contains("\"summary\":\"Dentist\""));
        assert!(posts[0].contains("tt_hash"));
        assert!(posts[0].contains("\"status\":\"confirmed\""));
    }

    #[tokio::test]
    async fn an_unchanged_event_is_not_sent_again() {
        let store = Store::open_in_memory().unwrap();
        let t = task(&store, "Dentist", NOW + DAY);
        let id = utre_core::task_event_id(&t.id).unwrap();
        let hash = hash_of(&store, &t);
        let mut r = rig_with(store, vec![list_route(&listing(&[(&id, &hash)]))], true).await;
        let report = push_tasks(&r.session, &mut r.store, NOW).await.unwrap();
        assert_eq!(report.unchanged, 1);
        assert_eq!(report.changed(), 0);
        assert_eq!(requests(&r).len(), 1, "only the list");
    }

    #[tokio::test]
    async fn a_changed_task_updates_by_put_and_a_taken_id_falls_back_to_put() {
        let store = Store::open_in_memory().unwrap();
        let a = task(&store, "Edited", NOW + DAY);
        let b = task(&store, "Taken", NOW + 2 * DAY);
        let ida = utre_core::task_event_id(&a.id).unwrap();
        let idb = utre_core::task_event_id(&b.id).unwrap();
        let mut r = rig_with(
            store,
            vec![
                // `a` is on Google with an old hash; `b` isn't listed but its id is taken.
                list_route(&listing(&[(&ida, "stale")])),
                Route::new(format!("{CAL}/{ida}"), 200, "{}").method("PUT"),
                Route::new(format!("{CAL}/{idb}"), 200, "{}").method("PUT"),
                Route::new(CAL, 409, r#"{"error":{"code":409}}"#).method("POST"),
            ],
            true,
        )
        .await;
        let report = push_tasks(&r.session, &mut r.store, NOW).await.unwrap();
        assert_eq!((report.created, report.updated), (0, 2));
        let puts = calls(&r, "PUT");
        assert_eq!(puts.len(), 2);
        assert_eq!(calls(&r, "POST").len(), 1, "only b tries POST first");
        assert!(puts.iter().any(|p| p.contains("\"summary\":\"Edited\"")));
    }

    #[tokio::test]
    async fn events_without_a_task_are_deleted_but_foreign_ones_stay() {
        let mut r = rig(
            vec![
                list_route(
                    &json!({"items": [
                        {"id": "gone", "extendedProperties": {"private": {"tt_task_id": "x"}}},
                        {"id": "gone2", "extendedProperties": {"private": {"tt_task_id": "y"}}},
                        {"id": "mine-by-hand", "summary": "Lunch"},
                    ]})
                    .to_string(),
                ),
                Route::new(format!("{CAL}/gone2"), 404, "{}").method("DELETE"),
                Route::new(format!("{CAL}/gone"), 204, "").method("DELETE"),
            ],
            true,
        )
        .await;
        // A trashed task is just absent from the plan.
        let t = task(&r.store, "Trashed", NOW + DAY);
        r.store.trash_task(&t.id).unwrap();
        let report = push_tasks(&r.session, &mut r.store, NOW).await.unwrap();
        assert_eq!(report.deleted, 2, "404 on delete counts as gone");
        let deletes = calls(&r, "DELETE");
        assert_eq!(deletes.len(), 2);
        assert!(deletes.iter().all(|d| !d.contains("mine-by-hand")));
    }

    #[tokio::test]
    async fn a_repeating_task_goes_out_as_occurrences() {
        let mut r = rig(
            vec![
                list_route(&listing(&[])),
                Route::new(CAL, 200, "{}").method("POST"),
            ],
            true,
        )
        .await;
        let t = task(&r.store, "Water plants", NOW + DAY);
        r.store
            .set_repeat(
                &t.id,
                Some(&Repeat {
                    rule: "FREQ=WEEKLY".into(),
                    mode: RepeatMode::Fixed,
                }),
            )
            .unwrap();
        let report = push_tasks(&r.session, &mut r.store, NOW).await.unwrap();
        assert!((16..=19).contains(&report.created), "{}", report.created);
        let base = utre_core::task_event_id(&t.id).unwrap();
        let posts = calls(&r, "POST");
        assert!(
            posts
                .iter()
                .all(|p| p.contains(&format!("\"id\":\"{base}v")))
        );
        assert!(posts.iter().all(|p| !p.contains("RRULE")));
    }

    #[tokio::test]
    async fn rate_limits_are_retried_with_backoff() {
        let mut r = rig(
            vec![
                list_route(&listing(&[])),
                Route::new(CAL, 429, "{}").method("POST").times(1),
                Route::new(
                    CAL,
                    403,
                    r#"{"error":{"errors":[{"reason":"rateLimitExceeded"}]}}"#,
                )
                .method("POST")
                .times(1),
                Route::new(CAL, 503, "{}").method("POST").times(1),
                Route::new(CAL, 200, "{}").method("POST"),
            ],
            true,
        )
        .await;
        task(&r.store, "Dentist", NOW + DAY);
        let report = push_tasks(&r.session, &mut r.store, NOW).await.unwrap();
        assert_eq!(report.created, 1);
        assert_eq!(calls(&r, "POST").len(), 4);
    }

    #[tokio::test]
    async fn a_refused_event_is_counted_and_the_rest_still_go() {
        let mut r = rig(
            vec![
                list_route(&listing(&[])),
                Route::new(
                    CAL,
                    400,
                    r#"{"error":{"errors":[{"reason":"badRequest"}]}}"#,
                )
                .method("POST")
                .times(1),
                Route::new(CAL, 200, "{}").method("POST"),
            ],
            true,
        )
        .await;
        task(&r.store, "A", NOW + DAY);
        task(&r.store, "B", NOW + 2 * DAY);
        let report = push_tasks(&r.session, &mut r.store, NOW).await.unwrap();
        assert_eq!((report.created, report.failed), (1, 1));
        assert!(report.first_error.unwrap().contains("400"));
    }

    #[tokio::test]
    async fn signed_out_or_switched_off_does_nothing() {
        let mut r = rig(vec![], false).await;
        task(&r.store, "Dentist", NOW + DAY);
        assert_eq!(
            push_tasks(&r.session, &mut r.store, NOW).await.unwrap(),
            PushReport::default()
        );
        let mut r = rig(vec![], true).await;
        task(&r.store, "Dentist", NOW + DAY);
        r.store.gcal_set_push(false).unwrap();
        assert_eq!(
            push_tasks(&r.session, &mut r.store, NOW).await.unwrap(),
            PushReport::default()
        );
        assert!(requests(&r).is_empty());
    }

    #[tokio::test]
    async fn the_tasks_calendar_is_found_by_its_marker_or_created_once() {
        // Another device made it: reuse, don't create.
        let mut r = rig(
            vec![
                Route::new(
                    format!("{API}/users/me/calendarList"),
                    200,
                    &json!({"items": [
                        {"id": "impostor", "summary": "Tasks", "description": "mine"},
                        {"id": "theirs", "summary": "Tasks", "description": MARKER},
                    ]})
                    .to_string(),
                ),
                Route::new(format!("{API}/calendars/theirs/events"), 200, &listing(&[]))
                    .method("GET"),
            ],
            true,
        )
        .await;
        r.store.gcal_set_tasks_calendar(None).unwrap();
        push_tasks(&r.session, &mut r.store, NOW).await.unwrap();
        assert_eq!(
            r.store.gcal_tasks_calendar().unwrap().as_deref(),
            Some("theirs")
        );
        assert!(calls(&r, "POST").is_empty());

        // Nobody made it: create it, remember its id.
        let mut r = rig(
            vec![
                Route::new(
                    format!("{API}/users/me/calendarList"),
                    200,
                    r#"{"items":[]}"#,
                ),
                Route::new(format!("{API}/calendars"), 200, r#"{"id":"fresh"}"#).method("POST"),
                Route::new(format!("{API}/calendars/fresh/events"), 200, &listing(&[]))
                    .method("GET"),
            ],
            true,
        )
        .await;
        r.store.gcal_set_tasks_calendar(None).unwrap();
        push_tasks(&r.session, &mut r.store, NOW).await.unwrap();
        assert_eq!(
            r.store.gcal_tasks_calendar().unwrap().as_deref(),
            Some("fresh")
        );
        let posts = calls(&r, "POST");
        assert_eq!(posts.len(), 1);
        assert!(posts[0].contains("\"summary\":\"Tasks\""));
    }

    #[tokio::test]
    async fn a_deleted_calendar_is_replaced() {
        let mut r = rig(
            vec![
                Route::new(format!("{API}/calendars/tasks-cal/events"), 404, "{}").method("GET"),
                Route::new(
                    format!("{API}/users/me/calendarList"),
                    200,
                    r#"{"items":[]}"#,
                ),
                Route::new(format!("{API}/calendars"), 200, r#"{"id":"fresh"}"#).method("POST"),
                Route::new(format!("{API}/calendars/fresh/events"), 200, &listing(&[]))
                    .method("GET"),
                Route::new(format!("{API}/calendars/fresh/events"), 200, "{}").method("POST"),
            ],
            true,
        )
        .await;
        task(&r.store, "Dentist", NOW + DAY);
        let report = push_tasks(&r.session, &mut r.store, NOW).await.unwrap();
        assert_eq!(report.created, 1);
        assert_eq!(
            r.store.gcal_tasks_calendar().unwrap().as_deref(),
            Some("fresh")
        );
    }
}
