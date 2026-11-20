use rusqlite::{params, Connection};
use video_manager_shared::ScrapeCandidate;

use crate::db::DbError;
use crate::tag;

pub fn apply(
    conn: &Connection,
    video_id: i64,
    c: &ScrapeCandidate,
    poster: Option<String>,
) -> Result<(), DbError> {
    conn.execute(
        "UPDATE videos SET title = ?1, year = ?2, plot = ?3, poster_path = ?4 WHERE id = ?5",
        params![
            c.title,
            c.year,
            Some(c.overview.clone()).filter(|s| !s.is_empty()),
            poster,
            video_id
        ],
    )?;
    for genre in &c.genres {
        tag::add_video_tag(conn, video_id, genre)?;
    }
    Ok(())
}

pub fn clean_query(filename: &str) -> String {
    let base = filename.rsplit_once('.').map(|(b, _)| b).unwrap_or(filename);
    base.replace(['_', '.'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}