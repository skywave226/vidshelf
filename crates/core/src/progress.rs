use rusqlite::{params, Connection};
use video_manager_shared::PlaybackProgress;

use crate::db::DbError;

pub fn save(conn: &Connection, video_id: i64, position_ms: i64) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO playback_progress (video_id, position_ms, watched, updated_at) \
         SELECT ?1, ?2, CASE WHEN duration_ms > 0 AND ?2 >= duration_ms * 95 / 100 THEN 1 ELSE 0 END, ?3 \
         FROM videos WHERE id = ?1 \
         ON CONFLICT(video_id) DO UPDATE SET \
           position_ms = excluded.position_ms, watched = excluded.watched, updated_at = excluded.updated_at",
        params![video_id, position_ms, crate::unix_now()],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, video_id: i64) -> Result<Option<PlaybackProgress>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT position_ms, watched FROM playback_progress WHERE video_id = ?1",
    )?;
    let mut rows = stmt.query_map(params![video_id], |r| {
        Ok(PlaybackProgress { position_ms: r.get(0)?, watched: r.get(1)? })
    })?;
    Ok(rows.next().transpose()?)
}