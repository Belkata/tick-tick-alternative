//! Lists, nested tags, and the task fields that move a task out of Inbox.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::oplog::Entity;
use crate::store::Writer;
use crate::task::require_task;
use crate::{Error, Result, Store};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    /// Display path, e.g. `home/garden`.
    pub path: String,
    pub color: Option<String>,
    pub icon: Option<String>,
}

pub(crate) fn require_list(conn: &Connection, id: &str) -> Result<()> {
    let ok: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM list WHERE id = ?1 AND deleted = 0",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    ok.map(|_| ())
        .ok_or_else(|| Error::NotFound(format!("list {id}")))
}

pub(crate) fn require_tag(conn: &Connection, id: &str) -> Result<()> {
    let ok: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM tag WHERE id = ?1 AND deleted = 0",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    ok.map(|_| ())
        .ok_or_else(|| Error::NotFound(format!("tag {id}")))
}

fn clean_name(name: &str, what: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid(format!("a {what} needs a name")));
    }
    Ok(name.to_string())
}

/// All live tags with their display paths, sorted by path.
pub(crate) fn load_tags(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE t(id, name, parent_id, color, icon, path, depth) AS (
             SELECT id, name, parent_id, color, icon, name, 0 FROM tag
              WHERE deleted = 0 AND (parent_id IS NULL
                    OR parent_id NOT IN (SELECT id FROM tag WHERE deleted = 0))
             UNION ALL
             SELECT c.id, c.name, c.parent_id, c.color, c.icon, t.path || '/' || c.name, t.depth + 1
               FROM tag c JOIN t ON c.parent_id = t.id
              WHERE c.deleted = 0 AND t.depth < 32
         )
         SELECT id, name, parent_id, path, color, icon FROM t ORDER BY path, id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Tag {
            id: r.get(0)?,
            name: r.get(1)?,
            parent_id: r.get(2)?,
            path: r.get(3)?,
            color: r.get(4)?,
            icon: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// `id` plus every live tag under it.
pub(crate) fn tag_subtree(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE d(id, depth) AS (SELECT ?1, 0
           UNION SELECT t.id, d.depth + 1 FROM tag t JOIN d ON t.parent_id = d.id
            WHERE t.deleted = 0 AND d.depth < 32)
         SELECT id FROM d",
    )?;
    let ids = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

/// A tag by id or by path (`home/garden`, case-insensitive).
pub(crate) fn find_tag(conn: &Connection, id_or_path: &str) -> Result<Option<Tag>> {
    let tags = load_tags(conn)?;
    Ok(tags
        .iter()
        .find(|t| t.id == id_or_path)
        .or_else(|| {
            tags.iter()
                .find(|t| t.path.eq_ignore_ascii_case(id_or_path))
        })
        .cloned())
}

/// A list by id or by name (case-insensitive), archived ones included.
pub(crate) fn find_list(conn: &Connection, id_or_name: &str) -> Result<Option<List>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, color, icon, archived FROM list WHERE deleted = 0 \
           AND (id = ?1 OR lower(name) = lower(?1)) ORDER BY id = ?1 DESC, id LIMIT 1",
    )?;
    Ok(stmt
        .query_row([id_or_name], |r| {
            Ok(List {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                icon: r.get(3)?,
                archived: r.get(4)?,
            })
        })
        .optional()?)
}

fn sibling_tag(conn: &Connection, parent_id: Option<&str>, name: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT id FROM tag WHERE deleted = 0 AND parent_id IS ?1 AND lower(name) = lower(?2) \
             ORDER BY id LIMIT 1",
            params![parent_id, name],
            |r| r.get(0),
        )
        .optional()?)
}

fn create_tag_in(w: &mut Writer, name: &str, parent_id: Option<&str>) -> Result<String> {
    if name.contains('/') {
        return Err(Error::Invalid("tag names can't contain '/'".into()));
    }
    if let Some(p) = parent_id {
        require_tag(w.conn, p)?;
    }
    // Names are unique among siblings: reuse the existing tag.
    if let Some(id) = sibling_tag(w.conn, parent_id, name)? {
        return Ok(id);
    }
    let id = Uuid::now_v7().to_string();
    let now = w.now();
    w.set(Entity::Tag, &id, "name", name)?;
    w.set(Entity::Tag, &id, "created_at", now)?;
    if parent_id.is_some() {
        w.set(Entity::Tag, &id, "parent_id", parent_id)?;
    }
    Ok(id)
}

impl Store {
    // ----- Lists -----

    /// Create a flat list. Returns its id.
    pub fn create_list(&self, name: &str) -> Result<String> {
        let name = clean_name(name, "list")?;
        self.write(|w| {
            if find_list(w.conn, &name)?.is_some_and(|l| l.name.eq_ignore_ascii_case(&name)) {
                return Err(Error::Invalid(format!("a list named '{name}' exists")));
            }
            let id = Uuid::now_v7().to_string();
            let now = w.now();
            w.set(Entity::List, &id, "name", name.as_str())?;
            w.set(Entity::List, &id, "created_at", now)?;
            Ok(id)
        })
    }

    /// Lists that aren't deleted, by name. Archived ones only if asked.
    pub fn lists(&self, include_archived: bool) -> Result<Vec<List>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, color, icon, archived FROM list \
             WHERE deleted = 0 AND (archived = 0 OR ?1) ORDER BY sort_key, lower(name), id",
        )?;
        let rows = stmt.query_map([include_archived], |r| {
            Ok(List {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                icon: r.get(3)?,
                archived: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A list by id or name (case-insensitive).
    pub fn find_list(&self, id_or_name: &str) -> Result<Option<List>> {
        find_list(&self.conn, id_or_name)
    }

    pub fn rename_list(&self, id: &str, name: &str) -> Result<()> {
        let name = clean_name(name, "list")?;
        self.write(|w| {
            require_list(w.conn, id)?;
            w.set(Entity::List, id, "name", name.as_str())
        })
    }

    pub fn set_list_archived(&self, id: &str, archived: bool) -> Result<()> {
        self.write(|w| {
            require_list(w.conn, id)?;
            w.set(Entity::List, id, "archived", archived)
        })
    }

    /// Delete a list. Its tasks stay, with no list (so untagged, undated ones return to Inbox).
    pub fn delete_list(&self, id: &str) -> Result<()> {
        self.write(|w| {
            require_list(w.conn, id)?;
            let mut stmt = w.conn.prepare("SELECT id FROM task WHERE list_id = ?1")?;
            let tasks: Vec<String> = stmt
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            drop(stmt);
            for t in tasks {
                w.set(Entity::Task, &t, "list_id", None::<String>)?;
            }
            w.set(Entity::List, id, "deleted", true)
        })
    }

    /// Move a task into a list, or out of every list with `None`.
    pub fn set_task_list(&self, task_id: &str, list_id: Option<&str>) -> Result<()> {
        self.write(|w| {
            require_task(w.conn, task_id)?;
            if let Some(l) = list_id {
                require_list(w.conn, l)?;
            }
            w.set(Entity::Task, task_id, "list_id", list_id)
        })
    }

    // ----- Tags -----

    /// Create a tag, optionally nested under `parent_id`. Returns its id; an existing
    /// sibling with the same name (any case) is reused.
    pub fn create_tag(&self, name: &str, parent_id: Option<&str>) -> Result<String> {
        let name = clean_name(name, "tag")?;
        self.write(|w| create_tag_in(w, &name, parent_id))
    }

    /// Find or create every level of a path like `home/garden`. Returns the leaf's id.
    pub fn ensure_tag_path(&self, path: &str) -> Result<String> {
        let parts: Vec<&str> = path.split('/').map(str::trim).collect();
        if parts.iter().any(|p| p.is_empty()) {
            return Err(Error::Invalid(format!("bad tag path '{path}'")));
        }
        self.write(|w| {
            let mut parent: Option<String> = None;
            for part in parts {
                parent = Some(create_tag_in(w, part, parent.as_deref())?);
            }
            Ok(parent.expect("at least one part"))
        })
    }

    /// Live tags with display paths, sorted by path.
    pub fn tags(&self) -> Result<Vec<Tag>> {
        load_tags(&self.conn)
    }

    /// A tag by id or path (case-insensitive).
    pub fn find_tag(&self, id_or_path: &str) -> Result<Option<Tag>> {
        find_tag(&self.conn, id_or_path)
    }

    pub fn rename_tag(&self, id: &str, name: &str) -> Result<()> {
        let name = clean_name(name, "tag")?;
        if name.contains('/') {
            return Err(Error::Invalid("tag names can't contain '/'".into()));
        }
        self.write(|w| {
            require_tag(w.conn, id)?;
            let parent: Option<String> =
                w.conn
                    .query_row("SELECT parent_id FROM tag WHERE id = ?1", [id], |r| {
                        r.get(0)
                    })?;
            if sibling_tag(w.conn, parent.as_deref(), &name)?.is_some_and(|s| s != id) {
                return Err(Error::Invalid(format!(
                    "a sibling tag named '{name}' exists"
                )));
            }
            w.set(Entity::Tag, id, "name", name.as_str())
        })
    }

    /// Re-nest a tag under `parent_id`, or at the top with `None`. Tasks keep it.
    pub fn move_tag(&self, id: &str, parent_id: Option<&str>) -> Result<()> {
        self.write(|w| {
            require_tag(w.conn, id)?;
            if let Some(p) = parent_id {
                require_tag(w.conn, p)?;
                if tag_subtree(w.conn, id)?.iter().any(|t| t == p) {
                    return Err(Error::Invalid("a tag can't be under itself".into()));
                }
            }
            w.set(Entity::Tag, id, "parent_id", parent_id)
        })
    }

    /// Delete a tag and the tags under it, and remove them from every task.
    pub fn delete_tag(&self, id: &str) -> Result<()> {
        self.write(|w| {
            require_tag(w.conn, id)?;
            for tag in tag_subtree(w.conn, id)? {
                let mut stmt = w
                    .conn
                    .prepare("SELECT task_id FROM task_tag WHERE tag_id = ?1")?;
                let tasks: Vec<String> = stmt
                    .query_map([&tag], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                drop(stmt);
                for t in tasks {
                    w.set(Entity::Task, &t, &format!("tag:{tag}"), false)?;
                }
                w.set(Entity::Tag, &tag, "deleted", true)?;
            }
            Ok(())
        })
    }

    pub fn tag_task(&self, task_id: &str, tag_id: &str) -> Result<()> {
        self.write(|w| {
            require_task(w.conn, task_id)?;
            require_tag(w.conn, tag_id)?;
            w.set(Entity::Task, task_id, &format!("tag:{tag_id}"), true)
        })
    }

    pub fn untag_task(&self, task_id: &str, tag_id: &str) -> Result<()> {
        self.write(|w| {
            require_task(w.conn, task_id)?;
            w.set(Entity::Task, task_id, &format!("tag:{tag_id}"), false)
        })
    }

    /// The live tags on a task, by path.
    pub fn task_tags(&self, task_id: &str) -> Result<Vec<Tag>> {
        let mut stmt = self
            .conn
            .prepare("SELECT tag_id FROM task_tag WHERE task_id = ?1")?;
        let ids: Vec<String> = stmt
            .query_map([task_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(load_tags(&self.conn)?
            .into_iter()
            .filter(|t| ids.contains(&t.id))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[test]
    fn tag_paths_nest_and_dedupe() {
        let s = Store::open_in_memory().unwrap();
        let garden = s.ensure_tag_path("home/garden").unwrap();
        assert_eq!(
            s.ensure_tag_path("Home/Garden").unwrap(),
            garden,
            "case-insensitive reuse"
        );
        let paths: Vec<_> = s.tags().unwrap().into_iter().map(|t| t.path).collect();
        assert_eq!(paths, ["home", "home/garden"]);
        assert_eq!(s.find_tag("home/garden").unwrap().unwrap().id, garden);
        assert!(s.create_tag("a/b", None).is_err());
    }

    #[test]
    fn rename_and_move_keep_task_links() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("t").unwrap();
        let errands = s.ensure_tag_path("errands").unwrap();
        let home = s.ensure_tag_path("home").unwrap();
        s.tag_task(&t.id, &errands).unwrap();
        s.rename_tag(&errands, "shopping").unwrap();
        s.move_tag(&errands, Some(&home)).unwrap();
        assert!(s.move_tag(&home, Some(&errands)).is_err(), "loop");
        let tags = s.task_tags(&t.id).unwrap();
        assert_eq!(tags[0].path, "home/shopping");
    }

    #[test]
    fn deleting_a_tag_untags_tasks_and_children() {
        let s = Store::open_in_memory().unwrap();
        let t = s.add_task("t").unwrap();
        let home = s.ensure_tag_path("home").unwrap();
        let garden = s.ensure_tag_path("home/garden").unwrap();
        s.tag_task(&t.id, &garden).unwrap();
        assert!(s.inbox().unwrap().is_empty());
        s.delete_tag(&home).unwrap();
        assert!(s.tags().unwrap().is_empty());
        assert_eq!(s.inbox().unwrap().len(), 1, "back in Inbox");
    }

    #[test]
    fn lists_rename_archive_delete() {
        let s = Store::open_in_memory().unwrap();
        let work = s.create_list("Work").unwrap();
        assert!(s.create_list("work").is_err(), "duplicate name");
        let t = s.add_task("t").unwrap();
        s.set_task_list(&t.id, Some(&work)).unwrap();
        s.rename_list(&work, "Job").unwrap();
        assert_eq!(s.find_list("job").unwrap().unwrap().id, work);
        s.set_list_archived(&work, true).unwrap();
        assert!(s.lists(false).unwrap().is_empty());
        assert_eq!(s.lists(true).unwrap().len(), 1);
        s.delete_list(&work).unwrap();
        assert_eq!(s.get_task(&t.id).unwrap().unwrap().list_id, None);
        assert_eq!(s.inbox().unwrap().len(), 1);
    }
}
