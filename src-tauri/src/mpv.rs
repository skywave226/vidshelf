use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use libloading::Library;
use tauri::{AppHandle, Emitter, Manager, State};
use video_manager_core::{progress, rusqlite::params, settings};

use crate::AppState;

// 供容器 wndproc（主线程）取当前会话发命令（点击画面暂停）
static APP: OnceLock<AppHandle> = OnceLock::new();
// wndproc 里不能直接 emit（会阻塞窗口消息循环），只置位，由 mpv 事件线程上报
static ACTIVITY_PENDING: AtomicBool = AtomicBool::new(false);

const MPV_FORMAT_STRING: c_int = 1;
const MPV_FORMAT_FLAG: c_int = 3;
const MPV_FORMAT_DOUBLE: c_int = 5;

const EV_SHUTDOWN: c_int = 1;
const EV_END_FILE: c_int = 7;
const EV_FILE_LOADED: c_int = 8;
const EV_PROPERTY_CHANGE: c_int = 22;

#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct MpvEventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct MpvEventEndFile {
    reason: c_int,
    error: c_int,
}

pub struct MpvApi {
    _lib: Library,
    create: unsafe extern "C" fn() -> *mut c_void,
    initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    set_option_string: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    observe_property: unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int,
    wait_event: unsafe extern "C" fn(*mut c_void, f64) -> *mut MpvEvent,
    free: unsafe extern "C" fn(*mut c_void),
    terminate_destroy: unsafe extern "C" fn(*mut c_void),
}

unsafe impl Send for MpvApi {}
unsafe impl Sync for MpvApi {}

impl MpvApi {
    pub fn load(candidates: &[String]) -> Result<Self, String> {
        let mut last = String::from("no candidates");
        for path in candidates {
            let lib = match unsafe { Library::new(path) } {
                Ok(l) => l,
                Err(e) => {
                    last = format!("{path}: {e}");
                    continue;
                }
            };
            macro_rules! sym {
                ($name:expr, $t:ty) => {
                    match unsafe { lib.get::<$t>($name) } {
                        Ok(s) => *s,
                        Err(e) => {
                            last = format!("{path}: {e}");
                            continue;
                        }
                    }
                };
            }
            // SAFETY: signatures mirror mpv/client.h; fn pointers are copied out of the Symbols.
            let api = Self {
                    create: sym!(b"mpv_create\0", unsafe extern "C" fn() -> *mut c_void),
                    initialize: sym!(b"mpv_initialize\0", unsafe extern "C" fn(*mut c_void) -> c_int),
                    set_option_string: sym!(b"mpv_set_option_string\0", unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int),
                    command: sym!(b"mpv_command\0", unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int),
                    get_property: sym!(b"mpv_get_property\0", unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int),
                    observe_property: sym!(b"mpv_observe_property\0", unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int),
                    wait_event: sym!(b"mpv_wait_event\0", unsafe extern "C" fn(*mut c_void, f64) -> *mut MpvEvent),
                    free: sym!(b"mpv_free\0", unsafe extern "C" fn(*mut c_void)),
                    terminate_destroy: sym!(b"mpv_terminate_destroy\0", unsafe extern "C" fn(*mut c_void)),
                    _lib: lib,
            };
            println!("[mpv] loaded: {path}");
            return Ok(api);
        }
        Err(last)
    }

    pub fn version(&self) -> Option<String> {
        unsafe {
            let ctx = (self.create)();
            if ctx.is_null() {
                return None;
            }
            // 此构建要求 initialize 后才能读属性；idle 初始化不会创建窗口
            let _ = self.opt(ctx, "load-scripts", "no");
            let _ = self.opt(ctx, "terminal", "no");
            if (self.initialize)(ctx) < 0 {
                (self.terminate_destroy)(ctx);
                return None;
            }
            let name = CString::new("mpv-version").unwrap();
            let mut ptr: *mut c_char = std::ptr::null_mut();
            let rc = (self.get_property)(ctx, name.as_ptr(), MPV_FORMAT_STRING, &mut ptr as *mut _ as *mut c_void);
            let v = if rc >= 0 && !ptr.is_null() {
                let s = CStr::from_ptr(ptr).to_string_lossy().into_owned();
                (self.free)(ptr as *mut c_void);
                Some(s)
            } else {
                println!("[mpv] version query failed: rc={rc}");
                None
            };
            (self.terminate_destroy)(ctx);
            v
        }
    }

    fn opt(&self, ctx: *mut c_void, name: &str, value: &str) -> Result<(), String> {
        let n = CString::new(name).unwrap();
        let v = CString::new(value).unwrap();
        let rc = unsafe { (self.set_option_string)(ctx, n.as_ptr(), v.as_ptr()) };
        if rc < 0 {
            Err(format!("mpv option {name}={value} failed: {rc}"))
        } else {
            Ok(())
        }
    }

    fn cmd(&self, ctx: *mut c_void, args: &[&str]) -> Result<(), String> {
        let owned: Vec<CString> = args.iter().map(|a| CString::new(*a).unwrap()).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        let rc = unsafe { (self.command)(ctx, ptrs.as_ptr()) };
        if rc < 0 {
            Err(format!("mpv command {:?} failed: {rc}", args))
        } else {
            Ok(())
        }
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicI32, AtomicI64, AtomicIsize, Ordering};
    use tauri::Manager;

    pub const WS_CHILD: u32 = 0x4000_0000;
    // STATIC 默认 hit-test 透明（HTTRANSPARENT），鼠标会穿透到 webview；SS_NOTIFY 使其可命中
    pub const SS_NOTIFY: u32 = 0x0100;
    pub const SWP_SHOWWINDOW: u32 = 0x0040;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const SW_HIDE: u32 = 0;
    const GWLP_WNDPROC: i32 = -4;
    const WM_MOUSEMOVE: u32 = 0x0200;
    const WM_MOUSELEAVE: u32 = 0x02A3;
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_LBUTTONUP: u32 = 0x0202;

    static ORIG_WNDPROC: AtomicIsize = AtomicIsize::new(0);
    static ORIG_MPV_WNDPROC: AtomicIsize = AtomicIsize::new(0);
    static DOWN_X: AtomicI32 = AtomicI32::new(0);
    static DOWN_Y: AtomicI32 = AtomicI32::new(0);
    static DOWN_T: AtomicI64 = AtomicI64::new(0);
    static LAST_ACTIVE: AtomicI64 = AtomicI64::new(0);

    fn lparam_x(l: isize) -> i32 {
        ((l as u32) & 0xFFFF) as u16 as i16 as i32
    }
    fn lparam_y(l: isize) -> i32 {
        (((l as u32) >> 16) & 0xFFFF) as u16 as i16 as i32
    }
    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    // 鼠标活动节流置位，由 mpv 事件线程异步上报，避免在 wndproc 内阻塞
    fn notify_activity() {
        let now = now_ms();
        if now - LAST_ACTIVE.load(Ordering::Relaxed) >= 100 {
            LAST_ACTIVE.store(now, Ordering::Relaxed);
            super::ACTIVITY_PENDING.store(true, Ordering::Relaxed);
        }
    }

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn CreateWindowExW(
            exstyle: u32,
            class: *const u16,
            name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: isize,
            menu: isize,
            inst: isize,
            param: *mut c_void,
        ) -> isize;
        pub fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
        pub fn ShowWindow(hwnd: isize, cmd: u32) -> i32;
        pub fn DestroyWindow(hwnd: isize) -> i32;
        fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
        fn CallWindowProcW(prev: isize, hwnd: isize, msg: u32, w: usize, l: isize) -> isize;
        fn FindWindowExW(parent: isize, after: isize, class: *const u16, name: *const u16) -> isize;
        fn SendMessageW(hwnd: isize, msg: u32, w: usize, l: isize) -> isize;
        fn GetCursorPos(pt: *mut Point) -> i32;
        fn GetWindowRect(hwnd: isize, rc: *mut Rect) -> i32;
        fn GetClientRect(hwnd: isize, rc: *mut Rect) -> i32;
    }

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    // OS hit-test 停在 STATIC 容器层、不进入 mpv 子窗口；容器收到鼠标后转发给 mpv（同原点，坐标不变）
    unsafe extern "system" fn host_wndproc(hwnd: isize, msg: u32, w: usize, l: isize) -> isize {
        if msg == WM_MOUSEMOVE {
            notify_activity();
        }
        if (0x0200..=0x020E).contains(&msg) {
            let class = wide("mpv");
            let child = FindWindowExW(hwnd, 0, class.as_ptr(), std::ptr::null());
            if child != 0 {
                if ORIG_MPV_WNDPROC.load(Ordering::Relaxed) == 0 {
                    let orig = SetWindowLongPtrW(child, GWLP_WNDPROC, mpv_wndproc as isize);
                    if orig != 0 {
                        ORIG_MPV_WNDPROC.store(orig, Ordering::Relaxed);
                        println!("[mpv] subclassed mpv child hwnd={child}");
                    }
                }
                return SendMessageW(child, msg, w, l);
            }
        }
        CallWindowProcW(ORIG_WNDPROC.load(Ordering::Relaxed), hwnd, msg, w, l)
    }

    // OS 的 leave 判定按 hit-test 目标（STATIC 容器）而非 mpv 矩形：转发鼠标后每次移动都误发
    // WM_MOUSELEAVE，OSC 的 mouse_in_window 被反复清零、点击坐标变 (-1,-1) 而落空。
    // 光标仍在 mpv 矩形内时吞掉误发的 leave，真实离开才放行。
    unsafe extern "system" fn mpv_wndproc(hwnd: isize, msg: u32, w: usize, l: isize) -> isize {
        // 鼠标直接落在 mpv 子窗口时也要上报活动（否则隐藏后移入画面无法唤醒控制条）
        if (0x0200..=0x020E).contains(&msg) {
            notify_activity();
        }
        // 单击暂停判定放在子窗口：OSC 按下时 SetCapture，抬起事件直达子窗口而非容器，
        // 容器侧拿不到 WM_LBUTTONUP。DOWN（转发或直达）与 UP 都会到这里。
        if msg == WM_LBUTTONDOWN {
            DOWN_X.store(lparam_x(l), Ordering::Relaxed);
            DOWN_Y.store(lparam_y(l), Ordering::Relaxed);
            DOWN_T.store(now_ms(), Ordering::Relaxed);
        } else if msg == WM_LBUTTONUP {
            let dx = lparam_x(l) - DOWN_X.load(Ordering::Relaxed);
            let dy = lparam_y(l) - DOWN_Y.load(Ordering::Relaxed);
            let dt = now_ms() - DOWN_T.load(Ordering::Relaxed);
            // 位移<6px、时长<400ms 才算单击（排除拖动）；OSC 已关闭，整个画面都可点击暂停
            let pause = dx * dx + dy * dy < 36 && dt < 400;
            if pause {
                if let Some(app) = super::APP.get() {
                    if let Some(s) = app.state::<super::AppState>().mpv.lock().unwrap().as_ref() {
                        let _ = s.command(&["cycle", "pause"]);
                    }
                }
            }
        }
        if msg == WM_MOUSELEAVE {
            let mut pt = Point { x: 0, y: 0 };
            let mut rc = Rect { left: 0, top: 0, right: 0, bottom: 0 };
            if GetCursorPos(&mut pt) != 0
                && GetWindowRect(hwnd, &mut rc) != 0
                && pt.x >= rc.left
                && pt.x < rc.right
                && pt.y >= rc.top
                && pt.y < rc.bottom
            {
                return 0;
            }
        }
        CallWindowProcW(ORIG_MPV_WNDPROC.load(Ordering::Relaxed), hwnd, msg, w, l)
    }

    pub fn subclass(hwnd: isize) {
        unsafe {
            ORIG_MPV_WNDPROC.store(0, Ordering::Relaxed);
            let orig = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, host_wndproc as isize);
            ORIG_WNDPROC.store(orig, Ordering::Relaxed);
        }
    }

    pub fn create_container(parent: isize, x: i32, y: i32, w: i32, h: i32) -> isize {
        let class = wide("STATIC");
        unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                WS_CHILD | SS_NOTIFY,
                x,
                y,
                w,
                h,
                parent,
                0,
                0,
                std::ptr::null_mut(),
            )
        }
    }

    pub fn place(hwnd: isize, x: i32, y: i32, w: i32, h: i32) {
        unsafe {
            SetWindowPos(hwnd, 0, x, y, w, h, SWP_SHOWWINDOW | SWP_NOACTIVATE);
        }
    }

    // 不带 SWP_SHOWWINDOW：关闭流程中已隐藏的容器不会被 resize 重新显示
    pub fn resize(hwnd: isize, x: i32, y: i32, w: i32, h: i32) {
        unsafe {
            SetWindowPos(hwnd, 0, x, y, w, h, SWP_NOACTIVATE);
        }
    }

    pub fn hide(hwnd: isize) {
        if hwnd != 0 {
            unsafe {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    pub fn destroy(hwnd: isize) {
        if hwnd != 0 {
            unsafe {
                DestroyWindow(hwnd);
            }
        }
    }
}

pub struct MpvSession {
    ctx: *mut c_void,
    container: isize,
    api: Arc<MpvApi>,
}

unsafe impl Send for MpvSession {}
unsafe impl Sync for MpvSession {}

impl MpvSession {
    fn quit(&self) {
        let _ = self.api.cmd(self.ctx, &["quit"]);
    }

    pub fn command(&self, args: &[&str]) -> Result<(), String> {
        self.api.cmd(self.ctx, args)
    }
}

fn dll_candidates(app: &AppHandle) -> Vec<String> {
    let mut v = Vec::new();
    if let Ok(res) = app.path().resource_dir() {
        v.push(res.join("lib/libmpv-2.dll").to_string_lossy().into_owned());
        v.push(res.join("libmpv-2.dll").to_string_lossy().into_owned());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.join("libmpv-2.dll").to_string_lossy().into_owned());
            v.push(dir.join("lib/libmpv-2.dll").to_string_lossy().into_owned());
        }
    }
    v.push("libmpv-2.dll".to_string());
    v
}

pub fn init_api(app: &AppHandle) -> (Option<Arc<MpvApi>>, Option<String>) {
    let _ = APP.set(app.clone());
    match MpvApi::load(&dll_candidates(app)) {
        Ok(api) => {
            let version = api.version();
            println!("[mpv] version: {}", version.as_deref().unwrap_or("?"));
            let api = Arc::new(api);
            (Some(api), version)
        }
        Err(e) => {
            println!("[mpv] unavailable: {e}");
            (None, None)
        }
    }
}

#[tauri::command]
pub fn mpv_open(
    app: AppHandle,
    state: State<'_, AppState>,
    video_id: i64,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Result<(), String> {
    println!("[mpv] open request: id={video_id} rect={x},{y},{w}x{h}");
    let api = state.mpv_api.clone().ok_or("libmpv 不可用")?;

    let old = {
        let guard = state.mpv.lock().unwrap();
        guard.as_ref().map(|s| (s.ctx, s.container, s.api.clone()))
    };
    if let Some((ctx, old_container, api)) = old {
        let _ = api.cmd(ctx, &["quit"]);
        let _ = app.run_on_main_thread(move || win::hide(old_container));
        for _ in 0..150 {
            if state.mpv.lock().unwrap().is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if state.mpv.lock().unwrap().is_some() {
            return Err("播放器正忙".into());
        }
    }

    let (path, resume_ms, vol, muted) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let (path, dur): (String, i64) = conn
            .query_row(
                "SELECT path, COALESCE(duration_ms, 0) FROM videos WHERE id = ?1",
                params![video_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| "视频不存在".to_string())?;
        let pos = progress::get(&conn, video_id)
            .map_err(|e| e.to_string())?
            .map(|p| p.position_ms)
            .unwrap_or(0);
        let resume = if pos > 3000 && dur > 0 && pos < dur - 5000 { pos } else { 0 };
        let (vol, muted) = settings::get_player_volume(&conn);
        (path, resume, vol, muted)
    };

    let parent = app
        .get_webview_window("main")
        .ok_or("主窗口不存在")?
        .hwnd()
        .map_err(|e| e.to_string())?
        .0 as isize;
    // 容器窗口必须属于主线程：命令线程无消息泵，跨线程 SetWindowPos/DestroyWindow 会挂起或失败
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let h = win::create_container(parent, x, y, w, h);
        if h != 0 {
            win::subclass(h);
        }
        let _ = tx.send(h);
    })
    .map_err(|e| e.to_string())?;
    let container = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "创建播放窗口超时".to_string())?;
    if container == 0 {
        return Err("创建播放窗口失败".into());
    }
    println!("[mpv] container hwnd={container}");

    let ctx = unsafe { (api.create)() };
    if ctx.is_null() {
        let _ = app.run_on_main_thread(move || win::destroy(container));
        return Err("mpv_create 失败".into());
    }

    // 截图统一落到 data/screenshots，避免污染工作目录
    let shot_dir = std::path::Path::new(&state.thumb_dir)
        .parent()
        .map(|p| p.join("screenshots"))
        .unwrap_or_else(|| std::path::PathBuf::from("screenshots"));
    let _ = std::fs::create_dir_all(&shot_dir);
    let shot_dir = shot_dir.to_string_lossy().into_owned();

    let setup = || -> Result<(), String> {
        api.opt(ctx, "wid", &container.to_string())?;
        let _ = api.opt(ctx, "screenshot-directory", &shot_dir);
        // libmpv 默认不读用户配置；no-config 是 CLI 专用标志，API 设置会报 -5
        api.opt(ctx, "load-scripts", "no")?;
        api.opt(ctx, "terminal", "no")?;
        api.opt(ctx, "keep-open", "always")?;
        api.opt(ctx, "save-position-on-quit", "no")?;
        api.opt(ctx, "hwdec", "auto-safe")?;
        api.opt(ctx, "ytdl", "no")?;
        api.opt(ctx, "force-window", "immediate")?;
        // OSC 关闭：控件由 webview 自绘，外观与 ArtPlayer 保持一致
        api.opt(ctx, "osc", "no")?;
        api.opt(ctx, "osd-bar", "no")?;
        // 音量统一：以上次持久化的音量/静音启动（跨视频/重启一致）
        api.opt(ctx, "volume", &vol.to_string())?;
        api.opt(ctx, "mute", if muted { "yes" } else { "no" })?;
        if resume_ms > 0 {
            api.opt(ctx, "start", &format!("{}", resume_ms as f64 / 1000.0))?;
        }
        unsafe {
            let observe = |name: &str, fmt: c_int, ud: u64| -> Result<(), String> {
                let n = CString::new(name).unwrap();
                let rc = (api.observe_property)(ctx, ud, n.as_ptr(), fmt);
                if rc < 0 { Err(format!("observe {name}: {rc}")) } else { Ok(()) }
            };
            observe("time-pos", MPV_FORMAT_DOUBLE, 1)?;
            observe("duration", MPV_FORMAT_DOUBLE, 2)?;
            observe("pause", MPV_FORMAT_FLAG, 3)?;
            observe("volume", MPV_FORMAT_DOUBLE, 4)?;
            observe("mute", MPV_FORMAT_FLAG, 5)?;
            let rc = (api.initialize)(ctx);
            if rc < 0 {
                return Err(format!("mpv_initialize: {rc}"));
            }
        }
        api.cmd(ctx, &["loadfile", &path])?;
        Ok(())
    };
    if let Err(e) = setup() {
        println!("[mpv] setup failed: {e}");
        unsafe { (api.terminate_destroy)(ctx) };
        let _ = app.run_on_main_thread(move || win::destroy(container));
        return Err(e);
    }
    app.run_on_main_thread(move || win::place(container, x, y, w, h))
        .map_err(|e| e.to_string())?;
    println!("[mpv] open: id={video_id} resume={resume_ms}ms");

    let session = Arc::new(MpvSession { ctx, container, api: api.clone() });
    *state.mpv.lock().unwrap() = Some(session.clone());

    let sess = session.clone();
    let handle = app.clone();
    std::thread::spawn(move || {
        let ctx = sess.ctx;
        let session_api = sess.api.clone();
        let mut pos_ms: i64 = 0;
        let mut dur_ms: i64 = 0;
        let mut paused = false;
        let mut last_saved: i64 = -1;
        let mut last_emit = std::time::Instant::now();
        let mut volume: f64 = 100.0;
        let mut mute = false;
        let emit_state = |pos: i64, dur: i64, paused: bool| {
            let _ = handle.emit(
                "mpv-state",
                serde_json::json!({
                    "position_ms": pos,
                    "duration_ms": dur,
                    "paused": paused,
                }),
            );
        };
        let mut save = |pos: i64, min_delta: i64| {
            if pos > 0 && (pos - last_saved).abs() >= min_delta {
                last_saved = pos;
                if let Ok(conn) = handle.state::<AppState>().db.lock() {
                    let _ = progress::save(&conn, video_id, pos);
                }
            }
        };
        loop {
            let ev = unsafe { (session_api.wait_event)(ctx, 0.2) };
            if ACTIVITY_PENDING.swap(false, Ordering::Relaxed) {
                let _ = handle.emit("mpv-activity", true);
            }
            if ev.is_null() {
                continue;
            }
            let ev = unsafe { &*ev };
            match ev.event_id {
                EV_SHUTDOWN => break,
                EV_FILE_LOADED => {
                    let _ = handle.emit("mpv-ready", video_id);
                    emit_state(pos_ms, dur_ms, paused);
                }
                EV_END_FILE => {
                    let ef = unsafe { &*(ev.data as *const MpvEventEndFile) };
                    if ef.reason == 0 && dur_ms > 0 {
                        save(dur_ms, 0);
                        paused = true;
                        emit_state(dur_ms, dur_ms, true);
                    } else if ef.reason == 4 {
                        let _ = handle.emit("mpv-error", "播放失败（解码错误）");
                    }
                }
                EV_PROPERTY_CHANGE => {
                    let prop = unsafe { &*(ev.data as *const MpvEventProperty) };
                    if prop.data.is_null() {
                        continue;
                    }
                    let name = unsafe { CStr::from_ptr(prop.name) }.to_string_lossy();
                    match name.as_ref() {
                        "time-pos" if prop.format == MPV_FORMAT_DOUBLE => {
                            pos_ms = (unsafe { *(prop.data as *const f64) } * 1000.0) as i64;
                            save(pos_ms, 5000);
                            // 节流上报，避免每帧 IPC；进度条 5Hz 刷新已足够
                            if last_emit.elapsed().as_millis() >= 200 {
                                last_emit = std::time::Instant::now();
                                emit_state(pos_ms, dur_ms, paused);
                            }
                        }
                        "duration" if prop.format == MPV_FORMAT_DOUBLE => {
                            dur_ms = (unsafe { *(prop.data as *const f64) } * 1000.0) as i64;
                            last_emit = std::time::Instant::now();
                            emit_state(pos_ms, dur_ms, paused);
                        }
                        "pause" if prop.format == MPV_FORMAT_FLAG => {
                            paused = unsafe { *(prop.data as *const c_int) } != 0;
                            save(pos_ms, 1000);
                            last_emit = std::time::Instant::now();
                            emit_state(pos_ms, dur_ms, paused);
                        }
                        "volume" if prop.format == MPV_FORMAT_DOUBLE => {
                            volume = unsafe { *(prop.data as *const f64) };
                            let _ = handle.emit("mpv-volume", serde_json::json!({"volume": volume, "mute": mute}));
                        }
                        "mute" if prop.format == MPV_FORMAT_FLAG => {
                            mute = unsafe { *(prop.data as *const c_int) } != 0;
                            let _ = handle.emit("mpv-volume", serde_json::json!({"volume": volume, "mute": mute}));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        save(pos_ms, 0);
        if let Ok(conn) = handle.state::<AppState>().db.lock() {
            let _ = settings::save_player_volume(&conn, volume, mute);
        }
        unsafe { (session_api.terminate_destroy)(ctx) };
        let container = sess.container;
        let _ = handle.run_on_main_thread(move || win::destroy(container));
        *handle.state::<AppState>().mpv.lock().unwrap() = None;
        let _ = handle.emit("mpv-closed", video_id);
        println!("[mpv] closed: id={video_id} pos={pos_ms}ms");
    });

    Ok(())
}

#[tauri::command]
pub fn mpv_rect(
    app: AppHandle,
    state: State<'_, AppState>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Result<(), String> {
    let container = state.mpv.lock().unwrap().as_ref().map(|s| s.container);
    if let Some(c) = container {
        app.run_on_main_thread(move || win::resize(c, x, y, w, h))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn mpv_close(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let sess = state.mpv.lock().unwrap().clone();
    if let Some(s) = sess {
        s.quit();
        // 立即隐藏，避免 mpv 关闭线程销毁容器前视频块残留在界面上
        let c = s.container;
        let _ = app.run_on_main_thread(move || win::hide(c));
    }
    Ok(())
}

#[tauri::command]
pub fn mpv_command(state: State<'_, AppState>, args: Vec<String>) -> Result<(), String> {
    let sess = state.mpv.lock().unwrap().clone();
    let s = sess.ok_or("播放器未打开")?;
    let refs: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
    s.command(&refs)
}

// 全屏由前端布局 + 主窗口 set_fullscreen 驱动，容器随上报矩形铺满；
// mpv 嵌入(wid=)下 cycle fullscreen 会被父窗口裁剪，不可靠
#[tauri::command]
pub fn mpv_fullscreen(app: AppHandle, on: bool) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or("主窗口不存在")?
        .set_fullscreen(on)
        .map_err(|e| e.to_string())
}