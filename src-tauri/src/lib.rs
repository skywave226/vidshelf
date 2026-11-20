mod commands;
#[cfg(feature = "libmpv")]
mod mpv;
mod scan;
mod scrape_worker;
mod thumb_worker;

use std::path::Path;
use std::sync::atomic::AtomicBool;
#[cfg(feature = "libmpv")]
use std::sync::Arc;
use std::sync::Mutex;

use tauri::Manager;
use video_manager_core::{db, rusqlite::Connection};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub db_path: String,
    pub thumb_dir: String,
    pub scanning: AtomicBool,
    #[cfg(feature = "libmpv")]
    pub mpv_api: Option<Arc<mpv::MpvApi>>,
    pub mpv_version: Option<String>,
    #[cfg(feature = "libmpv")]
    pub mpv: Mutex<Option<Arc<mpv::MpvSession>>>,
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let exe_dir = std::env::current_exe()?
                .parent()
                .map(Path::to_path_buf)
                .ok_or("无法定位程序所在目录")?;
            let dir = exe_dir.join("data");
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("library.db");
            if !db_path.exists() {
                let old = app.path().app_data_dir()?;
                if old.join("library.db").exists() {
                    for s in ["", "-wal", "-shm"] {
                        let src = old.join(format!("library.db{s}"));
                        if src.exists() {
                            std::fs::copy(&src, dir.join(format!("library.db{s}")))?;
                        }
                    }
                    let old_thumbs = old.join("thumbnails");
                    if old_thumbs.is_dir() {
                        copy_dir_all(&old_thumbs, &dir.join("thumbnails"))?;
                    }
                    println!("[setup] 已从旧目录迁移数据：{} -> {}", old.display(), dir.display());
                }
            }
            let conn = db::init(&db_path)?;
            let thumb_dir = dir.join("thumbnails");
            std::fs::create_dir_all(&thumb_dir)?;
            app.asset_protocol_scope().allow_directory(&thumb_dir, true)?;
            for lib in video_manager_core::library::list(&conn)? {
                app.asset_protocol_scope().allow_directory(&lib.path, true)?;
            }
            println!("[setup] database ready: {}", db_path.display());
            #[cfg(feature = "libmpv")]
            let (mpv_api, mpv_version) = mpv::init_api(app.handle());
            #[cfg(not(feature = "libmpv"))]
            let mpv_version: Option<String> = None;
            app.manage(AppState {
                db: Mutex::new(conn),
                db_path: db_path.to_string_lossy().into_owned(),
                thumb_dir: thumb_dir.to_string_lossy().into_owned(),
                scanning: AtomicBool::new(false),
                #[cfg(feature = "libmpv")]
                mpv_api,
                mpv_version,
                #[cfg(feature = "libmpv")]
                mpv: Mutex::new(None),
            });
            thumb_worker::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_libraries,
            commands::add_library,
            commands::remove_library,
            commands::pick_folder,
            commands::scan_library,
            commands::list_videos,
            commands::ensure_sprite,
            commands::save_progress,
            commands::get_progress,
            commands::open_external,
            commands::get_settings,
            commands::save_settings,
            commands::scrape_search,
            commands::scrape_apply,
            commands::scrape_all,
            commands::auto_tag_all,
            commands::list_tags,
            commands::get_video_tags,
            commands::add_video_tag,
            commands::remove_video_tag,
            commands::get_player_volume,
            commands::save_player_volume,
            #[cfg(feature = "libmpv")]
            mpv::mpv_open,
            #[cfg(feature = "libmpv")]
            mpv::mpv_rect,
            #[cfg(feature = "libmpv")]
            mpv::mpv_close,
            #[cfg(feature = "libmpv")]
            mpv::mpv_command,
            #[cfg(feature = "libmpv")]
            mpv::mpv_fullscreen,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");
}