use tauri::{AppHandle, Manager, State};
use video_manager_core::{
    autotag, db, library, media, progress, query, rusqlite::params, scraper, scraper::Scraper,
    settings, tag, thumb,
};
use video_manager_shared::{
    AppInfo, AutoTagStats, Library, PlaybackProgress, ScrapeCandidate, Settings, Tag, VideoPage,
    VideoQuery,
};

use crate::AppState;

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> Result<AppInfo, String> {
    println!("[ipc] app_info invoked from frontend");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(AppInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        db_path: state.db_path.clone(),
        thumb_dir: state.thumb_dir.clone(),
        video_count: db::video_count(&conn).map_err(|e| e.to_string())?,
        library_count: db::library_count(&conn).map_err(|e| e.to_string())?,
        ffmpeg_version: media::tool_version("ffmpeg"),
        mpv_version: state.mpv_version.clone(),
    })
}

#[tauri::command]
pub fn list_libraries(state: State<'_, AppState>) -> Result<Vec<Library>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_library(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<Library, String> {
    println!("[ipc] add_library: {path}");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let lib = library::add(&conn, &path).map_err(|e| e.to_string())?;
    app.asset_protocol_scope()
        .allow_directory(&path, true)
        .map_err(|e| e.to_string())?;
    Ok(lib)
}

#[tauri::command]
pub fn remove_library(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    println!("[ipc] remove_library: {id}");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    library::remove(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pick_folder(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let folder = app.dialog().file().blocking_pick_folder()?;
    let path = folder.into_path().ok()?;
    Some(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn scan_library(app: AppHandle, library_id: Option<i64>) -> Result<(), String> {
    println!("[ipc] scan_library: {library_id:?}");
    crate::scan::start(app, library_id)
}

#[tauri::command]
pub fn list_videos(state: State<'_, AppState>, query: VideoQuery) -> Result<VideoPage, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    query::videos(&conn, &query).map_err(|e| e.to_string())
}

fn sprite_busy() -> &'static std::sync::atomic::AtomicBool {
    static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    &BUSY
}

#[tauri::command]
pub async fn ensure_sprite(
    state: State<'_, AppState>,
    video_id: i64,
) -> Result<String, String> {
    let (path, duration_ms, sprite_ready) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT path, COALESCE(duration_ms, 0), sprite_ready FROM videos WHERE id = ?1",
            params![video_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
        )
        .map_err(|e| e.to_string())?
    };
    let out = std::path::Path::new(&state.thumb_dir).join(format!("{video_id}_sprite.jpg"));
    if sprite_ready == 1 && out.exists() {
        return Ok(out.to_string_lossy().into_owned());
    }

    // 同一时刻只允许一个雪碧图任务，避免快速划过多张卡片时叠加 CPU
    use std::sync::atomic::Ordering;
    if sprite_busy().swap(true, Ordering::SeqCst) {
        return Err("预览图生成中".into());
    }
    let video = path.clone();
    let dest = out.clone();
    let joined = tauri::async_runtime::spawn_blocking(move || {
        thumb::generate_sprite(std::path::Path::new(&video), duration_ms, &dest)
    })
    .await;
    sprite_busy().store(false, Ordering::SeqCst);
    let ok = matches!(joined, Ok(true));

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE videos SET sprite_ready = ?1 WHERE id = ?2",
            params![if ok { 1 } else { 2 }, video_id],
        )
        .map_err(|e| e.to_string())?;
    }
    if ok {
        Ok(out.to_string_lossy().into_owned())
    } else {
        Err("预览图生成失败".into())
    }
}

#[tauri::command]
pub fn save_progress(state: State<'_, AppState>, video_id: i64, position_ms: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    progress::save(&conn, video_id, position_ms).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_progress(state: State<'_, AppState>, video_id: i64) -> Result<Option<PlaybackProgress>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    progress::get(&conn, video_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    settings::get(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    println!(
        "[settings] saved: enabled={} type={} proxy={} key_len={}",
        settings.scrape_enabled,
        settings.media_type,
        settings.proxy,
        settings.tmdb_api_key.trim().len()
    );
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    settings::save(&conn, &settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_player_volume(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let (volume, mute) = settings::get_player_volume(&conn);
    println!("[ipc] get_player_volume: vol={volume} mute={mute}");
    Ok(serde_json::json!({ "volume": volume, "mute": mute }))
}

#[tauri::command]
pub fn save_player_volume(state: State<'_, AppState>, volume: f64, mute: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    settings::save_player_volume(&conn, volume, mute).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scrape_search(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<ScrapeCandidate>, String> {
    println!("[scrape] search: {query}");
    let settings = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        settings::get(&conn).map_err(|e| e.to_string())?
    };
    let client = scraper::TmdbClient::new(&settings).map_err(|e| e.to_string())?;
    let results = client.search(&query).await.map_err(|e| e.to_string())?;
    println!("[scrape] results: {}", results.len());
    Ok(results)
}

#[tauri::command]
pub async fn scrape_apply(
    state: State<'_, AppState>,
    video_id: i64,
    candidate: ScrapeCandidate,
) -> Result<(), String> {
    println!("[scrape] apply: id={video_id} title={}", candidate.title);
    let settings = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        settings::get(&conn).map_err(|e| e.to_string())?
    };
    let mut poster = candidate.poster_path.clone();
    if settings.scrape_enabled {
        if let (Some(p), Ok(client)) = (candidate.poster_path.clone(), scraper::TmdbClient::new(&settings)) {
            let dest = std::path::Path::new(&state.thumb_dir).join(format!("{video_id}_poster.jpg"));
            match client.fetch_poster(&p, &dest).await {
                Ok(()) => {
                    println!("[scrape] poster saved: id={video_id}");
                    poster = Some(dest.to_string_lossy().into_owned());
                }
                Err(e) => println!("[scrape] poster download failed: id={video_id} {e}"),
            }
        }
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    video_manager_core::metadata::apply(&conn, video_id, &candidate, poster).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scrape_all(app: AppHandle) -> Result<(), String> {
    println!("[ipc] scrape_all");
    crate::scrape_worker::start(app)
}

#[tauri::command]
pub fn open_external(app: AppHandle, state: State<'_, AppState>, video_id: i64) -> Result<(), String> {
    let path = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        conn.query_row(
            "SELECT path FROM videos WHERE id = ?1",
            params![video_id],
            |r| r.get::<_, String>(0),
        )
        .map_err(|_| "视频不存在".to_string())?
    };
    println!("[ipc] open_external: {path}");
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn auto_tag_all(state: State<'_, AppState>) -> Result<AutoTagStats, String> {
    println!("[ipc] auto_tag_all");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    autotag::apply_all(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_tags(state: State<'_, AppState>) -> Result<Vec<Tag>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    tag::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_video_tags(state: State<'_, AppState>, video_id: i64) -> Result<Vec<Tag>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    tag::for_video(&conn, video_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_video_tag(state: State<'_, AppState>, video_id: i64, name: String) -> Result<Vec<Tag>, String> {
    println!("[ipc] add_video_tag: id={video_id} name={name}");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    tag::add_video_tag(&conn, video_id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_video_tag(state: State<'_, AppState>, video_id: i64, tag_id: i64) -> Result<Vec<Tag>, String> {
    println!("[ipc] remove_video_tag: id={video_id} tag={tag_id}");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    tag::remove_video_tag(&conn, video_id, tag_id).map_err(|e| e.to_string())
}