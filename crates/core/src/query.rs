use rusqlite::{params, Connection};
use video_manager_shared::{Video, VideoPage, VideoQuery};

use crate::db::DbError;

fn row_to_video(r: &rusqlite::Row<'_>) -> rusqlite::Result<Video> {
    Ok(Video {
        id: r.get(0)?,
        library_id: r.get(1)?,
        path: r.get(2)?,
        filename: r.get(3)?,
        size: r.get(4)?,
        mtime: r.get(5)?,
        duration_ms: r.get(6)?,
        width: r.get(7)?,
        height: r.get(8)?,
        title: r.get(9)?,
        year: r.get(10)?,
        plot: r.get(11)?,
        poster_path: r.get(12)?,
        created_at: r.get(13)?,
        thumb_ready: r.get(14)?,
        sprite_ready: r.get(15)?,
        progress_ms: r.get(16)?,
    })
}

const FILTER: &str = "WHERE (?1 IS NULL OR v.library_id = ?1) \
     AND (?2 IS NULL OR v.filename LIKE ?2 OR v.title LIKE ?2) \
     AND (?3 IS NULL OR EXISTS (SELECT 1 FROM video_tags vt WHERE vt.video_id = v.id AND vt.tag_id = ?3))";

pub fn videos(conn: &Connection, q: &VideoQuery) -> Result<VideoPage, DbError> {
    let order = match q.sort.as_str() {
        "name" => "v.filename COLLATE NOCASE ASC",
        "size" => "v.size DESC",
        "duration" => "v.duration_ms DESC",
        _ => "v.created_at DESC, v.id DESC",
    };
    let like = q
        .text
        .as_ref()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| format!("%{t}%"));
    let page = q.page.max(1);
    let size = q.page_size.clamp(1, 500);
    let offset = (page - 1) * size;

    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM videos v {FILTER}"),
        params![q.library_id, like, q.tag_id],
        |r| r.get(0),
    )?;

    let sql = format!(
        "SELECT v.id, v.library_id, v.path, v.filename, v.size, v.mtime, v.duration_ms, v.width, v.height, \
         v.title, v.year, v.plot, v.poster_path, v.created_at, v.thumb_ready, v.sprite_ready, p.position_ms \
         FROM videos v LEFT JOIN playback_progress p ON p.video_id = v.id \
         {FILTER} ORDER BY {order} LIMIT ?4 OFFSET ?5"
    );
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(params![q.library_id, like, q.tag_id, size, offset], row_to_video)?
        .collect::<Result<_, _>>()?;
    Ok(VideoPage { total, items })
}