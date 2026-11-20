use video_manager_core::{db, metadata, query, rusqlite::params, scraper, settings};
use video_manager_shared::{ScrapeCandidate, Settings, VideoQuery};

fn main() {
    let dir = std::env::temp_dir().join(format!("vm4_verify_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = db::init(&dir.join("test.db")).unwrap();

    let s0 = settings::get(&conn).unwrap();
    assert_eq!(s0, Settings::default());
    let s1 = Settings {
        scrape_enabled: true,
        tmdb_api_key: "k".into(),
        proxy: "http://127.0.0.1:7890".into(),
        media_type: "tv".into(),
        language: "zh-CN".into(),
    };
    settings::save(&conn, &s1).unwrap();
    assert_eq!(settings::get(&conn).unwrap(), s1);
    println!("[verify-m4] settings roundtrip PASS");

    assert!(matches!(
        scraper::TmdbClient::new(&Settings::default()),
        Err(scraper::ScrapeError::Disabled)
    ));
    let nokey = Settings {
        scrape_enabled: true,
        ..Default::default()
    };
    assert!(matches!(
        scraper::TmdbClient::new(&nokey),
        Err(scraper::ScrapeError::NoKey)
    ));
    let badproxy = Settings {
        scrape_enabled: true,
        tmdb_api_key: "k".into(),
        proxy: "not a url".into(),
        ..Default::default()
    };
    assert!(matches!(
        scraper::TmdbClient::new(&badproxy),
        Err(scraper::ScrapeError::BadProxy(_))
    ));
    println!("[verify-m4] scraper gating PASS");

    let sample = r#"{"page":1,"results":[{"id":550,"title":"搏击俱乐部","original_title":"Fight Club","release_date":"1999-10-15","overview":"一个失眠的上班族……","poster_path":"/pB8BM7pdSp6B6Ih7QZ4DrQ3PmJK.jpg","vote_average":8.4},{"id":1,"name":"测试剧","original_name":"Test Show","first_air_date":"2020-01-01","vote_average":7.0}],"total_results":2}"#;
    let cands = scraper::parse_search(sample).unwrap();
    assert_eq!(cands.len(), 2);
    assert_eq!(cands[0].title, "搏击俱乐部");
    assert_eq!(cands[0].year, Some(1999));
    assert_eq!(cands[0].poster_path.as_deref(), Some("/pB8BM7pdSp6B6Ih7QZ4DrQ3PmJK.jpg"));
    assert_eq!(cands[1].title, "测试剧");
    assert_eq!(cands[1].year, Some(2020));
    println!("[verify-m4] tmdb parse PASS");

    conn.execute(
        "INSERT INTO libraries (path, name, created_at) VALUES (?1, 'lib', 0)",
        params![dir.to_string_lossy()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO videos (library_id, path, filename, size, mtime, created_at) \
         VALUES (1, ?1, 'Fight.Club.1999.1080p.mkv', 100, 0, 0)",
        params![dir.join("f.mkv").to_string_lossy()],
    )
    .unwrap();
    let c = ScrapeCandidate {
        source_id: 550,
        title: "搏击俱乐部".into(),
        original_title: "Fight Club".into(),
        year: Some(1999),
        overview: "简介".into(),
        poster_path: Some("/x.jpg".into()),
        rating: 8.4,
        genres: vec!["剧情".into(), "惊悚".into()],
    };
    metadata::apply(&conn, 1, &c, Some("/local/poster.jpg".into())).unwrap();
    assert_eq!(
        metadata::clean_query("Fight.Club.1999.1080p.mkv"),
        "Fight Club 1999 1080p"
    );
    assert_eq!(metadata::clean_query("长视频_续播测试.mp4"), "长视频 续播测试");
    let page = query::videos(
        &conn,
        &VideoQuery {
            library_id: None,
            text: Some("搏击".into()),
            tag_id: None,
            sort: "added".into(),
            page: 1,
            page_size: 100,
        },
    )
    .unwrap();
    assert_eq!(page.total, 1);
    let v = &page.items[0];
    assert_eq!(v.title.as_deref(), Some("搏击俱乐部"));
    assert_eq!(v.year, Some(1999));
    assert_eq!(v.plot.as_deref(), Some("简介"));
    assert_eq!(v.poster_path.as_deref(), Some("/local/poster.jpg"));
    println!("[verify-m4] apply+search PASS");

    std::fs::remove_dir_all(&dir).ok();
    println!("[verify-m4] ALL PASS");
}