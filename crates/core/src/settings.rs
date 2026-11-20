use rusqlite::{params, Connection, OptionalExtension};
use video_manager_shared::Settings;

use crate::db::DbError;

pub fn get(conn: &Connection) -> Result<Settings, DbError> {
    let json: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| r.get(0))
        .optional()?;
    Ok(json
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default())
}

pub fn save(conn: &Connection, settings: &Settings) -> Result<(), DbError> {
    let json = serde_json::to_string(settings)?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('app', ?1) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![json],
    )?;
    Ok(())
}

// 播放器音量状态独立存于 key='player'，与刮削设置（key='app'）解耦，跨视频/重启统一
pub fn get_player_volume(conn: &Connection) -> (f64, bool) {
    let json: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'player'", [], |r| r.get(0))
        .optional()
        .unwrap_or(None);
    json.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| {
            (
                v.get("volume").and_then(|x| x.as_f64()).unwrap_or(100.0),
                v.get("mute").and_then(|x| x.as_bool()).unwrap_or(false),
            )
        })
        .unwrap_or((100.0, false))
}

pub fn save_player_volume(conn: &Connection, volume: f64, mute: bool) -> Result<(), DbError> {
    let json = serde_json::json!({ "volume": volume, "mute": mute }).to_string();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('player', ?1) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![json],
    )?;
    Ok(())
}