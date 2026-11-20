use video_manager_core::{db, library, progress, query, scan};
use video_manager_shared::VideoQuery;

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata");
    if !root.is_dir() {
        println!("testdata 不存在，跳过验证");
        return;
    }
    let db_path = std::env::temp_dir().join(format!("vm-verify-m3-{}.db", video_manager_core::unix_now()));
    let conn = db::init(&db_path).unwrap();
    let lib = library::add(&conn, &root.to_string_lossy()).unwrap();
    scan::sync(&conn, lib.id, &scan::walk(&root)).unwrap();

    let all = query::videos(&conn, &VideoQuery { library_id: None, text: None, tag_id: None, sort: "added".into(), page: 1, page_size: 100 }).unwrap();
    let v = &all.items[0];
    let dur = 60_000i64;
    conn.execute("UPDATE videos SET duration_ms = ?1 WHERE id = ?2", video_manager_core::rusqlite::params![dur, v.id]).unwrap();

    assert!(progress::get(&conn, v.id).unwrap().is_none());

    progress::save(&conn, v.id, 30_000).unwrap();
    let p = progress::get(&conn, v.id).unwrap().unwrap();
    assert_eq!((p.position_ms, p.watched), (30_000, 0));
    println!("save/get: {}ms watched={}", p.position_ms, p.watched);

    progress::save(&conn, v.id, 58_000).unwrap();
    let p = progress::get(&conn, v.id).unwrap().unwrap();
    assert_eq!((p.position_ms, p.watched), (58_000, 1));
    println!(">=95% 自动标记 watched: OK");

    let joined = query::videos(&conn, &VideoQuery { library_id: None, text: None, tag_id: None, sort: "added".into(), page: 1, page_size: 100 }).unwrap();
    let me = joined.items.iter().find(|x| x.id == v.id).unwrap();
    assert_eq!(me.progress_ms, Some(58_000));
    assert!(joined.items.iter().filter(|x| x.id != v.id).all(|x| x.progress_ms.is_none()));
    println!("查询 LEFT JOIN 进度: OK");

    let found = query::videos(&conn, &VideoQuery { library_id: Some(lib.id), text: Some("纪录片".into()), tag_id: None, sort: "name".into(), page: 1, page_size: 100 }).unwrap();
    assert_eq!(found.total, 1);
    println!("join 后搜索/过滤: OK");

    drop(conn);
    let _ = std::fs::remove_file(&db_path);
    println!("VERIFY OK");
}