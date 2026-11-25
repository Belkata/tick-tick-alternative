//! The TickTick importer maps fields and is safe to run twice.
use serde_json::json;
use utre_core::{Store, parse_export};

fn export() -> serde_json::Value {
    json!({
        "projects": [{"id": "p1", "name": "✈️Work", "color": "#E6EA49"}, {"id": "inbox", "name": "Inbox"}],
        "columns": [{"id": "c1", "name": "To do"}],
        "tags": [
            {"name": "theater", "label": "theater", "color": "#8C5EFA", "parent": ""},
            {"name": "via-verde", "label": "Via-Verde", "color": "#FFD966", "parent": "theater"}
        ],
        "undone": [
            {"id": "a", "projectId": "p1", "title": "Parent", "content": "notes", "priority": 5,
             "isAllDay": true, "timeZone": "Europe/Sofia",
             "startDate": "2026-11-09T00:00:00+0200", "dueDate": "2026-11-09T00:00:00+0200",
             "repeatFlag": "RRULE:FREQ=DAILY;INTERVAL=1", "repeatFrom": "2", "reminders": ["TRIGGER:PT9H"],
             "status": 0, "tags": ["via-verde"], "createdTime": "2026-10-22T17:41:02+0300",
             "items": [{"title": "one", "status": 0, "sortOrder": 0}, {"title": "two", "status": 2, "sortOrder": 1}]},
            {"id": "b", "projectId": "inbox123", "title": "Child", "parentId": "a", "status": 0,
             "createdTime": "2026-10-23T10:00:00+0300"}
        ],
        "completed": [
            {"id": "c", "projectId": "inbox123", "title": "Done one", "status": 2,
             "completedTime": "2026-11-01T10:00:00+0200", "createdTime": "2026-10-01T10:00:00+0200"}
        ]
    })
}

#[test]
fn imports_and_is_idempotent() {
    let store = Store::open_in_memory().unwrap();
    let text = export().to_string();
    let r = store
        .import_ticktick(&parse_export(&text).unwrap())
        .unwrap();
    assert_eq!((r.open_tasks, r.done_tasks, r.subtasks), (2, 1, 1));
    assert_eq!((r.checklist_items, r.repeats, r.reminders), (2, 1, 1));
    assert_eq!(store.lists(false).unwrap()[0].name, "Work");
    assert!(store.find_tag("theater/Via-Verde").unwrap().is_some());

    let again = store
        .import_ticktick(&parse_export(&text).unwrap())
        .unwrap();
    assert_eq!(
        (again.open_tasks, again.done_tasks, again.already_there),
        (0, 0, 3)
    );
}
