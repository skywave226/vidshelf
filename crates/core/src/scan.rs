use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::UNIX_EPOCH;

use rusqlite::{params, Connection};
use walkdir::WalkDir;

use crate::db::DbError;
use crate::probe::ProbeInfo;

pub const VIDEO_EXTS: &[&str] = &[
    "mp4", "mkv", "avi", "wmv", "mov", "flv", "ts", "m2ts", "webm", "mpg", "mpeg", "rmvb", "rm",
    "3gp", "vob",
];

pub struct FileEntry {
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub mtime: i64,
}

pub fn walk(root: &Path) -> Vec<FileEntry> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| VIDEO_EXTS.contains(&s.to_ascii_lowercase().as_str()))
                .unwrap_or(false)
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let mtime = meta
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_secs() as i64;
            Some(FileEntry {
                path: e.path().to_string_lossy().into_owned(),
                filename: e.file_name().to_string_lossy().into_owned(),
                size: meta.len() as i64,
                mtime,
            })
        })
        .collect()
}

#[derive(Default)]
pub struct SyncStats {
    pub added: i64,
    pub updated: i64,
    pub removed: i64,
    pub added_ids: Vec<i64>,
}

pub fn sync(conn: &Connection, library_id: i64, entries: &[FileEntry]) -> Result<SyncStats, DbError> {
    let mut existing: HashMap<String, (i64, i64, i64)> = HashMap::new();
    {
        let mut stmt =
            conn.prepare("SELECT id, path, size, mtime FROM videos WHERE library_id = ?1")?;
        let rows = stmt.query_map(params![library_id], |r| {
            Ok((
                r.get::<_, String>(1)?,
                (r.get::<_, i64>(0)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?),
            ))
        })?;
        for row in rows {
            let (path, v) = row?;
            existing.insert(path, v);
        }
    }

    let tx = conn.unchecked_transaction()?;
    let now = crate::unix_now();
    let mut stats = SyncStats::default();
    let mut seen: HashSet<&str> = HashSet::new();
    for e in entries {
        seen.insert(&e.path);
        match existing.get(&e.path) {
            Some((id, size, mtime)) => {
                if *size != e.size || *mtime != e.mtime {
                    tx.execute(
                        "UPDATE videos SET size = ?1, mtime = ?2, filename = ?3 WHERE id = ?4",
                        params![e.size, e.mtime, e.filename, id],
                    )?;
                    stats.updated += 1;
                }
            }
            None => {
                tx.execute(
                    "INSERT INTO videos (library_id, path, filename, size, mtime, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![library_id, e.path, e.filename, e.size, e.mtime, now],
                )?;
                stats.added += 1;
                stats.added_ids.push(tx.last_insert_rowid());
            }
        }
    }
    for (path, (id, _, _)) in &existing {
        if !seen.contains(path.as_str()) {
            tx.execute("DELETE FROM videos WHERE id = ?1", params![id])?;
            stats.removed += 1;
        }
    }
    tx.commit()?;
    Ok(stats)
}

pub fn pending_probe(conn: &Connection, library_id: i64) -> Result<Vec<(i64, String)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, path FROM videos WHERE library_id = ?1 AND duration_ms IS NULL ORDER BY id",
    )?;
    let rows = stmt.query_map(params![library_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn save_probe(conn: &Connection, id: i64, info: Option<&ProbeInfo>) -> Result<(), DbError> {
    match info {
        Some(p) => {
            conn.execute(
                "UPDATE videos SET duration_ms = ?1, width = ?2, height = ?3 WHERE id = ?4",
                params![p.duration_ms, p.width, p.height, id],
            )?;
        }
        None => {
            conn.execute("UPDATE videos SET duration_ms = -1 WHERE id = ?1", params![id])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_sync_roundtrip() {
        let root = std::env::temp_dir().join(format!("vm-scan-test-{}", crate::unix_now()));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.mp4"), b"fake").unwrap();
        std::fs::write(root.join("b.mkv"), b"fake").unwrap();
        std::fs::write(root.join("notes.txt"), b"not a video").unwrap();
        std::fs::write(root.join("sub/d.avi"), b"fake").unwrap();

        let conn = crate::db::init(&root.join("test.db")).unwrap();
        let lib = crate::library::add(&conn, &root.to_string_lossy()).unwrap();

        let entries = walk(&root);
        assert_eq!(entries.len(), 3);

        let stats = sync(&conn, lib.id, &entries).unwrap();
        assert_eq!(stats.added, 3);

        let stats = sync(&conn, lib.id, &entries).unwrap();
        assert_eq!((stats.added, stats.updated, stats.removed), (0, 0, 0));

        std::fs::remove_file(root.join("b.mkv")).unwrap();
        let stats = sync(&conn, lib.id, &walk(&root)).unwrap();
        assert_eq!(stats.removed, 1);
        assert_eq!(crate::db::video_count(&conn).unwrap(), 2);

        drop(conn);
        std::fs::remove_dir_all(&root).unwrap();
    }
}