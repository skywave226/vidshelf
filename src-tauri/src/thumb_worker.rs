use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};
use video_manager_core::{rusqlite::params, thumb};
use video_manager_shared::ThumbProgress;

use crate::AppState;

static RUNNING: AtomicBool = AtomicBool::new(false);

pub fn start(app: AppHandle) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let pending: Vec<(i64, String, i64)> = {
        let state = app.state::<AppState>();
        let Ok(conn) = state.db.lock() else {
            RUNNING.store(false, Ordering::SeqCst);
            return;
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, path, COALESCE(duration_ms, 0) FROM videos WHERE thumb_ready = 0 ORDER BY id",
        ) else {
            RUNNING.store(false, Ordering::SeqCst);
            return;
        };
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        rows
    };
    if pending.is_empty() {
        RUNNING.store(false, Ordering::SeqCst);
        return;
    }

    let total = pending.len() as i64;
    println!("[thumb] start: {total} pending");
    let _ = app.emit("thumb-progress", ThumbProgress { done: 0, total });

    let queue = Arc::new(Mutex::new(VecDeque::from(pending)));
    let done = Arc::new(AtomicI64::new(0));
    let thumb_dir = app.state::<AppState>().thumb_dir.clone();

    // 单并发即可：ffmpeg 已限制单线程，避免后台生成把 CPU 占满
    for _ in 0..1 {
        let app = app.clone();
        let queue = queue.clone();
        let done = done.clone();
        let thumb_dir = thumb_dir.clone();
        std::thread::spawn(move || loop {
            let item = queue.lock().unwrap().pop_front();
            let Some((id, path, duration_ms)) = item else { break };
            let out = std::path::Path::new(&thumb_dir).join(format!("{id}.jpg"));
            let ok = thumb::generate_thumb(std::path::Path::new(&path), duration_ms, &out);
            {
                let state = app.state::<AppState>();
                let locked = state.db.lock();
                if let Ok(conn) = locked {
                    let _ = conn.execute(
                        "UPDATE videos SET thumb_ready = ?1 WHERE id = ?2",
                        params![if ok { 1 } else { 2 }, id],
                    );
                }
            }
            let d = done.fetch_add(1, Ordering::SeqCst) + 1;
            if d % 5 == 0 || d == total {
                let _ = app.emit("thumb-progress", ThumbProgress { done: d, total });
            }
            if d == total {
                println!("[thumb] done: {total}");
                RUNNING.store(false, Ordering::SeqCst);
                start(app.clone());
            }
        });
    }
}