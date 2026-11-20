#![allow(non_snake_case)]

mod art;
mod components;
mod ipc;
mod panels;

use components::{MpvState, Pager, PlayerModal, Sidebar, VideoGrid};
use dioxus::prelude::*;
use panels::{ScrapeModal, SettingsModal, TagModal};
use video_manager_shared::{
    AppInfo, AutoTagStats, Library, ScanProgress, ScanStats, ScrapeProgress, ScrapeStats, Settings,
    Tag, ThumbProgress, Video, VideoPage, VideoQuery,
};

#[derive(serde::Serialize)]
struct QueryArgs {
    query: VideoQuery,
}

#[derive(serde::Serialize)]
struct AddArgs {
    path: String,
}

#[derive(serde::Serialize)]
struct RemoveArgs {
    id: i64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanArgs {
    library_id: Option<i64>,
}

#[derive(serde::Deserialize)]
struct MpvVolume {
    volume: f64,
    mute: bool,
}

fn main() {
    launch(App);
}

fn App() -> Element {
    let mut libraries = use_signal(Vec::<Library>::new);
    let mut selected_lib = use_signal(|| None::<i64>);
    let mut text = use_signal(String::new);
    let mut sort = use_signal(|| "added".to_string());
    let mut page = use_signal(|| 1i64);
    let mut videos = use_signal(|| VideoPage { total: 0, items: vec![] });
    let mut progress = use_signal(|| None::<ScanProgress>);
    let mut thumb_progress = use_signal(|| None::<ThumbProgress>);
    let mut scrape_progress = use_signal(|| None::<ScrapeProgress>);
    let mut toast = use_signal(|| None::<String>);
    let mut playing = use_signal(|| None::<Video>);
    let mut mpv_ready = use_signal(|| false);
    let mut mpv_err = use_signal(|| None::<String>);
    let mut mpv_volume = use_signal(|| 100.0f64);
    let mut mpv_mute = use_signal(|| false);
    let mut mpv_state = use_signal(MpvState::default);
    let mut mpv_activity = use_signal(|| js_sys::Date::now() as u64);
    let mut mpv_controls_visible = use_signal(|| true);
    let mut loading = use_signal(|| false);
    let mut refresh = use_signal(|| 0u64);
    let mut info = use_signal(|| None::<AppInfo>);
    let mut settings = use_signal(Settings::default);
    let mut show_settings = use_signal(|| false);
    let mut scraping = use_signal(|| None::<Video>);
    let mut auto_tagging = use_signal(|| false);
    let mut tags = use_signal(Vec::<Tag>::new);
    let mut tag_filter = use_signal(|| None::<i64>);
    let mut tagging = use_signal(|| None::<Video>);

    use_resource(move || async move {
        match ipc::invoke_no_args::<Vec<Library>>("list_libraries").await {
            Ok(libs) => {
                let has_libs = !libs.is_empty();
                libraries.set(libs);
                if has_libs {
                    let _ = ipc::invoke::<_, ()>("scan_library", ScanArgs { library_id: None }).await;
                }
            }
            Err(e) => toast.set(Some(format!("加载媒体库失败：{e}"))),
        }
        if let Ok(i) = ipc::invoke_no_args::<AppInfo>("app_info").await {
            info.set(Some(i));
        }
        if let Ok(s) = ipc::invoke_no_args::<Settings>("get_settings").await {
            settings.set(s);
        }
    });

    use_resource(move || async move {
        loading.set(true);
        let q = VideoQuery {
            library_id: selected_lib(),
            text: Some(text()),
            tag_id: tag_filter(),
            sort: sort(),
            page: page(),
            page_size: 100,
        };
        let _ = refresh();
        match ipc::invoke::<_, VideoPage>("list_videos", QueryArgs { query: q }).await {
            Ok(p) => videos.set(p),
            Err(e) => toast.set(Some(format!("查询失败：{e}"))),
        }
        loading.set(false);
    });

    use_resource(move || async move {
        let _ = refresh();
        if let Ok(t) = ipc::invoke_no_args::<Vec<Tag>>("list_tags").await {
            if let Some(id) = tag_filter() {
                if !t.iter().any(|x| x.id == id) {
                    tag_filter.set(None);
                }
            }
            tags.set(t);
        }
    });

    use_hook(move || {
        spawn(async move {
            let _ = ipc::listen::<ScanProgress, _>("scan-progress", move |p| {
                progress.set(Some(p));
            })
            .await;
        });
        spawn(async move {
            let _ = ipc::listen::<ScanStats, _>("scan-done", move |s| {
                progress.set(None);
                toast.set(Some(format!(
                    "扫描完成：新增 {} · 更新 {} · 移除 {} · 解析 {}（失败 {}）",
                    s.added, s.updated, s.removed, s.probed, s.probe_failed
                )));
                refresh.set(refresh() + 1);
            })
            .await;
        });
        spawn(async move {
            let _ = ipc::listen::<String, _>("scan-error", move |e| {
                progress.set(None);
                toast.set(Some(format!("扫描失败：{e}")));
            })
            .await;
        });
        spawn(async move {
            let _ = ipc::listen::<ThumbProgress, _>("thumb-progress", move |p| {
                if p.total > 0 && p.done >= p.total {
                    thumb_progress.set(None);
                    refresh.set(refresh() + 1);
                } else {
                    thumb_progress.set(Some(p));
                }
            })
            .await;
        });
        spawn(async move {
            let _ = ipc::listen::<ScrapeProgress, _>("scrape-progress", move |p| {
                scrape_progress.set(Some(p));
            })
            .await;
        });
        spawn(async move {
            let _ = ipc::listen::<ScrapeStats, _>("scrape-done", move |s| {
                scrape_progress.set(None);
                toast.set(Some(format!(
                    "刮削完成：匹配 {} / 共 {}（失败 {}）",
                    s.matched, s.total, s.failed
                )));
                refresh.set(refresh() + 1);
            })
            .await;
        });
        spawn(async move {
            let _ = ipc::listen::<i64, _>("mpv-ready", move |_| {
                mpv_ready.set(true);
            })
            .await;
        });
        spawn(async move {
            let _ = ipc::listen::<String, _>("mpv-error", move |e| {
                mpv_err.set(Some(e));
                spawn(async move {
                    let _ = ipc::invoke_no_args::<()>("mpv_close").await;
                });
            })
            .await;
        });
        spawn(async move {
            let _ = ipc::listen::<MpvVolume, _>("mpv-volume", move |v| {
                mpv_volume.set(v.volume);
                mpv_mute.set(v.mute);
            })
            .await;
        });
        spawn(async move {
            let _ = ipc::listen::<MpvState, _>("mpv-state", move |s| {
                mpv_state.set(s);
            })
            .await;
        });
        spawn(async move {
            let _ = ipc::listen::<bool, _>("mpv-activity", move |_| {
                mpv_activity.set(js_sys::Date::now() as u64);
                mpv_controls_visible.set(true);
            })
            .await;
        });
    });

    let add_library = move || {
        spawn(async move {
            let picked = match ipc::invoke_no_args::<Option<String>>("pick_folder").await {
                Ok(p) => p,
                Err(e) => {
                    toast.set(Some(format!("选择文件夹失败：{e}")));
                    return;
                }
            };
            let Some(path) = picked else { return };
            match ipc::invoke::<_, Library>("add_library", AddArgs { path }).await {
                Ok(lib) => {
                    libraries.write().push(lib.clone());
                    toast.set(Some(format!("已添加「{}」，开始扫描…", lib.name)));
                    if let Err(e) =
                        ipc::invoke::<_, ()>("scan_library", ScanArgs { library_id: Some(lib.id) }).await
                    {
                        toast.set(Some(e));
                    }
                }
                Err(e) => toast.set(Some(e)),
            }
        });
    };

    let remove_library = move |id: i64| {
        spawn(async move {
            match ipc::invoke::<_, ()>("remove_library", RemoveArgs { id }).await {
                Ok(()) => {
                    libraries.write().retain(|l| l.id != id);
                    if selected_lib() == Some(id) {
                        selected_lib.set(None);
                    }
                    refresh.set(refresh() + 1);
                }
                Err(e) => toast.set(Some(e)),
            }
        });
    };

    let scan = move |id: Option<i64>| {
        spawn(async move {
            if let Err(e) = ipc::invoke::<_, ()>("scan_library", ScanArgs { library_id: id }).await {
                toast.set(Some(e));
            }
        });
    };

    let scrape_all = move || {
        spawn(async move {
            if let Err(e) = ipc::invoke_no_args::<()>("scrape_all").await {
                toast.set(Some(e));
            }
        });
    };

    let mut auto_tag = move || {
        if auto_tagging() {
            return;
        }
        auto_tagging.set(true);
        toast.set(Some("正在生成标签…".into()));
        spawn(async move {
            match ipc::invoke_no_args::<AutoTagStats>("auto_tag_all").await {
                Ok(s) => {
                    toast.set(Some(format!(
                        "自动标签完成：处理 {} 个视频，新增 {} 个标签",
                        s.videos, s.tags_added
                    )));
                    refresh.set(refresh() + 1);
                }
                Err(e) => toast.set(Some(format!("自动标签失败：{e}"))),
            }
            auto_tagging.set(false);
        });
    };

    rsx! {
        link { rel: "stylesheet", href: asset!("/assets/main.css") }
        div { class: "app",
            Sidebar {
                libraries: libraries(),
                selected: selected_lib(),
                tags: tags(),
                tag_filter: tag_filter(),
                progress: progress(),
                thumb_progress: thumb_progress(),
                scrape_progress: scrape_progress(),
                info: info(),
                on_select: move |id| { selected_lib.set(id); page.set(1); },
                on_tag_filter: move |id| { tag_filter.set(id); page.set(1); },
                on_add: move |_| add_library(),
                on_remove: move |id| remove_library(id),
                on_scan: move |id| scan(Some(id)),
                on_settings: move |_| show_settings.set(true),
            }
            div { class: "main",
                div { class: "toolbar",
                    input {
                        class: "search",
                        r#type: "text",
                        placeholder: "搜索文件名 / 标题…",
                        value: text(),
                        oninput: move |e| { text.set(e.value()); page.set(1); },
                    }
                    select {
                        class: "sort",
                        value: sort(),
                        onchange: move |e| { sort.set(e.value()); page.set(1); },
                        option { value: "added", "最近入库" }
                        option { value: "name", "文件名" }
                        option { value: "size", "大小" }
                        option { value: "duration", "时长" }
                    }
                    button { class: "btn", onclick: move |_| scan(None), "扫描全部" }
                    button { class: "btn", onclick: move |_| scrape_all(), "一键刮削" }
                    button {
                        class: "btn",
                        disabled: auto_tagging(),
                        onclick: move |_| auto_tag(),
                        if auto_tagging() { "生成中…" } else { "自动标签" }
                    }
                }
                if let Some(t) = toast() {
                    div { class: "toast",
                        span { "{t}" }
                        button { class: "toast-close", onclick: move |_| toast.set(None), "×" }
                    }
                }
                VideoGrid {
                    page: videos(),
                    loading: loading(),
                    thumb_dir: info().map(|i| i.thumb_dir.clone()).unwrap_or_default(),
                    on_open: move |v| {
                        mpv_ready.set(false);
                        mpv_err.set(None);
                        mpv_state.set(MpvState::default());
                        mpv_activity.set(js_sys::Date::now() as u64);
                        mpv_controls_visible.set(true);
                        playing.set(Some(v));
                    },
                    on_scrape: move |v| scraping.set(Some(v)),
                    on_tag: move |v| tagging.set(Some(v)),
                }
                Pager {
                    total: videos().total,
                    page: page(),
                    on_page: move |p| page.set(p),
                }
            }
            if let Some(v) = playing() {
                PlayerModal {
                    video: v,
                    mpv: info().and_then(|i| i.mpv_version.clone()).is_some(),
                    ready: mpv_ready(),
                    err: mpv_err(),
                    state: mpv_state(),
                    activity: mpv_activity,
                    controls_visible: mpv_controls_visible,
                    volume: mpv_volume(),
                    mute: mpv_mute(),
                    on_close: move |_| { playing.set(None); refresh.set(refresh() + 1); },
                }
            }
            if show_settings() {
                SettingsModal {
                    initial: settings(),
                    info: info(),
                    on_save: move |s| {
                        settings.set(s);
                        show_settings.set(false);
                        toast.set(Some("设置已保存".to_string()));
                    },
                    on_close: move |_| show_settings.set(false),
                }
            }
            if let Some(v) = scraping() {
                ScrapeModal {
                    video: v,
                    on_close: move |_| scraping.set(None),
                    on_applied: move |_| refresh.set(refresh() + 1),
                }
            }
            if let Some(v) = tagging() {
                TagModal {
                    video: v,
                    on_close: move |_| tagging.set(None),
                    on_changed: move |_| refresh.set(refresh() + 1),
                }
            }
        }
    }
}