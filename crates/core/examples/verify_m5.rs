use video_manager_core::rusqlite::{params, Connection};
use video_manager_core::{db, query, tag};
use video_manager_shared::VideoQuery;

fn insert_video(conn: &Connection, lib: i64, path: &str, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO videos (library_id, path, filename, size, mtime, created_at) \
         VALUES (?1, ?2, ?3, 0, 0, 0)",
        params![lib, path, name],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn tag_id(conn: &Connection, name: &str) -> i64 {
    conn.query_row("SELECT id FROM tags WHERE name = ?1", params![name], |r| r.get(0))
        .unwrap()
}

fn names(tags: &[video_manager_shared::Tag]) -> Vec<String> {
    tags.iter().map(|t| t.name.clone()).collect()
}

fn q(tag: Option<i64>, text: Option<&str>) -> VideoQuery {
    VideoQuery {
        library_id: None,
        text: text.map(|s| s.to_string()),
        tag_id: tag,
        sort: "added".into(),
        page: 1,
        page_size: 100,
    }
}

fn ids(page: &video_manager_shared::VideoPage) -> Vec<i64> {
    let mut v: Vec<i64> = page.items.iter().map(|x| x.id).collect();
    v.sort();
    v
}

fn main() {
    let dir = std::env::temp_dir().join(format!("vmtag_verify_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = db::init(&dir.join("test.db")).unwrap();

    let version: i32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 4);
    println!("[verify-m5] schema user_version=4 PASS");

    conn.execute(
        "INSERT INTO libraries (path, name, created_at) VALUES ('C:/lib', 'lib', 0)",
        [],
    )
    .unwrap();
    let lib = conn.last_insert_rowid();
    let v1 = insert_video(&conn, lib, "C:/lib/a.mkv", "a.mkv");
    let v2 = insert_video(&conn, lib, "C:/lib/b.mkv", "b.mkv");
    let v3 = insert_video(&conn, lib, "C:/lib/c.mkv", "c.mkv");

    assert!(tag::list(&conn).unwrap().is_empty());
    assert!(tag::for_video(&conn, v1).unwrap().is_empty());
    println!("[verify-m5] empty initial state PASS");

    let t = tag::add_video_tag(&conn, v1, "动作").unwrap();
    assert_eq!(names(&t), vec!["动作"]);
    let t = tag::add_video_tag(&conn, v1, "科幻").unwrap();
    assert_eq!(names(&t), vec!["动作", "科幻"]);
    tag::add_video_tag(&conn, v2, "动作").unwrap();
    println!("[verify-m5] add tags PASS");

    // idempotent re-add + whitespace no-op
    let t = tag::add_video_tag(&conn, v1, "动作").unwrap();
    assert_eq!(t.len(), 2);
    let t = tag::add_video_tag(&conn, v1, "   ").unwrap();
    assert_eq!(t.len(), 2);
    let all = tag::list(&conn).unwrap();
    let action = all.iter().find(|x| x.name == "动作").unwrap();
    assert_eq!(action.count, 2);
    println!("[verify-m5] idempotent add + whitespace no-op + counts PASS");

    // query filtering by tag
    let action_id = tag_id(&conn, "动作");
    let scifi_id = tag_id(&conn, "科幻");
    let p = query::videos(&conn, &q(Some(action_id), None)).unwrap();
    assert_eq!(p.total, 2);
    assert_eq!(ids(&p), vec![v1, v2]);
    assert!(!ids(&p).contains(&v3));
    let p = query::videos(&conn, &q(Some(scifi_id), None)).unwrap();
    assert_eq!(p.total, 1);
    assert_eq!(ids(&p), vec![v1]);
    let p = query::videos(&conn, &q(None, None)).unwrap();
    assert_eq!(p.total, 3);
    println!("[verify-m5] query tag filter PASS");

    // combined tag + text filter
    let p = query::videos(&conn, &q(Some(action_id), Some("a.mkv"))).unwrap();
    assert_eq!(p.total, 1);
    assert_eq!(ids(&p), vec![v1]);
    println!("[verify-m5] combined tag+text filter PASS");

    // remove association keeps tag while still referenced elsewhere
    let t = tag::remove_video_tag(&conn, v1, action_id).unwrap();
    assert_eq!(names(&t), vec!["科幻"]);
    assert!(tag::list(&conn).unwrap().iter().any(|x| x.name == "动作"));
    let p = query::videos(&conn, &q(Some(action_id), None)).unwrap();
    assert_eq!(ids(&p), vec![v2]);
    println!("[verify-m5] remove association (tag survives) PASS");

    // removing last reference orphan-cleans the tag
    tag::remove_video_tag(&conn, v2, action_id).unwrap();
    assert!(!tag::list(&conn).unwrap().iter().any(|x| x.name == "动作"));
    let t = tag::remove_video_tag(&conn, v1, scifi_id).unwrap();
    assert!(t.is_empty());
    assert!(tag::list(&conn).unwrap().is_empty());
    let p = query::videos(&conn, &q(None, None)).unwrap();
    assert_eq!(p.total, 3);
    println!("[verify-m5] orphan cleanup + videos intact PASS");

    println!("[verify-m5] ALL PASS");
}