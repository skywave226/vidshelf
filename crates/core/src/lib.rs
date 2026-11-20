pub mod autotag;
pub mod db;
pub mod library;
pub mod media;
pub mod metadata;
pub mod probe;
pub mod progress;
pub mod query;
pub mod scan;
pub mod scraper;
pub mod settings;
pub mod tag;
pub mod thumb;

pub use rusqlite;

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}