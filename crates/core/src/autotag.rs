use std::collections::HashSet;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use video_manager_shared::AutoTagStats;

use crate::db::DbError;
use crate::tag;

pub fn apply_all(conn: &Connection) -> Result<AutoTagStats, DbError> {
    let ids: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT id FROM videos ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut stats = AutoTagStats {
        videos: 0,
        tags_added: 0,
    };
    for id in ids {
        let (videos, added) = apply_video(conn, id)?;
        stats.videos += videos;
        stats.tags_added += added;
    }
    Ok(stats)
}

pub fn apply_video(conn: &Connection, id: i64) -> Result<(i64, i64), DbError> {
    let row = conn
        .query_row(
            "SELECT path, width, height, duration_ms FROM videos WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<i32>>(1)?,
                    r.get::<_, Option<i32>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((path, width, height, duration)) = row else {
        return Ok((0, 0));
    };

    let existing: HashSet<String> = tag::for_video(conn, id)?
        .into_iter()
        .map(|t| t.name)
        .collect();
    let mut added = 0;
    for name in derive(&path, width, height, duration) {
        if !existing.contains(&name) {
            tag::add_video_tag(conn, id, &name)?;
            added += 1;
        }
    }
    Ok((1, added))
}

pub fn derive(
    path: &str,
    width: Option<i32>,
    height: Option<i32>,
    duration_ms: Option<i64>,
) -> Vec<String> {
    let p = Path::new(path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let parent = p
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let text = format!("{stem} {parent}").to_lowercase();

    let mut out: Vec<String> = Vec::new();
    let mut push = |name: &str| {
        if !out.iter().any(|x| x == name) {
            out.push(name.to_string());
        }
    };

    if hit(
        &text,
        &["纪录片", "记录片", "documentary", "documentaries"],
    ) {
        push("纪录片");
    }
    if hit(&text, &["动画", "动漫", "anime", "cartoon", "animation"]) {
        push("动画");
    }
    if hit(
        &text,
        &["教程", "教学", "课程", "tutorial", "course", "lesson"],
    ) {
        push("教程");
    }
    if hit(&text, &["电影", "movie", "film"]) {
        push("电影");
    }
    if hit(
        &text,
        &["电视剧", "剧集", "episode", "series", "season"],
    ) {
        push("剧集");
    }
    if text.contains('第') && text.contains('集') {
        push("剧集");
    }
    if hit(&text, &["预告", "trailer", "teaser"]) {
        push("预告");
    }
    if hit(&text, &["花絮", "behind the scenes", "bts"]) {
        push("花絮");
    }
    if hit(&text, &["演唱会", "concert", "live"]) {
        push("演唱会");
    }
    if hit(
        &text,
        &[
            "风景", "风光", "旅行", "travel", "landscape", "海滩", "海边", "沙滩",
        ],
    ) {
        push("风景");
    }
    if hit(&text, &["游戏", "game", "gameplay"]) {
        push("游戏");
    }
    if hit(&text, &["音乐", "music", "mv"]) {
        push("音乐");
    }
    if hit(&text, &["测试", "示例", "test", "sample", "demo"]) {
        push("测试");
    }
    if hit(&text, &["vlog", "家庭录像", "home video"]) {
        push("Vlog");
    }
    if hit(&text, &["长视频"]) {
        push("长视频");
    }
    if hit(&text, &["短视频"]) {
        push("短视频");
    }
    if hit(&text, &["hdr"]) {
        push("HDR");
    }

    if hit(&text, &["8k", "4320p"]) {
        push("8K");
    }
    if hit(&text, &["4k", "2160p"]) {
        push("4K");
    }
    if hit(&text, &["2k", "1440p"]) {
        push("2K");
    }
    if hit(&text, &["1080p"]) {
        push("1080P");
    }
    if hit(&text, &["720p"]) {
        push("720P");
    }

    if let (Some(w), Some(h)) = (width, height) {
        let long = w.max(h);
        if long >= 7680 {
            push("8K");
        } else if long >= 3840 {
            push("4K");
        } else if long >= 2560 {
            push("2K");
        } else if long >= 1920 {
            push("1080P");
        } else if long >= 1280 {
            push("720P");
        }
        if h > w {
            push("竖屏");
        }
    }

    if let Some(d) = duration_ms {
        if d >= 30 * 60 * 1000 {
            push("长视频");
        } else if d > 0 && d <= 60 * 1000 {
            push("短视频");
        }
    }

    out
}

fn hit(text: &str, words: &[&str]) -> bool {
    words.iter().any(|w| {
        if w.contains(' ') {
            text.contains(w)
        } else if w.chars().all(|c| c.is_ascii_alphanumeric()) {
            has_token(text, w)
        } else {
            text.contains(w)
        }
    })
}

fn has_token(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|t| t == word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_keywords_and_resolution() {
        let tags = derive(
            "D:/videos/自然纪录片_第一集.mp4",
            Some(1280),
            Some(720),
            Some(300_000),
        );
        assert!(tags.contains(&"纪录片".to_string()));
        assert!(tags.contains(&"剧集".to_string()));
        assert!(tags.contains(&"720P".to_string()));
        assert!(!tags.contains(&"短视频".to_string()));
    }

    #[test]
    fn derives_portrait_and_short() {
        let tags = derive(
            "C:/videos/竖屏_测试.mp4",
            Some(1080),
            Some(1920),
            Some(30_000),
        );
        assert!(tags.contains(&"竖屏".to_string()));
        assert!(tags.contains(&"1080P".to_string()));
        assert!(tags.contains(&"测试".to_string()));
        assert!(tags.contains(&"短视频".to_string()));
    }

    #[test]
    fn derives_movie_and_long() {
        let tags = derive(
            "D:/电影/Fight.Club.1999.4K.mkv",
            Some(3840),
            Some(2160),
            Some(7_200_000),
        );
        assert!(tags.contains(&"电影".to_string()));
        assert!(tags.contains(&"4K".to_string()));
        assert!(tags.contains(&"长视频".to_string()));
    }

    #[test]
    fn ignores_generic_folder_tokens() {
        let tags = derive("D:/testdata/风景延时.mp4", Some(854), Some(480), Some(4_000));
        assert!(tags.contains(&"风景".to_string()));
        assert!(tags.contains(&"短视频".to_string()));
        assert!(!tags.contains(&"测试".to_string()));
    }
}
