use video_manager_core::{db, library, probe, query, scan};
use video_manager_shared::VideoQuery;

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata");
    if !root.is_dir() {
        println!("testdata 不存在，跳过验证");
        return;
    }
    let db_path = std::env::temp_dir().join(format!("vm-verify-{}.db", video_manager_core::unix_now()));
    let conn = db::init(&db_path).unwrap();
    let lib = library::add(&conn, &root.to_string_lossy()).unwrap();

    let entries = scan::walk(&root);
    println!("walk: {} 个视频文件", entries.len());
    let stats = scan::sync(&conn, lib.id, &entries).unwrap();
    println!("sync: added={} updated={} removed={}", stats.added, stats.updated, stats.removed);
    assert_eq!(stats.added, 5);

    for (id, path) in scan::pending_probe(&conn, lib.id).unwrap() {
        let info = probe::probe(std::path::Path::new(&path));
        scan::save_probe(&conn, id, info.as_ref()).unwrap();
        match &info {
            Some(p) => println!("probe: {} -> {}ms {}x{:?}", path, p.duration_ms, p.width.unwrap_or(0), p.height),
            None => println!("probe: {} -> 失败", path),
        }
        assert!(info.is_some(), "ffprobe 解析失败: {path}");
    }

    let all = query::videos(&conn, &VideoQuery { library_id: None, text: None, tag_id: None, sort: "added".into(), page: 1, page_size: 100 }).unwrap();
    println!("query all: total={}", all.total);
    for v in &all.items {
        println!("  {} | {:?}ms | {:?}x{:?} | {} bytes", v.filename, v.duration_ms, v.width, v.height, v.size);
    }
    assert_eq!(all.total, 5);
    assert!(all.items.iter().all(|v| v.duration_ms.unwrap_or(-1) > 0));

    let found = query::videos(&conn, &VideoQuery { library_id: Some(lib.id), text: Some("纪录片".into()), tag_id: None, sort: "name".into(), page: 1, page_size: 100 }).unwrap();
    println!("search 纪录片: total={}", found.total);
    assert_eq!(found.total, 1);

    let by_size = query::videos(&conn, &VideoQuery { library_id: None, text: None, tag_id: None, sort: "size".into(), page: 1, page_size: 2 }).unwrap();
    assert_eq!(by_size.items.len(), 2);
    assert!(by_size.items[0].size >= by_size.items[1].size);
    println!("sort+分页: OK");

    drop(conn);
    let _ = std::fs::remove_file(&db_path);
    println!("VERIFY OK");
}