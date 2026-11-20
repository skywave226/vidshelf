use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppInfo {
    pub app_version: String,
    pub db_path: String,
    pub thumb_dir: String,
    pub video_count: i64,
    pub library_count: i64,
    pub ffmpeg_version: Option<String>,
    pub mpv_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Video {
    pub id: i64,
    pub library_id: i64,
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub mtime: i64,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub title: Option<String>,
    pub year: Option<i32>,
    pub plot: Option<String>,
    pub poster_path: Option<String>,
    pub created_at: i64,
    pub thumb_ready: i64,
    pub sprite_ready: i64,
    pub progress_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub scrape_enabled: bool,
    pub tmdb_api_key: String,
    pub proxy: String,
    pub media_type: String,
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            scrape_enabled: false,
            tmdb_api_key: String::new(),
            proxy: String::new(),
            media_type: "movie".into(),
            language: "zh-CN".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeCandidate {
    pub source_id: i64,
    pub title: String,
    pub original_title: String,
    pub year: Option<i32>,
    pub overview: String,
    pub poster_path: Option<String>,
    pub rating: f64,
    #[serde(default)]
    pub genres: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackProgress {
    pub position_ms: i64,
    pub watched: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoQuery {
    pub library_id: Option<i64>,
    pub text: Option<String>,
    pub tag_id: Option<i64>,
    pub sort: String,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoPage {
    pub total: i64,
    pub items: Vec<Video>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanProgress {
    pub library_id: i64,
    pub library_name: String,
    pub phase: String,
    pub done: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScanStats {
    pub added: i64,
    pub updated: i64,
    pub removed: i64,
    pub probed: i64,
    pub probe_failed: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThumbProgress {
    pub done: i64,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrapeProgress {
    pub done: i64,
    pub total: i64,
    pub matched: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrapeStats {
    pub total: i64,
    pub matched: i64,
    pub failed: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoTagStats {
    pub videos: i64,
    pub tags_added: i64,
}