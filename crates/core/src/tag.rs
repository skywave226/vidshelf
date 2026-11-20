use rusqlite::{params, Connection};
use video_manager_shared::Tag;

use crate::db::DbError;

pub fn list(conn: &Connection) -> Result<Vec<Tag>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, COUNT(vt.video_id) \
         FROM tags t LEFT JOIN video_tags vt ON vt.tag_id = t.id \
         GROUP BY t.id ORDER BY COUNT(vt.video_id) DESC, t.name COLLATE NOCASE ASC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Tag {
                id: r.get(0)?,
                name: r.get(1)?,
                count: r.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn for_video(conn: &Connection, video_id: i64) -> Result<Vec<Tag>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, (SELECT COUNT(*) FROM video_tags x WHERE x.tag_id = t.id) \
         FROM tags t JOIN video_tags vt ON vt.tag_id = t.id \
         WHERE vt.video_id = ?1 ORDER BY t.name COLLATE NOCASE ASC",
    )?;
    let rows = stmt
        .query_map(params![video_id], |r| {
            Ok(Tag {
                id: r.get(0)?,
                name: r.get(1)?,
                count: r.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn add_video_tag(conn: &Connection, video_id: i64, name: &str) -> Result<Vec<Tag>, DbError> {
    let name = name.trim();
    if name.is_empty() {
        return for_video(conn, video_id);
    }
    conn.execute(
        "INSERT INTO tags (name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
        params![name],
    )?;
    let tag_id: i64 =
        conn.query_row("SELECT id FROM tags WHERE name = ?1", params![name], |r| {
            r.get(0)
        })?;
    conn.execute(
        "INSERT INTO video_tags (video_id, tag_id) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
        params![video_id, tag_id],
    )?;
    for_video(conn, video_id)
}

pub fn remove_video_tag(conn: &Connection, video_id: i64, tag_id: i64) -> Result<Vec<Tag>, DbError> {
    conn.execute(
        "DELETE FROM video_tags WHERE video_id = ?1 AND tag_id = ?2",
        params![video_id, tag_id],
    )?;
    conn.execute(
        "DELETE FROM tags WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM video_tags WHERE tag_id = ?1)",
        params![tag_id],
    )?;
    for_video(conn, video_id)
}