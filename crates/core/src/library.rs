use rusqlite::{params, Connection};
use video_manager_shared::Library;

use crate::db::DbError;

pub fn list(conn: &Connection) -> Result<Vec<Library>, DbError> {
    let mut stmt = conn.prepare("SELECT id, path, name, created_at FROM libraries ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok(Library {
            id: r.get(0)?,
            path: r.get(1)?,
            name: r.get(2)?,
            created_at: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<Library>, DbError> {
    let mut stmt =
        conn.prepare("SELECT id, path, name, created_at FROM libraries WHERE id = ?1")?;
    let mut rows = stmt.query_map(params![id], |r| {
        Ok(Library {
            id: r.get(0)?,
            path: r.get(1)?,
            name: r.get(2)?,
            created_at: r.get(3)?,
        })
    })?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn add(conn: &Connection, path: &str) -> Result<Library, DbError> {
    let p = std::path::Path::new(path);
    if !p.is_dir() {
        return Err(DbError::InvalidPath(path.to_string()));
    }
    // asset 协议拒绝含 `..` 的路径，入库前必须规范化
    let canonical = dunce::canonicalize(p).map_err(|_| DbError::InvalidPath(path.to_string()))?;
    let path = canonical.to_string_lossy().into_owned();
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM libraries WHERE path = ?1",
        params![path],
        |r| r.get(0),
    )?;
    if exists > 0 {
        return Err(DbError::Duplicate);
    }
    let name = canonical
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());
    let now = crate::unix_now();
    conn.execute(
        "INSERT INTO libraries (path, name, created_at) VALUES (?1, ?2, ?3)",
        params![path, name, now],
    )?;
    Ok(Library {
        id: conn.last_insert_rowid(),
        path,
        name,
        created_at: now,
    })
}

pub fn remove(conn: &Connection, id: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM libraries WHERE id = ?1", params![id])?;
    Ok(())
}