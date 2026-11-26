//! Lists, nested tags, and the task fields that move a task out of Inbox.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::oplog::Entity;
use crate::order::{key_between, spread_keys};
use crate::store::Writer;
use crate::task::require_task;
use crate::{Error, Result, Store};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct List {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// How deep the tag tree is walked; deeper tags (or a cycle from concurrent moves) are hidden.
const MAX_TAG_DEPTH: usize = 32;

/// All live tags as a tree walk: each tag, then its children. Siblings go in their
/// arranged order (`sort_key`), and ones never arranged come after, by name.
pub(crate) fn load_tags(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, parent_id, color, icon FROM tag WHERE deleted = 0 \
         ORDER BY sort_key = '', sort_key, lower(name), id",
    )?;
    let rows: Vec<Tag> = stmt
        .query_map([], |r| {
            Ok(Tag {
                id: r.get(0)?,
                name: r.get(1)?,
                parent_id: r.get(2)?,
                path: String::new(),
                color: r.get(3)?,
                icon: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let live: std::collections::HashSet<&str> = rows.iter().map(|t| t.id.as_str()).collect();
    // Children of each parent, already in sibling order; a dangling parent counts as the top.
    let mut children: std::collections::HashMap<Option<&str>, Vec<&Tag>> = Default::default();
    for t in &rows {
        let parent = t.parent_id.as_deref().filter(|p| live.contains(p));
        children.entry(parent).or_default().push(t);
    }
    let mut out = Vec::with_capacity(rows.len());
    let mut seen: std::collections::HashSet<&str> = Default::default();
    let top: Vec<_> = children
        .get(&None)
        .into_iter()
        .flatten()
        .map(|t| (*t, t.name.clone()))
        .collect();
    walk_tags(&children, top, &mut seen, &mut out);
    // Tags the walk never reached sit in a loop (concurrent moves on two devices) or under
    // a chain that is too deep. Don't hide them: show each such tag at the top, in sibling
    // order, with whatever hangs under it. Every device picks the same ones.
    for t in &rows {
        if !seen.contains(t.id.as_str()) {
            walk_tags(&children, vec![(t, t.name.clone())], &mut seen, &mut out);
        }
    }
    Ok(out)
}

type TagChildren<'a> = std::collections::HashMap<Option<&'a str>, Vec<&'a Tag>>;

/// Depth-first from `roots` (each with its path), children in sibling order, skipping
/// tags already `seen`.
fn walk_tags<'a>(
    children: &TagChildren<'a>,
    roots: Vec<(&'a Tag, String)>,
    seen: &mut std::collections::HashSet<&'a str>,
    out: &mut Vec<Tag>,
) {
    let mut stack: Vec<(&Tag, String, usize)> =
        roots.into_iter().rev().map(|(t, p)| (t, p, 0)).collect();
    while let Some((t, path, depth)) = stack.pop() {
        if !seen.insert(t.id.as_str()) {
            continue;
        }
        if depth < MAX_TAG_DEPTH {
            for c in children
                .get(&Some(t.id.as_str()))
                .into_iter()
                .flatten()
                .rev()
            {
                stack.push((c, format!("{path}/{}", c.name), depth + 1));
            }
        }
        out.push(Tag { path, ..t.clone() });
    }
}

/// Shared by `place_list` and `place_filter`: siblings are every live, unarchived row of
/// `table`, ordered as shown (arranged first, then by name).
pub(crate) fn place_among(
    w: &mut crate::store::Writer<'_>,
    entity: Entity,
    table: &str,
    what: &str,
    id: &str,
    before: Option<&str>,
) -> Result<()> {
    let extra = if table == "list" {
        " AND archived = 0"
    } else {
        ""
    };
    let mut stmt = w.conn.prepare(&format!(
        "SELECT id, sort_key FROM {table} WHERE deleted = 0{extra} \
         ORDER BY sort_key = '', sort_key, lower(name), id"
    ))?;
    let mut others: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    others.retain(|(t, _)| t != id);
    let at = match before {
        None => others.len(),
        Some(b) => others.iter().position(|(t, _)| t == b).ok_or_else(|| {
            Error::Invalid(format!("{what} {b} isn't another {what} to put it before"))
        })?,
    };
    let arranged =
        others.iter().all(|(_, k)| !k.is_empty()) && others.windows(2).all(|p| p[0].1 < p[1].1);
    if arranged {
        let a = at.checked_sub(1).map(|i| others[i].1.as_str());
        let b = others.get(at).map(|(_, k)| k.as_str());
        let key = key_between(a, b);
        if a.is_none_or(|a| a < key.as_str()) && b.is_none_or(|b| key.as_str() < b) {
            return w.set(entity, id, "sort_key", key.as_str());
        }
    }
    others.insert(at, (id.to_string(), String::new()));
    for ((t, old), key) in others.iter().zip(spread_keys(others.len())) {
        if *old != key {
            w.set(entity, t, "sort_key", key.as_str())?;
        }
    }
    Ok(())
}

/// The live siblings of `id` (itself included), in display order. Like [load_tags],
/// a tag whose parent is gone counts as top-level.
fn tag_siblings(conn: &Connection, id: &str) -> Result<Vec<(String, String)>> {
    let parent: Option<String> = conn.query_row(
        "SELECT p.id FROM tag t LEFT JOIN tag p ON p.id = t.parent_id AND p.deleted = 0 \
         WHERE t.id = ?1",
        [id],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, sort_key FROM tag WHERE deleted = 0 AND (parent_id IS ?1 OR (?1 IS NULL \
           AND parent_id NOT IN (SELECT id FROM tag WHERE deleted = 0))) \
         ORDER BY sort_key = '', sort_key, lower(name), id",
    )?;
    Ok(stmt
        .query_map([parent], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
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

/// A colour must be `#rrggbb`; it's stored lowercase. Empty means none.
fn clean_color(color: Option<&str>) -> Result<Option<String>> {
    let Some(c) = color.map(str::trim).filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    let hex = c.strip_prefix('#').unwrap_or(c);
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(Error::Invalid(format!("colour '{c}' isn't #rrggbb")));
    }
    Ok(Some(format!("#{}", hex.to_ascii_lowercase())))
}

fn clean_icon(icon: Option<&str>) -> Option<String> {
    icon.map(str::trim)
        .filter(|i| !i.is_empty())
        .map(str::to_string)
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
             WHERE deleted = 0 AND (archived = 0 OR ?1) \
             ORDER BY sort_key = '', sort_key, lower(name), id",
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

    /// Set a list's colour (`#rrggbb`) and icon (an emoji or icon name). `None` clears.
    pub fn set_list_style(&self, id: &str, color: Option<&str>, icon: Option<&str>) -> Result<()> {
        let color = clean_color(color)?;
        let icon = clean_icon(icon);
        self.write(|w| {
            require_list(w.conn, id)?;
            w.set(Entity::List, id, "color", color.as_deref())?;
            w.set(Entity::List, id, "icon", icon.as_deref())
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

    /// Live tags with display paths, as a tree in sidebar order (see `place_tag`).
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

    /// Arrange a tag among its siblings: right before `before` (a sibling), or last with
    /// `None`. Usually one op; the first arrangement of a set of siblings numbers them all.
    pub fn place_tag(&self, id: &str, before: Option<&str>) -> Result<()> {
        self.write(|w| {
            require_tag(w.conn, id)?;
            let mut others = tag_siblings(w.conn, id)?;
            others.retain(|(t, _)| t != id);
            let at = match before {
                None => others.len(),
                Some(b) => others
                    .iter()
                    .position(|(t, _)| t == b)
                    .ok_or_else(|| Error::Invalid(format!("tag {b} isn't a sibling of {id}")))?,
            };
            let arranged = others.iter().all(|(_, k)| !k.is_empty())
                && others.windows(2).all(|p| p[0].1 < p[1].1);
            if arranged {
                let a = at.checked_sub(1).map(|i| others[i].1.as_str());
                let b = others.get(at).map(|(_, k)| k.as_str());
                let key = key_between(a, b);
                // A key this module didn't make (e.g. from a future version) may not fit.
                if a.is_none_or(|a| a < key.as_str()) && b.is_none_or(|b| key.as_str() < b) {
                    return w.set(Entity::Tag, id, "sort_key", key.as_str());
                }
            }
            // Ties, never-arranged or odd keys: number them all in the order they show now.
            others.insert(at, (id.to_string(), String::new()));
            for ((t, old), key) in others.iter().zip(spread_keys(others.len())) {
                if *old != key {
                    w.set(Entity::Tag, t, "sort_key", key.as_str())?;
                }
            }
            Ok(())
        })
    }

    /// Arrange a list: right before `before`, or last with `None` (see `place_tag`).
    pub fn place_list(&self, id: &str, before: Option<&str>) -> Result<()> {
        self.write(|w| {
            require_list(w.conn, id)?;
            place_among(w, Entity::List, "list", "list", id, before)
        })
    }

    /// Set a tag's colour (`#rrggbb`) and icon (an emoji or icon name). `None` clears.
    pub fn set_tag_style(&self, id: &str, color: Option<&str>, icon: Option<&str>) -> Result<()> {
        let color = clean_color(color)?;
        let icon = clean_icon(icon);
        self.write(|w| {
            require_tag(w.conn, id)?;
            w.set(Entity::Tag, id, "color", color.as_deref())?;
            w.set(Entity::Tag, id, "icon", icon.as_deref())
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

    /// The live tags on a task, in sidebar order.
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
    fn tags_in_a_loop_from_concurrent_moves_are_not_hidden() {
        use crate::hlc::Hlc;
        use crate::oplog::{Entity, Op};
        let s = Store::open_in_memory().unwrap();
        let a = s.create_tag("a", None).unwrap();
        let b = s.create_tag("b", None).unwrap();
        let c = s.ensure_tag_path("c/d").unwrap();
        // Device 1 moved a under b while device 2 moved b under a.
        let far = 4_000_000_000_000;
        let mv = |n: u64, id: &str, parent: &str| Op {
            hlc: Hlc {
                wall: far + n,
                counter: 0,
                device: 9,
            },
            entity: Entity::Tag,
            entity_id: id.into(),
            field: "parent_id".into(),
            value: serde_json::json!(parent),
        };
        assert_eq!(s.apply_remote(&[mv(1, &a, &b), mv(2, &b, &a)]).unwrap(), 2);
        let tags = s.tags().unwrap();
        assert_eq!(tags.len(), 4, "nothing disappears: {tags:?}");
        let ids: Vec<_> = tags.iter().map(|t| t.id.as_str()).collect();
        for id in [&a, &b, &c] {
            assert!(ids.contains(&id.as_str()));
        }
        // The same result every time, and each tag once.
        assert_eq!(s.tags().unwrap(), tags);
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 4);
    }

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
    fn tags_walk_as_a_tree_and_can_be_arranged() {
        let s = Store::open_in_memory().unwrap();
        let paths =
            |s: &Store| -> Vec<String> { s.tags().unwrap().into_iter().map(|t| t.path).collect() };
        let work = s.ensure_tag_path("Work").unwrap();
        let home = s.ensure_tag_path("home").unwrap();
        s.ensure_tag_path("home/garden").unwrap();
        let kitchen = s.ensure_tag_path("home/kitchen").unwrap();
        s.ensure_tag_path("home-office").unwrap();
        // Never arranged: by name (any case), children right after their parent.
        assert_eq!(
            paths(&s),
            ["home", "home/garden", "home/kitchen", "home-office", "Work"]
        );

        s.place_tag(&work, Some(&home)).unwrap();
        assert_eq!(
            paths(&s),
            ["Work", "home", "home/garden", "home/kitchen", "home-office"]
        );
        s.place_tag(&home, None).unwrap();
        assert_eq!(
            paths(&s),
            ["Work", "home-office", "home", "home/garden", "home/kitchen"]
        );
        s.place_tag(&kitchen, None).unwrap();
        s.place_tag(
            &kitchen,
            Some(&s.find_tag("home/garden").unwrap().unwrap().id),
        )
        .unwrap();
        assert_eq!(
            paths(&s),
            ["Work", "home-office", "home", "home/kitchen", "home/garden"]
        );

        // A new tag goes after the arranged ones; arranging it is a single op.
        let alpha = s.ensure_tag_path("alpha").unwrap();
        assert_eq!(paths(&s).last().unwrap(), "alpha");
        let before = s.ops().unwrap().len();
        s.place_tag(&alpha, Some(&work)).unwrap();
        assert_eq!(s.ops().unwrap().len(), before + 1);
        assert_eq!(paths(&s)[..2], ["alpha", "Work"]);

        // A tag whose parent was deleted on another device sits at the top level.
        let shed = s.ensure_tag_path("shed").unwrap();
        s.conn
            .execute("UPDATE tag SET parent_id = 'gone' WHERE id = ?1", [&shed])
            .unwrap();
        s.place_tag(&shed, Some(&alpha)).unwrap();
        assert_eq!(paths(&s)[..2], ["shed", "alpha"]);
        assert!(s.place_tag(&kitchen, Some(&work)).is_err(), "not a sibling");
        assert!(s.place_tag(&work, Some(&work)).is_err(), "itself");
    }

    #[test]
    fn lists_and_filters_arrange_like_tags() {
        let s = Store::open_in_memory().unwrap();
        let names = |s: &Store| -> Vec<String> {
            s.lists(false)
                .unwrap()
                .into_iter()
                .map(|l| l.name)
                .collect()
        };
        let a = s.create_list("Alpha").unwrap();
        let b = s.create_list("Beta").unwrap();
        let c = s.create_list("Gamma").unwrap();
        assert_eq!(names(&s), ["Alpha", "Beta", "Gamma"]);
        s.place_list(&c, Some(&a)).unwrap();
        assert_eq!(names(&s), ["Gamma", "Alpha", "Beta"]);
        s.place_list(&c, None).unwrap();
        assert_eq!(names(&s), ["Alpha", "Beta", "Gamma"]);
        let d = s.create_list("Delta").unwrap();
        assert_eq!(names(&s).last().unwrap(), "Delta");
        let before = s.ops().unwrap().len();
        s.place_list(&d, Some(&b)).unwrap();
        assert_eq!(s.ops().unwrap().len(), before + 1);
        assert_eq!(names(&s), ["Alpha", "Delta", "Beta", "Gamma"]);
        assert!(s.place_list(&a, Some(&a)).is_err());
        assert!(s.place_list("nope", None).is_err());

        let rule: crate::FilterRule =
            serde_json::from_value(serde_json::json!({"field":"list","op":"is","value":a}))
                .unwrap();
        let mk = |n: &str| {
            s.save_filter(&crate::Filter {
                name: n.into(),
                icon: None,
                rule: rule.clone(),
                sort: vec![],
                group: None,
                show_completed: false,
            })
            .unwrap()
        };
        let f1 = mk("One");
        let f2 = mk("Two");
        let fnames = |s: &Store| -> Vec<String> {
            s.filters()
                .unwrap()
                .into_iter()
                .map(|f| f.filter.name)
                .collect()
        };
        assert_eq!(fnames(&s), ["One", "Two"]);
        s.place_filter(&f2, Some(&f1)).unwrap();
        assert_eq!(fnames(&s), ["Two", "One"]);
        assert!(s.place_filter("nope", None).is_err());
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

    #[test]
    fn styles_validate_and_clear() {
        let s = Store::open_in_memory().unwrap();
        let tag = s.ensure_tag_path("home/garden").unwrap();
        s.set_tag_style(&tag, Some("#12A594"), Some("🌱")).unwrap();
        let t = s.find_tag(&tag).unwrap().unwrap();
        assert_eq!(
            (t.color.as_deref(), t.icon.as_deref()),
            (Some("#12a594"), Some("🌱"))
        );
        assert!(s.set_tag_style(&tag, Some("teal"), None).is_err());
        s.set_tag_style(&tag, Some(""), None).unwrap();
        let t = s.find_tag(&tag).unwrap().unwrap();
        assert_eq!((t.color, t.icon), (None, None));

        let work = s.create_list("Work").unwrap();
        s.set_list_style(&work, Some("4c8df6"), None).unwrap();
        assert_eq!(
            s.find_list(&work).unwrap().unwrap().color.as_deref(),
            Some("#4c8df6")
        );
    }
}
