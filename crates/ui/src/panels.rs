use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use video_manager_shared::{AppInfo, ScrapeCandidate, Settings, Tag, Video};

use crate::ipc;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveSettingsArgs {
    settings: Settings,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScrapeSearchArgs {
    query: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScrapeApplyArgs {
    video_id: i64,
    candidate: ScrapeCandidate,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VideoIdArgs {
    video_id: i64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AddTagArgs {
    video_id: i64,
    name: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoveTagArgs {
    video_id: i64,
    tag_id: i64,
}

fn clean_query(filename: &str) -> String {
    let base = filename.rsplit_once('.').map(|(b, _)| b).unwrap_or(filename);
    base.replace(['_', '.'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[component]
pub fn SettingsModal(
    initial: Settings,
    info: Option<AppInfo>,
    on_save: EventHandler<Settings>,
    on_close: EventHandler<()>,
) -> Element {
    let mut enabled = use_signal(|| initial.scrape_enabled);
    let mut key = use_signal(|| initial.tmdb_api_key.clone());
    let mut proxy = use_signal(|| initial.proxy.clone());
    let mut media_type = use_signal(|| initial.media_type.clone());
    let mut language = use_signal(|| initial.language.clone());
    let mut saving = use_signal(|| false);
    let mut err = use_signal(|| None::<String>);

    let mut save = move || {
        if saving() {
            return;
        }
        saving.set(true);
        err.set(None);
        let settings = Settings {
            scrape_enabled: enabled(),
            tmdb_api_key: key(),
            proxy: proxy(),
            media_type: media_type(),
            language: language(),
        };
        spawn_forever(async move {
            match ipc::invoke::<_, ()>(
                "save_settings",
                SaveSettingsArgs {
                    settings: settings.clone(),
                },
            )
            .await
            {
                Ok(()) => on_save.call(settings),
                Err(e) => {
                    err.set(Some(e));
                    saving.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "modal-mask", onclick: move |_| on_close.call(()),
            div { class: "modal", onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h2 { "设置" }
                    button { class: "toast-close", onclick: move |_| on_close.call(()), "×" }
                }
                div { class: "form",
                    label { class: "row-check",
                        input {
                            r#type: "checkbox",
                            checked: enabled(),
                            onchange: move |e| enabled.set(e.checked()),
                        }
                        span { "启用刮削（TMDB）" }
                    }
                    p { class: "muted small", "关闭时为纯本地模式，不发起任何网络请求；刮削是唯一联网功能。" }
                    div { class: "form-row",
                        label { "API key" }
                        input {
                            r#type: "text",
                            value: key(),
                            placeholder: "TMDB v3 key 或 v4 读取令牌",
                            oninput: move |e| key.set(e.value()),
                        }
                    }
                    div { class: "form-row",
                        label { "代理" }
                        input {
                            r#type: "text",
                            value: proxy(),
                            placeholder: "http://127.0.0.1:7890 或 socks5://…（留空直连）",
                            oninput: move |e| proxy.set(e.value()),
                        }
                    }
                    div { class: "form-row",
                        label { "内容类型" }
                        select {
                            value: media_type(),
                            onchange: move |e| media_type.set(e.value()),
                            option { value: "movie", "电影" }
                            option { value: "tv", "剧集" }
                        }
                    }
                    div { class: "form-row",
                        label { "刮削语言" }
                        select {
                            value: language(),
                            onchange: move |e| language.set(e.value()),
                            option { value: "zh-CN", "简体中文" }
                            option { value: "en-US", "English" }
                        }
                    }
                    if let Some(e) = err() {
                        p { class: "warn small", "{e}" }
                    }
                    div { class: "form-actions",
                        button { class: "btn", disabled: saving(), onclick: move |_| save(), "保存" }
                        button { class: "btn", onclick: move |_| on_close.call(()), "取消" }
                    }
                }
                if let Some(i) = info {
                    div { class: "about",
                        h3 { "关于" }
                        div { class: "about-row", span { class: "about-k", "版本" } span { "{i.app_version}" } }
                        div { class: "about-row", span { class: "about-k", "视频数" } span { "{i.video_count}" } }
                        div { class: "about-row", span { class: "about-k", "媒体库" } span { "{i.library_count}" } }
                        div { class: "about-row",
                            span { class: "about-k", "ffmpeg" }
                            span {
                                class: if i.ffmpeg_version.is_some() { "about-ok" } else { "warn" },
                                if let Some(v) = i.ffmpeg_version.clone() { "{v}" } else { "未找到" }
                            }
                        }
                        div { class: "about-row",
                            span { class: "about-k", "libmpv" }
                            span {
                                class: if i.mpv_version.is_some() { "about-ok" } else { "warn" },
                                if let Some(v) = i.mpv_version.clone() { "{v}" } else { "未找到" }
                            }
                        }
                        div { class: "about-row", span { class: "about-k", "数据库" } span { class: "about-path", title: "{i.db_path}", "{i.db_path}" } }
                        div { class: "about-row", span { class: "about-k", "缩略图" } span { class: "about-path", title: "{i.thumb_dir}", "{i.thumb_dir}" } }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ScrapeModal(
    video: Video,
    on_close: EventHandler<()>,
    on_applied: EventHandler<()>,
) -> Element {
    let mut query = use_signal(|| clean_query(&video.filename));
    let mut busy = use_signal(|| false);
    let mut searched = use_signal(|| false);
    let mut err = use_signal(|| None::<String>);
    let mut results = use_signal(Vec::<ScrapeCandidate>::new);
    let mut applied = use_signal(|| None::<i64>);
    let video_id = video.id;

    let mut search = move || {
        let q = query();
        if busy() || q.trim().is_empty() {
            return;
        }
        busy.set(true);
        err.set(None);
        spawn(async move {
            match ipc::invoke::<_, Vec<ScrapeCandidate>>(
                "scrape_search",
                ScrapeSearchArgs { query: q },
            )
            .await
            {
                Ok(r) => {
                    results.set(r);
                    searched.set(true);
                }
                Err(e) => err.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let apply = move |c: ScrapeCandidate| {
        spawn_forever(async move {
            match ipc::invoke::<_, ()>(
                "scrape_apply",
                ScrapeApplyArgs {
                    video_id,
                    candidate: c.clone(),
                },
            )
            .await
            {
                Ok(()) => {
                    applied.set(Some(c.source_id));
                    on_applied.call(());
                }
                Err(e) => err.set(Some(e)),
            }
        });
    };

    rsx! {
        div { class: "modal-mask", onclick: move |_| on_close.call(()),
            div { class: "modal scrape-modal", onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h2 { "刮削 · {video.filename}" }
                    button { class: "toast-close", onclick: move |_| on_close.call(()), "×" }
                }
                div { class: "scrape-bar",
                    input {
                        class: "search",
                        r#type: "text",
                        value: query(),
                        oninput: move |e| query.set(e.value()),
                        onkeydown: move |e| { if e.key() == Key::Enter { search(); } },
                    }
                    button {
                        class: "btn",
                        disabled: busy(),
                        onclick: move |_| search(),
                        if busy() { "搜索中…" } else { "搜索" }
                    }
                }
                if let Some(e) = err() {
                    p { class: "warn small", "{e}" }
                }
                if searched() && results().is_empty() {
                    p { class: "muted small", "未找到匹配，可修改关键词（去掉分辨率、组名等标签）后重试。" }
                }
                div { class: "scrape-list",
                    for c in results().iter().cloned() {
                        div { class: "scrape-item", key: "{c.source_id}",
                            if let Some(p) = c.poster_path.clone() {
                                img { class: "scrape-poster", src: "https://image.tmdb.org/t/p/w92{p}", alt: "" }
                            } else {
                                div { class: "scrape-poster" }
                            }
                            div { class: "scrape-info",
                                div {
                                    "{c.title}"
                                    if let Some(y) = c.year {
                                        span { class: "muted small", "（{y}）" }
                                    }
                                }
                                div { class: "muted small", "{c.original_title} · ★ {c.rating:.1}" }
                                if !c.genres.is_empty() {
                                    div { class: "muted small", "{c.genres.join(\" · \")}" }
                                }
                                p { class: "muted small scrape-overview", "{c.overview}" }
                            }
                            button {
                                class: "btn small",
                                disabled: applied() == Some(c.source_id),
                                onclick: move |_| apply(c.clone()),
                                if applied() == Some(c.source_id) { "已应用" } else { "应用" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn TagModal(
    video: Video,
    on_close: EventHandler<()>,
    on_changed: EventHandler<()>,
) -> Element {
    let mut tags = use_signal(Vec::<Tag>::new);
    let mut draft = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut err = use_signal(|| None::<String>);
    let video_id = video.id;

    use_resource(move || async move {
        if let Ok(t) = ipc::invoke::<_, Vec<Tag>>("get_video_tags", VideoIdArgs { video_id }).await {
            tags.set(t);
        }
    });

    let mut add = move || {
        let name = draft().trim().to_string();
        if busy() || name.is_empty() {
            return;
        }
        busy.set(true);
        err.set(None);
        spawn_forever(async move {
            match ipc::invoke::<_, Vec<Tag>>("add_video_tag", AddTagArgs { video_id, name }).await {
                Ok(t) => {
                    tags.set(t);
                    draft.set(String::new());
                    on_changed.call(());
                }
                Err(e) => err.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let remove = move |tag_id: i64| {
        spawn_forever(async move {
            match ipc::invoke::<_, Vec<Tag>>("remove_video_tag", RemoveTagArgs { video_id, tag_id })
                .await
            {
                Ok(t) => {
                    tags.set(t);
                    on_changed.call(());
                }
                Err(e) => err.set(Some(e)),
            }
        });
    };

    rsx! {
        div { class: "modal-mask", onclick: move |_| on_close.call(()),
            div { class: "modal tag-modal", onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h2 { "标签 · {video.filename}" }
                    button { class: "toast-close", onclick: move |_| on_close.call(()), "×" }
                }
                div { class: "tag-chips",
                    for t in tags().iter().cloned() {
                        span { class: "tag-chip", key: "{t.id}",
                            "{t.name}"
                            button { class: "tag-chip-x", title: "移除", onclick: move |_| remove(t.id), "×" }
                        }
                    }
                    if tags().is_empty() {
                        span { class: "muted small", "暂无标签，输入名称后回车添加" }
                    }
                }
                div { class: "scrape-bar",
                    input {
                        class: "search",
                        r#type: "text",
                        value: draft(),
                        placeholder: "新标签名…",
                        oninput: move |e| draft.set(e.value()),
                        onkeydown: move |e| { if e.key() == Key::Enter { add(); } },
                    }
                    button { class: "btn", disabled: busy(), onclick: move |_| add(), "添加" }
                }
                if let Some(e) = err() {
                    p { class: "warn small", "{e}" }
                }
            }
        }
    }
}