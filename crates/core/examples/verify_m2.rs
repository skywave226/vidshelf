use video_manager_core::{db, library, probe, scan, thumb};

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata");
    if !root.is_dir() {
        println!("testdata 不存在，跳过验证");
        return;
    }
    let stamp = video_manager_core::unix_now();
    let db_path = std::env::temp_dir().join(format!("vm-verify-m2-{stamp}.db"));
    let out_dir = std::env::temp_dir().join(format!("vm-verify-m2-{stamp}"));
    std::fs::create_dir_all(&out_dir).unwrap();

    let conn = db::init(&db_path).unwrap();
    let lib = library::add(&conn, &root.to_string_lossy()).unwrap();
    scan::sync(&conn, lib.id, &scan::walk(&root)).unwrap();

    let mut n = 0;
    for (id, path) in scan::pending_probe(&conn, lib.id).unwrap() {
        let info = probe::probe(std::path::Path::new(&path)).expect("ffprobe 失败");
        scan::save_probe(&conn, id, Some(&info)).unwrap();
        let video = std::path::Path::new(&path);

        let thumb_out = out_dir.join(format!("{id}.jpg"));
        assert!(
            thumb::generate_thumb(video, info.duration_ms, &thumb_out),
            "缩略图生成失败: {path}"
        );
        let tlen = std::fs::metadata(&thumb_out).unwrap().len();
        assert!(tlen > 1000, "缩略图过小: {tlen}");

        let sprite_out = out_dir.join(format!("{id}-sprite.jpg"));
        assert!(
            thumb::generate_sprite(video, info.duration_ms, &sprite_out),
            "精灵图生成失败: {path}"
        );
        let slen = std::fs::metadata(&sprite_out).unwrap().len();
        assert!(slen > 1000, "精灵图过小: {slen}");

        println!("thumb+sprite: {} ({}ms) -> {}B / {}B", path, info.duration_ms, tlen, slen);
        n += 1;
    }
    assert_eq!(n, 5);

    drop(conn);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_dir_all(&out_dir);
    println!("VERIFY OK");
}