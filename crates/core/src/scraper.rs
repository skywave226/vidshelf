use serde::Deserialize;
use video_manager_shared::{ScrapeCandidate, Settings};

#[derive(Debug, thiserror::Error)]
pub enum ScrapeError {
    #[error("纯本地模式：未启用刮削（设置中开启）")]
    Disabled,
    #[error("未配置 TMDB API key（设置中填写）")]
    NoKey,
    #[error("代理地址无效：{0}")]
    BadProxy(String),
    #[error("网络请求失败：{0}")]
    Http(String),
    #[error("TMDB 响应解析失败：{0}")]
    Parse(String),
}

pub trait Scraper {
    fn search(
        &self,
        query: &str,
    ) -> impl std::future::Future<Output = Result<Vec<ScrapeCandidate>, ScrapeError>> + Send;

    fn fetch_poster(
        &self,
        poster_path: &str,
        dest: &std::path::Path,
    ) -> impl std::future::Future<Output = Result<(), ScrapeError>> + Send;
}

pub struct TmdbClient {
    http: reqwest::Client,
    api_key: String,
    media_type: String,
    language: String,
}

impl TmdbClient {
    pub fn new(settings: &Settings) -> Result<Self, ScrapeError> {
        if !settings.scrape_enabled {
            return Err(ScrapeError::Disabled);
        }
        let api_key = settings.tmdb_api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(ScrapeError::NoKey);
        }
        let mut builder = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(20));
        let proxy = settings.proxy.trim();
        if !proxy.is_empty() {
            builder = builder.proxy(
                reqwest::Proxy::all(proxy).map_err(|e| ScrapeError::BadProxy(e.to_string()))?,
            );
        }
        Ok(Self {
            http: builder.build().map_err(|e| ScrapeError::Http(e.to_string()))?,
            api_key,
            media_type: if settings.media_type == "tv" { "tv" } else { "movie" }.into(),
            language: settings.language.clone(),
        })
    }
}

impl Scraper for TmdbClient {
    async fn search(&self, query: &str) -> Result<Vec<ScrapeCandidate>, ScrapeError> {
        let url = format!("https://api.themoviedb.org/3/search/{}", self.media_type);
        let mut req = self.http.get(&url).query(&[
            ("query", query),
            ("language", self.language.as_str()),
            ("include_adult", "false"),
            ("page", "1"),
        ]);
        // v4 read token 是 JWT（eyJ 前缀），走 Bearer；v3 key 走查询参数
        if self.api_key.starts_with("eyJ") {
            req = req.bearer_auth(&self.api_key);
        } else {
            req = req.query(&[("api_key", self.api_key.as_str())]);
        }
        let resp = req.send().await.map_err(|e| ScrapeError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ScrapeError::Http(e.to_string()))?;
        if !status.is_success() {
            let snippet: String = body.chars().take(200).collect();
            return Err(ScrapeError::Http(format!("TMDB HTTP {status}：{snippet}")));
        }
        parse_search(&body)
    }

    async fn fetch_poster(
        &self,
        poster_path: &str,
        dest: &std::path::Path,
    ) -> Result<(), ScrapeError> {
        let url = format!("https://image.tmdb.org/t/p/w342{poster_path}");
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ScrapeError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ScrapeError::Http(format!("poster HTTP {}", resp.status())));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ScrapeError::Http(e.to_string()))?;
        std::fs::write(dest, &bytes).map_err(|e| ScrapeError::Http(format!("写入海报失败：{e}")))?;
        Ok(())
    }
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    id: i64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    original_title: Option<String>,
    #[serde(default)]
    original_name: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    first_air_date: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    poster_path: Option<String>,
    #[serde(default)]
    vote_average: Option<f64>,
    #[serde(default)]
    genre_ids: Vec<i64>,
}

pub fn parse_search(body: &str) -> Result<Vec<ScrapeCandidate>, ScrapeError> {
    let resp: SearchResponse =
        serde_json::from_str(body).map_err(|e| ScrapeError::Parse(e.to_string()))?;
    Ok(resp
        .results
        .into_iter()
        .map(|it| ScrapeCandidate {
            source_id: it.id,
            title: it.title.or(it.name).unwrap_or_default(),
            original_title: it.original_title.or(it.original_name).unwrap_or_default(),
            year: it
                .release_date
                .or(it.first_air_date)
                .and_then(|d| d.get(..4).and_then(|y| y.parse().ok())),
            overview: it.overview.unwrap_or_default(),
            poster_path: it.poster_path,
            rating: it.vote_average.unwrap_or(0.0),
            genres: genres_for(&it.genre_ids),
        })
        .collect())
}

fn genres_for(ids: &[i64]) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        if let Some(name) = genre_name(*id) {
            if !out.iter().any(|x: &String| x == name) {
                out.push(name.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_dedups_genre_tags() {
        let body = r#"{"results":[{"id":1,"title":"X","genre_ids":[99,16,99,99999],"overview":"","poster_path":null,"vote_average":8.0}]}"#;
        let list = parse_search(body).unwrap();
        assert_eq!(list[0].genres, vec!["纪录片".to_string(), "动画".to_string()]);
    }
}

fn genre_name(id: i64) -> Option<&'static str> {
    Some(match id {
        28 => "动作",
        12 => "冒险",
        16 => "动画",
        35 => "喜剧",
        80 => "犯罪",
        99 => "纪录片",
        18 => "剧情",
        10751 => "家庭",
        14 => "奇幻",
        36 => "历史",
        27 => "恐怖",
        10402 => "音乐",
        9648 => "悬疑",
        10749 => "爱情",
        878 => "科幻",
        10770 => "电视电影",
        53 => "惊悚",
        10752 => "战争",
        37 => "西部",
        10759 => "动作冒险",
        10762 => "儿童",
        10763 => "新闻",
        10764 => "真人秀",
        10765 => "科幻奇幻",
        10766 => "肥皂剧",
        10767 => "脱口秀",
        10768 => "战争政治",
        _ => return None,
    })
}