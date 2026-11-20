use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager};
use video_manager_core::{autotag, library, probe, scan};
use video_manager_shared::{ScanProgress, ScanStats};

use crate::AppState;

pub fn start(app: AppHandle, library_id: Option<i64>) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Err("已有扫描任务在进行".into());
    }
    std::thread::spawn(move || {
        let result = run(&app, library_id);
        app.state::<AppState>().scanning.store(false, Ordering::SeqCst);
        match result {
            Ok(stats) => {
                println!("[scan] done: {stats:?}");
                let _ = app.emit("scan-done", stats);
            }
            Err(e) => {
                println!("[scan] error: {e}");
                let _ = app.emit("scan-error", e);
            }
        }
    });
    Ok(())
}

fn run(app: &AppHandle, library_id: Option<i64>) -> Result<ScanStats, String> {
    let libs = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        match library_id {
            Some(id) => vec![library::get(&conn, id)
                .map_err(|e| e.to_string())?
                .ok_or("媒体库不存在")?],
            None => library::list(&conn).map_err(|e| e.to_string())?,
        }
    };

    let mut total = ScanStats::default();
    for lib in libs {
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                library_id: lib.id,
                library_name: lib.name.clone(),
                phase: "walking".into(),
                done: 0,
                total: 0,
            },
        );
        let entries = scan::walk(std::path::Path::new(&lib.path));
        let sync_stats = {
            let state = app.state::<AppState>();
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            scan::sync(&conn, lib.id, &entries).map_err(|e| e.to_string())?
        };
        total.added += sync_stats.added;
        total.updated += sync_stats.updated;
        total.removed += sync_stats.removed;

        let pending = {
            let state = app.state::<AppState>();
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            scan::pending_probe(&conn, lib.id).map_err(|e| e.to_string())?
        };
        let n = pending.len();
        for (i, (id, path)) in pending.into_iter().enumerate() {
            let info = probe::probe(std::path::Path::new(&path));
            {
                let state = app.state::<AppState>();
                let conn = state.db.lock().map_err(|e| e.to_string())?;
                scan::save_probe(&conn, id, info.as_ref()).map_err(|e| e.to_string())?;
            }
            if info.is_some() {
                total.probed += 1;
            } else {
                total.probe_failed += 1;
            }
            if (i + 1) % 10 == 0 || i + 1 == n {
                let _ = app.emit(
                    "scan-progress",
                    ScanProgress {
                        library_id: lib.id,
                        library_name: lib.name.clone(),
                        phase: "probing".into(),
                        done: (i + 1) as i64,
                        total: n as i64,
                    },
                );
            }
        }

        if !sync_stats.added_ids.is_empty() {
            let state = app.state::<AppState>();
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            for id in &sync_stats.added_ids {
                if let Err(e) = autotag::apply_video(&conn, *id) {
                    println!("[scan] auto tag failed: id={id} {e}");
                }
            }
        }
    }
    crate::thumb_worker::start(app.clone());
    Ok(total)
}