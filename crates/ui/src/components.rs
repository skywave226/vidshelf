use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use video_manager_shared::{
    AppInfo, Library, ScanProgress, ScrapeProgress, Tag, ThumbProgress, Video, VideoPage,
};
use wasm_bindgen::{closure::Closure, JsCast};

use crate::ipc;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SpriteArgs {
    video_id: i64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VideoIdArgs {
    video_id: i64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MpvOpenArgs {
    video_id: i64,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(serde::Serialize)]
struct MpvRectArgs {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(serde::Serialize)]
struct MpvCommandArgs {
    args: Vec<String>,
}

#[derive(serde::Serialize)]
struct MpvFullscreenArgs {
    on: bool,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
pub struct MpvState {
    pub position_ms: i64,
    pub duration_ms: i64,
    pub paused: bool,
}

fn mpv_command(args: Vec<&str>) {
    let args = args.into_iter().map(str::to_string).collect();
    spawn_forever(async move {
        let _ = ipc::invoke::<_, ()>("mpv_command", MpvCommandArgs { args }).await;
    });
}

fn mpv_toggle_pause() {
    mpv_command(vec!["cycle", "pause"]);
}

fn mpv_seek_to(ms: i64, exact: bool) {
    let target = format!("{:.3}", ms.max(0) as f64 / 1000.0);
    mpv_command(vec![
        "seek",
        &target,
        if exact { "absolute+exact" } else { "absolute+keyframes" },
    ]);
}

fn mpv_seek_relative(sec: i64) {
    let target = sec.to_string();
    mpv_command(vec!["seek", &target, "relative"]);
}

const SPEEDS: [f64; 6] = [1.0, 1.25, 1.5, 2.0, 0.5, 0.75];

async fn delay_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(win) = web_sys::window() {
            let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

pub fn fmt_size(bytes: i64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else {
        format!("{:.0} KB", b / 1024.0)
    }
}

pub fn fmt_duration(ms: i64) -> String {
    let s = ms / 1000;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

#[component]
pub fn Sidebar(
    libraries: Vec<Library>,
    selected: Option<i64>,
    tags: Vec<Tag>,
    tag_filter: Option<i64>,
    progress: Option<ScanProgress>,
    thumb_progress: Option<ThumbProgress>,
    scrape_progress: Option<ScrapeProgress>,
    info: Option<AppInfo>,
    on_select: EventHandler<Option<i64>>,
    on_tag_filter: EventHandler<Option<i64>>,
    on_add: EventHandler<()>,
    on_remove: EventHandler<i64>,
    on_scan: EventHandler<i64>,
    on_settings: EventHandler<()>,
) -> Element {
    let mut confirming = use_signal(|| None::<i64>);
    let pct = progress
        .as_ref()
        .map(|p| if p.total > 0 { p.done * 100 / p.total } else { 0 })
        .unwrap_or(0);
    let thumb_pct = thumb_progress
        .as_ref()
        .map(|p| if p.total > 0 { p.done * 100 / p.total } else { 0 })
        .unwrap_or(0);
    let scrape_pct = scrape_progress
        .as_ref()
        .map(|p| if p.total > 0 { p.done * 100 / p.total } else { 0 })
        .unwrap_or(0);

    rsx! {
        aside { class: "sidebar",
            div { class: "side-head",
                span { "媒体库" }
                button { class: "btn small", onclick: move |_| on_add.call(()), "+ 添加" }
            }
            ul { class: "lib-list",
                li {
                    class: if selected.is_none() { "lib-item active" } else { "lib-item" },
                    onclick: move |_| on_select.call(None),
                    "全部视频"
                }
                for lib in libraries.iter().cloned() {
                    li {
                        key: "{lib.id}",
                        class: if selected == Some(lib.id) { "lib-item active" } else { "lib-item" },
                        div {
                            class: "lib-name",
                            title: "{lib.path}",
                            onclick: move |_| on_select.call(Some(lib.id)),
                            "{lib.name}"
                        }
                        div { class: "lib-actions",
                            button {
                                class: "link-btn",
                                onclick: move |_| on_scan.call(lib.id),
                                "扫描",
                            }
                            if confirming() == Some(lib.id) {
                                button {
                                    class: "link-btn danger",
                                    onclick: move |_| { confirming.set(None); on_remove.call(lib.id); },
                                    "确认删除",
                                }
                                button {
                                    class: "link-btn",
                                    onclick: move |_| confirming.set(None),
                                    "取消",
                                }
                            } else {
                                button {
                                    class: "link-btn",
                                    onclick: move |_| confirming.set(Some(lib.id)),
                                    "删除",
                                }
                            }
                        }
                    }
                }
            }
            if !tags.is_empty() {
                div { class: "side-head",
                    span { "标签" }
                    if tag_filter.is_some() {
                        button { class: "link-btn", onclick: move |_| on_tag_filter.call(None), "清除" }
                    }
                }
                ul { class: "tag-list",
                    for t in tags.iter().cloned() {
                        li {
                            key: "{t.id}",
                            class: if tag_filter == Some(t.id) { "tag-item active" } else { "tag-item" },
                            onclick: move |_| {
                                if tag_filter == Some(t.id) { on_tag_filter.call(None); }
                                else { on_tag_filter.call(Some(t.id)); }
                            },
                            span { class: "tag-name", "{t.name}" }
                            span { class: "tag-count muted small", "{t.count}" }
                        }
                    }
                }
            }
            if let Some(p) = progress {
                div { class: "scan-box",
                    if p.phase == "walking" {
                        span { class: "muted small", "正在遍历 {p.library_name}…" }
                    } else {
                        span { class: "muted small", "解析 {p.library_name}：{p.done}/{p.total}" }
                        div { class: "bar",
                            div {
                                class: "bar-fill",
                                style: "width: {pct}%",
                            }
                        }
                    }
                }
            }
            if let Some(p) = thumb_progress {
                div { class: "scan-box",
                    span { class: "muted small", "缩略图：{p.done}/{p.total}" }
                    div { class: "bar",
                        div {
                            class: "bar-fill",
                            style: "width: {thumb_pct}%",
                        }
                    }
                }
            }
            if let Some(p) = scrape_progress {
                div { class: "scan-box",
                    span { class: "muted small", "刮削：{p.done}/{p.total} · 匹配 {p.matched}" }
                    div { class: "bar",
                        div {
                            class: "bar-fill",
                            style: "width: {scrape_pct}%",
                        }
                    }
                }
            }
            div { class: "side-foot muted small",
                button {
                    class: "link-btn",
                    onclick: move |_| on_settings.call(()),
                    "设置",
                }
                if let Some(i) = info {
                    p { "v{i.app_version}" }
                    if i.ffmpeg_version.is_none() {
                        p { class: "warn", "未找到 ffmpeg" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn VideoGrid(
    page: VideoPage,
    loading: bool,
    thumb_dir: String,
    on_open: EventHandler<Video>,
    on_scrape: EventHandler<Video>,
    on_tag: EventHandler<Video>,
) -> Element {
    if page.items.is_empty() {
        return rsx! {
            div { class: "empty muted",
                if loading { "加载中…" } else { "暂无视频：添加媒体库并扫描后显示" }
            }
        };
    }
    rsx! {
        div { class: "grid",
            for v in page.items.iter().cloned() {
                VideoCard {
                    key: "{v.id}",
                    video: v.clone(),
                    thumb_dir: thumb_dir.clone(),
                    on_open: on_open,
                    on_scrape: on_scrape,
                    on_tag: on_tag,
                }
            }
        }
    }
}

#[component]
fn VideoCard(video: Video, thumb_dir: String, on_open: EventHandler<Video>, on_scrape: EventHandler<Video>, on_tag: EventHandler<Video>) -> Element {
    let mut hovering = use_signal(|| false);
    let mut sprite = use_signal(|| None::<String>);
    let mut sprite_busy = use_signal(|| false);
    let mut poster_failed = use_signal(|| false);

    let v = video.clone();
    let vs = video.clone();
    let vt = video.clone();
    let display_title = video.title.clone().unwrap_or_else(|| video.filename.clone());
    let letter = video
        .filename
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let poster_url = video
        .poster_path
        .as_ref()
        .filter(|_| !poster_failed())
        .map(|p| {
            if p.starts_with('/') {
                format!("https://image.tmdb.org/t/p/w342{p}")
            } else {
                ipc::asset_url(p)
            }
        });
    let thumb_url = if video.thumb_ready == 1 {
        Some(ipc::asset_url(&format!("{thumb_dir}/{}.jpg", video.id)))
    } else {
        None
    };

    let video_id = video.id;
    let on_enter = move |_| {
        hovering.set(true);
        if sprite().is_some() || sprite_busy() {
            return;
        }
        sprite_busy.set(true);
        spawn(async move {
            if let Ok(path) =
                ipc::invoke::<_, String>("ensure_sprite", SpriteArgs { video_id }).await
            {
                sprite.set(Some(ipc::asset_url(&path)));
            }
            sprite_busy.set(false);
        });
    };

    rsx! {
        div {
            class: "card-video",
            onclick: move |_| on_open.call(v.clone()),
            onmouseenter: on_enter,
            onmouseleave: move |_| hovering.set(false),
            div { class: "poster",
                span { class: "poster-letter", "{letter}" }
                if let Some(url) = poster_url {
                    img {
                        class: "poster-img",
                        src: "{url}",
                        loading: "lazy",
                        alt: "",
                        onerror: move |_| poster_failed.set(true),
                    }
                } else if let Some(url) = thumb_url {
                    img { class: "poster-img", src: "{url}", loading: "lazy", alt: "" }
                }
                button {
                    class: "card-scrape",
                    title: "刮削元数据",
                    onclick: move |e| { e.stop_propagation(); on_scrape.call(vs.clone()); },
                    "刮",
                }
                button {
                    class: "card-tag",
                    title: "编辑标签",
                    onclick: move |e| { e.stop_propagation(); on_tag.call(vt.clone()); },
                    "签",
                }
                if hovering() {
                    if let Some(url) = sprite() {
                        div { class: "sprite", style: "background-image: url({url})" }
                    }
                }
                if let Some(d) = video.duration_ms.filter(|d| *d >= 0) {
                    span { class: "badge-duration", "{fmt_duration(d)}" }
                }
            }
            div { class: "card-title", title: "{video.filename}", "{display_title}" }
            div { class: "card-meta muted small",
                span { "{fmt_size(video.size)}" }
                if let Some(y) = video.year {
                    span { "{y}" }
                }
                if let (Some(w), Some(h)) = (video.width, video.height) {
                    span { "{w}×{h}" }
                }
            }
            if let (Some(pos), Some(dur)) = (video.progress_ms, video.duration_ms.filter(|d| *d > 0)) {
                if pos > 0 {
                    div { class: "card-progress",
                        div {
                            class: if pos * 100 / dur >= 95 { "card-progress-fill watched" } else { "card-progress-fill" },
                            style: "width: {(pos * 100 / dur).min(100)}%",
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn Pager(total: i64, page: i64, on_page: EventHandler<i64>) -> Element {
    let pages = (total + 99) / 100;
    if pages <= 1 {
        return rsx! { div { class: "pager muted small", "共 {total} 个视频" } };
    }
    rsx! {
        div { class: "pager",
            button {
                class: "btn small",
                disabled: page <= 1,
                onclick: move |_| on_page.call(page - 1),
                "上一页",
            }
            span { class: "muted small", "第 {page} / {pages} 页 · 共 {total} 个" }
            button {
                class: "btn small",
                disabled: page >= pages,
                onclick: move |_| on_page.call(page + 1),
                "下一页",
            }
        }
    }
}

#[component]
pub fn PlayerModal(
    video: Video,
    mpv: bool,
    ready: bool,
    err: Option<String>,
    state: MpvState,
    activity: Signal<u64>,
    controls_visible: Signal<bool>,
    volume: f64,
    mute: bool,
    on_close: EventHandler<()>,
) -> Element {
    let ext = video
        .path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let src = ipc::asset_url(&video.path);

    let mut failed = use_signal(|| false);
    let mut art_host = use_signal(|| None::<web_sys::Element>);
    let mut art_err_cb = use_signal(|| None::<Closure<dyn FnMut()>>);

    let video_id = video.id;
    let duration = if state.duration_ms > 0 {
        state.duration_ms
    } else {
        video.duration_ms.unwrap_or(0)
    };
    let paused = state.paused;
    let mut drag = use_signal(|| None::<i64>);
    let position = drag()
        .unwrap_or_else(|| state.position_ms.max(0))
        .clamp(0, duration.max(1));
    let mut speed = use_signal(|| 1.0f64);
    let mut activity = activity;
    let mut controls_visible = controls_visible;
    let mut wake = move || {
        activity.set(js_sys::Date::now() as u64);
        controls_visible.set(true);
    };
    let pct = if duration > 0 {
        (position * 100 / duration).clamp(0, 100)
    } else {
        0
    };
    use_hook(move || {
        spawn(async move {
            loop {
                delay_ms(500).await;
                if controls_visible() && js_sys::Date::now() as u64 - activity() >= 5000 {
                    controls_visible.set(false);
                }
            }
        });
    });
    let mut cycle_speed = move || {
        let idx = SPEEDS
            .iter()
            .position(|s| (*s - speed()).abs() < 0.01)
            .unwrap_or(0);
        let next = SPEEDS[(idx + 1) % SPEEDS.len()];
        speed.set(next);
        let value = format!("{next}");
        mpv_command(vec!["set", "speed", &value]);
    };

    // mpv 模式：占位 div 的物理像素坐标上报给后端，原生窗口覆盖在 webview 上
    let mut host = use_signal(|| None::<web_sys::Element>);
    let mut opened = use_signal(|| false);
    let mut open_err = use_signal(|| None::<String>);
    let mut resize_cb = use_signal(|| None::<Closure<dyn FnMut()>>);
    let mut fullscreen = use_signal(|| false);
    let mut key_cb = use_signal(|| None::<Closure<dyn FnMut(web_sys::KeyboardEvent)>>);

    let mut report = move |open: bool| {
        let Some(el) = host() else { return };
        let r = el.get_bounding_client_rect();
        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0);
        let x = (r.left() * dpr).round() as i32;
        let y = (r.top() * dpr).round() as i32;
        let w = (r.width() * dpr).round() as i32;
        let h = (r.height() * dpr).round() as i32;
        if w <= 0 || h <= 0 {
            return;
        }
        if open {
            if opened() {
                return;
            }
            opened.set(true);
            // spawn_forever：弹窗卸载会取消普通 spawn 的任务，关闭/打开指令必须脱离组件生命周期
            spawn_forever(async move {
                if let Err(e) = ipc::invoke::<_, ()>(
                    "mpv_open",
                    MpvOpenArgs { video_id, x, y, w, h },
                )
                .await
                {
                    open_err.set(Some(e));
                }
            });
        } else if opened() {
            spawn_forever(async move {
                let _ = ipc::invoke::<_, ()>("mpv_rect", MpvRectArgs { x, y, w, h }).await;
            });
        }
    };

    use_hook(move || {
        if mpv {
            let cb = Closure::new(move || report(false));
            if let Some(win) = web_sys::window() {
                let _ = win.add_event_listener_with_callback(
                    "resize",
                    cb.as_ref().unchecked_ref(),
                );
            }
            resize_cb.set(Some(cb));
        }
    });
    use_drop(move || {
        if let Some(win) = web_sys::window() {
            if let Some(cb) = resize_cb.read().as_ref() {
                let _ = win.remove_event_listener_with_callback(
                    "resize",
                    cb.as_ref().unchecked_ref(),
                );
            }
        }
    });

    let close = move || {
        if mpv {
            spawn_forever(async move {
                let _ = ipc::invoke_no_args::<()>("mpv_close").await;
            });
        } else if let Some(h) = art_host.read().as_ref() {
            crate::art::art_destroy(h);
        }
        on_close.call(());
    };

    use_drop(move || {
        if let Some(h) = art_host.read().as_ref() {
            crate::art::art_destroy(h);
        }
    });

    let mut toggle_fullscreen = move || {
        let on = !fullscreen();
        fullscreen.set(on);
        spawn_forever(async move {
            let _ = ipc::invoke::<_, ()>("mpv_fullscreen", MpvFullscreenArgs { on }).await;
        });
    };

    let set_volume = move |v: i64| {
        spawn_forever(async move {
            let _ = ipc::invoke::<_, ()>(
                "mpv_command",
                MpvCommandArgs { args: vec!["set".into(), "volume".into(), v.to_string()] },
            )
            .await;
        });
    };
    let toggle_mute = move || {
        spawn_forever(async move {
            let _ = ipc::invoke::<_, ()>(
                "mpv_command",
                MpvCommandArgs { args: vec!["cycle".into(), "mute".into()] },
            )
            .await;
        });
    };

    // 快捷键：webview 持有焦点，window 级 keydown → mpv 命令（嵌入下原生焦点不在 mpv 子窗口）
    use_hook(move || {
        if mpv {
            let cb = Closure::new(move |e: web_sys::KeyboardEvent| {
                if let Some(t) = e.target() {
                    if let Ok(el) = t.dyn_into::<web_sys::Element>() {
                        let tag = el.tag_name().to_ascii_lowercase();
                        let editable = el
                            .get_attribute("contenteditable")
                            .map(|v| v != "false")
                            .unwrap_or(false);
                        if tag == "input" || tag == "textarea" || tag == "select" || editable {
                            return;
                        }
                    }
                }
                let key = e.key();
                let k = key.as_str();
                let cmd: Option<Vec<String>> = match k {
                    " " | "k" | "K" => Some(vec!["cycle".into(), "pause".into()]),
                    "ArrowLeft" | "j" | "J" => Some(vec!["seek".into(), "-5".into(), "relative".into()]),
                    "ArrowRight" | "l" | "L" => Some(vec!["seek".into(), "5".into(), "relative".into()]),
                    "ArrowUp" => Some(vec!["add".into(), "volume".into(), "5".into()]),
                    "ArrowDown" => Some(vec!["add".into(), "volume".into(), "-5".into()]),
                    "m" | "M" => Some(vec!["cycle".into(), "mute".into()]),
                    _ => None,
                };
                if let Some(args) = cmd {
                    e.prevent_default();
                    let osd = match k {
                        "ArrowUp" | "ArrowDown" => Some("Volume: ${volume}%"),
                        "m" | "M" => Some("Mute: ${mute}"),
                        _ => None,
                    };
                    spawn_forever(async move {
                        let _ = ipc::invoke::<_, ()>("mpv_command", MpvCommandArgs { args }).await;
                        if let Some(text) = osd {
                            let _ = ipc::invoke::<_, ()>(
                                "mpv_command",
                                MpvCommandArgs { args: vec!["show-text".into(), text.into(), "1000".into()] },
                            )
                            .await;
                        }
                    });
                    return;
                }
                match k {
                    "f" | "F" => {
                        e.prevent_default();
                        toggle_fullscreen();
                    }
                    "Escape" => {
                        e.prevent_default();
                        if fullscreen() {
                            toggle_fullscreen();
                        } else {
                            close();
                        }
                    }
                    _ => {}
                }
            });
            if let Some(win) = web_sys::window() {
                let _ = win.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
            }
            key_cb.set(Some(cb));
        }
    });
    use_drop(move || {
        if let Some(win) = web_sys::window() {
            if let Some(cb) = key_cb.read().as_ref() {
                let _ = win.remove_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
            }
        }
    });

    // 全屏切换后布局变化，上报新矩形让原生容器铺满（窗口 resize 事件也会持续上报）
    use_effect(move || {
        let _ = fullscreen();
        report(false);
    });
    // 控制条显示/隐藏会改变视频区域高度，等布局完成后再同步原生窗口
    use_effect(move || {
        let _ = controls_visible();
        spawn(async move {
            delay_ms(60).await;
            report(false);
        });
    });

    rsx! {
        div {
            class: "modal-mask",
            onclick: move |_| close(),
            onmousemove: move |_| wake(),
            div {
                class: if fullscreen() { "modal player-modal player-fullscreen" } else { "modal player-modal" },
                onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h2 { "{video.filename}" }
                    div { class: "player-head-btns",
                        button { class: "toast-close", onclick: move |_| close(), "×" }
                    }
                }
                if mpv {
                    if let Some(e) = err.clone().or_else(|| open_err()) {
                        div { class: "player-fallback",
                            p { class: "muted", "{e}" }
                            button {
                                class: "btn",
                                onclick: move |_| {
                                    spawn(async move {
                                        let _ = ipc::invoke::<_, ()>(
                                            "open_external",
                                            VideoIdArgs { video_id },
                                        )
                                        .await;
                                    });
                                },
                                "用系统播放器打开",
                            }
                        }
                    } else {
                        div {
                            class: if controls_visible() {
                                "player-stage"
                            } else {
                                "player-stage controls-hidden"
                            },
                            div {
                                class: "player-video mpv-host",
                                onmounted: move |e| {
                                    if let Some(h) = e.data().downcast::<web_sys::Element>() {
                                        host.set(Some(h.clone()));
                                        report(true);
                                    }
                                },
                                if !ready {
                                    div { class: "mpv-loading", "正在加载播放器…" }
                                }
                            }
                            if ready {
                                div { class: "player-controls",
                                    div { class: "pc-row",
                                        button {
                                            class: "pc-btn",
                                            title: if paused { "播放" } else { "暂停" },
                                            onclick: move |_| mpv_toggle_pause(),
                                            if paused {
                                                svg { view_box: "0 0 24 24",
                                                    path { d: "M8 5v14l11-7z" }
                                                }
                                            } else {
                                                svg { view_box: "0 0 24 24",
                                                    path { d: "M6 4h4v16H6zM14 4h4v16h-4z" }
                                                }
                                            }
                                        }
                                        button {
                                            class: "pc-btn",
                                            title: "后退 10 秒",
                                            onclick: move |_| mpv_seek_relative(-10),
                                            svg { view_box: "0 0 24 24",
                                                path { d: "M6 5h2v14H6zM18 5v14l-9-7z" }
                                            }
                                        }
                                        button {
                                            class: "pc-btn",
                                            title: "前进 10 秒",
                                            onclick: move |_| mpv_seek_relative(10),
                                            svg { view_box: "0 0 24 24",
                                                path { d: "M16 5h2v14h-2zM6 5l9 7-9 7z" }
                                            }
                                        }
                                        button {
                                            class: "pc-btn",
                                            title: if mute { "取消静音" } else { "静音" },
                                            onclick: move |_| toggle_mute(),
                                            svg { view_box: "0 0 24 24",
                                                path { d: "M3 9v6h4l5 5V4L7 9H3z" }
                                                if mute {
                                                    path {
                                                        d: "M16 9l5 5M21 9l-5 5",
                                                        fill: "none",
                                                        stroke: "currentColor",
                                                        stroke_width: "2",
                                                    }
                                                } else {
                                                    path {
                                                        d: "M16 8a4 4 0 0 1 0 8",
                                                        fill: "none",
                                                        stroke: "currentColor",
                                                        stroke_width: "2",
                                                    }
                                                }
                                            }
                                        }
                                        div { class: "pc-vol-wrap",
                                            div { class: "pc-vol-track" }
                                            div {
                                                class: "pc-vol-fill",
                                                style: "width: {volume as i64}%",
                                            }
                                            input {
                                                class: "pc-vol",
                                                r#type: "range",
                                                min: "0",
                                                max: "100",
                                                step: "1",
                                                value: "{volume as i64}",
                                                oninput: move |e| {
                                                    if let Ok(v) = e.value().parse::<i64>() {
                                                        set_volume(v);
                                                    }
                                                },
                                            }
                                        }
                                        span { class: "pc-vol-label", "{volume as i64}%" }
                                        span { class: "pc-time",
                                            "{fmt_duration(position)} / {fmt_duration(duration)}"
                                        }
                                        div { class: "pc-spacer" }
                                        button {
                                            class: "pc-btn",
                                            title: "截图",
                                            onclick: move |_| mpv_command(vec!["screenshot"]),
                                            svg { view_box: "0 0 24 24",
                                                path { d: "M9 4l-1.5 2H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-3.5L15 4H9zm3 5a5 5 0 1 1 0 10 5 5 0 0 1 0-10zm0 2a3 3 0 1 0 0 6 3 3 0 0 0 0-6z" }
                                            }
                                        }
                                        button {
                                            class: "pc-btn",
                                            title: "播放速度",
                                            onclick: move |_| cycle_speed(),
                                            svg { view_box: "0 0 24 24",
                                                path { d: "M19.4 13a7.8 7.8 0 0 0 0-2l2.1-1.7-2-3.4-2.5 1a7.6 7.6 0 0 0-1.7-1L15 3h-4l-.3 2.9a7.6 7.6 0 0 0-1.7 1l-2.5-1-2 3.4L6.6 11a7.8 7.8 0 0 0 0 2l-2.1 1.7 2 3.4 2.5-1a7.6 7.6 0 0 0 1.7 1L11 21h4l.3-2.9a7.6 7.6 0 0 0 1.7-1l2.5 1 2-3.4-2.1-1.7zM13 15.5a3.5 3.5 0 1 1 0-7 3.5 3.5 0 0 1 0 7z" }
                                            }
                                        }
                                        if speed() != 1.0 {
                                            span { class: "pc-speed", "{speed()}x" }
                                        }
                                        button {
                                            class: "pc-btn",
                                            title: if fullscreen() { "退出全屏" } else { "全屏" },
                                            onclick: move |_| toggle_fullscreen(),
                                            svg { view_box: "0 0 24 24",
                                                path { d: "M4 4h6v2H6v4H4V4zm10 0h6v6h-2V6h-4V4zM4 14h2v4h4v2H4v-6zm14 0h2v6h-6v-2h4v-4z" }
                                            }
                                        }
                                    }
                                    div { class: "pc-progress-wrap",
                                        div { class: "pc-progress-track" }
                                        div {
                                            class: "pc-progress-fill",
                                            style: "width: {pct}%",
                                        }
                                        input {
                                            class: "pc-progress",
                                            r#type: "range",
                                            min: "0",
                                            max: "{duration.max(1)}",
                                            step: "100",
                                            value: "{position}",
                                            oninput: move |e| {
                                                if let Ok(v) = e.value().parse::<i64>() {
                                                    drag.set(Some(v));
                                                    mpv_seek_to(v, false);
                                                }
                                            },
                                            onchange: move |e| {
                                                if let Ok(v) = e.value().parse::<i64>() {
                                                    drag.set(None);
                                                    mpv_seek_to(v, true);
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if !failed() {
                    div {
                        class: "player-video art-host",
                        onmounted: move |e| {
                            if let Some(h) = e.data().downcast::<web_sys::Element>() {
                                art_host.set(Some(h.clone()));
                                let cont_err = h.clone();
                                let cb = Closure::new(move || {
                                    crate::art::art_destroy(&cont_err);
                                    failed.set(true);
                                });
                                art_err_cb.set(Some(cb));
                                let cont = h.clone();
                                let url = src.clone();
                                let art_url = asset!("/assets/artplayer.min.js").to_string();
                                let glue_url = asset!("/assets/vm-art.js").to_string();
                                spawn(async move {
                                    if crate::art::ensure_loaded(&art_url, &glue_url)
                                        .await
                                        .is_ok()
                                    {
                                        if let Some(cb) = art_err_cb.read().as_ref() {
                                            crate::art::art_create(&cont, &url, video_id, cb);
                                        }
                                    }
                                });
                            }
                        },
                    }
                } else {
                    div { class: "player-fallback",
                        p { class: "muted", "此格式（{ext}）无法内置播放或解码失败" }
                        button {
                            class: "btn",
                            onclick: move |_| {
                                spawn(async move {
                                    let _ = ipc::invoke::<_, ()>(
                                        "open_external",
                                        VideoIdArgs { video_id },
                                    )
                                    .await;
                                });
                            },
                            "用系统播放器打开",
                        }
                    }
                }
                if !fullscreen() {
                    div { class: "card-meta muted small",
                        span { "{fmt_size(video.size)}" }
                        if let Some(d) = video.duration_ms.filter(|d| *d >= 0) {
                            span { "{fmt_duration(d)}" }
                        }
                        if let (Some(w), Some(h)) = (video.width, video.height) {
                            span { "{w}×{h}" }
                        }
                    }
                    if let Some(t) = video.title.clone() {
                        p { class: "scraped-title",
                            "{t}"
                            if let Some(y) = video.year {
                                span { class: "muted small", "（{y}）" }
                            }
                        }
                    }
                    if let Some(plot) = video.plot.clone() {
                        p { class: "muted small scraped-plot", "{plot}" }
                    }
                }
            }
        }
    }
}