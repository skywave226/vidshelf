use std::path::Path;

use rusqlite::Connection;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("目录不存在：{0}")]
    InvalidPath(String),
    #[error("该目录已在媒体库中")]
    Duplicate,
}

const SCHEMA_V1: &str = r#"
CREATE TABLE libraries (
    id          INTEGER PRIMARY KEY,
    path        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE TABLE videos (
    id           INTEGER PRIMARY KEY,
    library_id   INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path         TEXT NOT NULL UNIQUE,
    filename     TEXT NOT NULL,
    size         INTEGER NOT NULL,
    mtime        INTEGER NOT NULL,
    duration_ms  INTEGER,
    width        INTEGER,
    height       INTEGER,
    title        TEXT,
    year         INTEGER,
    plot         TEXT,
    poster_path  TEXT,
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_videos_library ON videos(library_id);

CREATE TABLE playback_progress (
    video_id    INTEGER PRIMARY KEY REFERENCES videos(id) ON DELETE CASCADE,
    position_ms INTEGER NOT NULL DEFAULT 0,
    watched     INTEGER NOT NULL DEFAULT 0,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE jobs (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,
    payload    TEXT NOT NULL,
    status     TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX idx_jobs_status ON jobs(status);
"#;

const SCHEMA_V2: &str = r#"
ALTER TABLE videos ADD COLUMN thumb_ready INTEGER NOT NULL DEFAULT 0;
ALTER TABLE videos ADD COLUMN sprite_ready INTEGER NOT NULL DEFAULT 0;
"#;

const SCHEMA_V3: &str = r#"
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

const SCHEMA_V4: &str = r#"
CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE video_tags (
    video_id INTEGER NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (video_id, tag_id)
);
CREATE INDEX idx_video_tags_tag ON video_tags(tag_id);
"#;

pub fn init(path: &Path) -> Result<Connection, DbError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    if version < 2 {
        conn.execute_batch(SCHEMA_V2)?;
        conn.pragma_update(None, "user_version", 2)?;
    }
    if version < 3 {
        conn.execute_batch(SCHEMA_V3)?;
        conn.pragma_update(None, "user_version", 3)?;
    }
    if version < 4 {
        conn.execute_batch(SCHEMA_V4)?;
        conn.pragma_update(None, "user_version", 4)?;
    }
    Ok(conn)
}

pub fn video_count(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM videos", [], |row| row.get(0))?)
}

pub fn library_count(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM libraries", [], |row| row.get(0))?)
}