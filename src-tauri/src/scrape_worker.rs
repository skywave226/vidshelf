use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager};
use video_manager_core::{metadata, scraper, scraper::Scraper, settings};
use video_manager_shared::{ScrapeProgress, ScrapeStats};

use crate::AppState;

static RUNNING: AtomicBool = AtomicBool::new(false);

pub fn start(app: AppHandle) -> Result<(), String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("刮削正在进行中".into());
    }
    let reset = || RUNNING.store(false, Ordering::SeqCst);
    let (settings, pending) = {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|e| {
            reset();
            e.to_string()
        })?;
        let s = settings::get(&conn).map_err(|e| {
            reset();
            e.to_string()
        })?;
        let mut stmt = conn
            .prepare("SELECT id, filename FROM videos WHERE title IS NULL ORDER BY id")
            .map_err(|e| {
                reset();
                e.to_string()
            })?;
        let rows: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| {
                reset();
                e.to_string()
            })?
            .filter_map(|r| r.ok())
            .collect();
        (s, rows)
    };
    let client = scraper::TmdbClient::new(&settings).map_err(|e| {
        reset();
        e.to_string()
    })?;
    if pending.is_empty() {
        reset();
        return Err("没有待刮削的视频（都已刮削过）".into());
    }

    let total = pending.len() as i64;
    println!("[scrape] batch start: {total}");
    let _ = app.emit("scrape-progress", ScrapeProgress { done: 0, total, matched: 0 });
    let thumb_dir = app.state::<AppState>().thumb_dir.clone();

    std::thread::spawn(move || {
        let mut matched = 0i64;
        let mut failed = 0i64;
        for (i, (id, filename)) in pending.iter().enumerate() {
            let query = metadata::clean_query(filename);
            let result: Result<Option<(video_manager_shared::ScrapeCandidate, Option<String>)>, scraper::ScrapeError> =
                tauri::async_runtime::block_on(async {
                    let Some(c) = client.search(&query).await?.into_iter().next() else {
                        return Ok(None);
                    };
                    let mut poster = c.poster_path.clone();
                    if let Some(p) = c.poster_path.clone() {
                        let dest = std::path::Path::new(&thumb_dir).join(format!("{id}_poster.jpg"));
                        if client.fetch_poster(&p, &dest).await.is_ok() {
                            poster = Some(dest.to_string_lossy().into_owned());
                        }
                    }
                    Ok(Some((c, poster)))
                });
            match result {
                Ok(Some((c, poster))) => {
                    let state = app.state::<AppState>();
                    let ok = state
                        .db
                        .lock()
                        .ok()
                        .map(|conn| metadata::apply(&conn, *id, &c, poster).is_ok())
                        .unwrap_or(false);
                    if ok {
                        matched += 1;
                    } else {
                        failed += 1;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    failed += 1;
                    println!("[scrape] batch item failed: id={id} {e}");
                }
            }
            let d = (i + 1) as i64;
            if d % 5 == 0 || d == total {
                let _ = app.emit("scrape-progress", ScrapeProgress { done: d, total, matched });
            }
        }
        println!("[scrape] batch done: total={total} matched={matched} failed={failed}");
        let _ = app.emit("scrape-done", ScrapeStats { total, matched, failed });
        RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(())
}