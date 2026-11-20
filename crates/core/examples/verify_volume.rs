use video_manager_core::{db, settings};
use video_manager_shared::Settings;

fn main() {
    let dir = std::env::temp_dir().join(format!("vmvol_verify_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = db::init(&dir.join("test.db")).unwrap();

    assert_eq!(settings::get_player_volume(&conn), (100.0, false));
    println!("[verify-vol] default (100,false) PASS");

    settings::save_player_volume(&conn, 40.0, true).unwrap();
    assert_eq!(settings::get_player_volume(&conn), (40.0, true));
    println!("[verify-vol] roundtrip (40,true) PASS");

    settings::save_player_volume(&conn, 0.0, false).unwrap();
    assert_eq!(settings::get_player_volume(&conn), (0.0, false));
    println!("[verify-vol] roundtrip (0,false) PASS");

    settings::save_player_volume(&conn, 73.5, false).unwrap();
    assert_eq!(settings::get_player_volume(&conn), (73.5, false));
    println!("[verify-vol] fractional (73.5,false) PASS");

    let mut s = Settings::default();
    s.scrape_enabled = true;
    s.tmdb_api_key = "k".into();
    settings::save(&conn, &s).unwrap();
    assert_eq!(settings::get_player_volume(&conn), (73.5, false));
    assert_eq!(settings::get(&conn).unwrap(), s);
    println!("[verify-vol] independent of scrape settings PASS");

    println!("[verify-vol] ALL PASS");
}