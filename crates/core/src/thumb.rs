use std::path::Path;
use std::process::Command;

const SPRITE_FRAMES: usize = 16;
const CELL_W: i32 = 240;
const CELL_H: i32 = 136;

fn ffmpeg_cmd() -> Command {
    let mut cmd = Command::new("ffmpeg");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS：后台任务不抢前台 CPU
        cmd.creation_flags(0x0800_0000 | 0x0000_4000);
    }
    cmd
}

pub fn generate_thumb(video: &Path, duration_ms: i64, out: &Path) -> bool {
    let seek = if duration_ms > 0 {
        duration_ms as f64 / 1000.0 * 0.1
    } else {
        1.0
    };
    let mut cmd = ffmpeg_cmd();
    // 解码/滤镜/编码都限制为单线程，避免 ffmpeg 默认吃满所有核心
    cmd.args(["-y", "-loglevel", "error", "-filter_threads", "1", "-threads", "1"])
        .args(["-ss", &seek.to_string(), "-i"])
        .arg(video)
        .args(["-frames:v", "1", "-vf", "scale=320:-2", "-threads", "1", "-q:v", "4"])
        .arg(out);
    cmd.status().map(|s| s.success()).unwrap_or(false) && out.exists()
}

pub fn generate_sprite(video: &Path, duration_ms: i64, out: &Path) -> bool {
    let dur = if duration_ms > 0 {
        duration_ms as f64 / 1000.0
    } else {
        1.0
    };
    // 每个小图固定 240x136，4x4 排列；按时间点分别 seek 抽帧，
    // 避免 fps/tile 方案对整段视频做全量解码（长视频 CPU 会失控）。
    let mut cmd = ffmpeg_cmd();
    cmd.args(["-y", "-loglevel", "error", "-filter_threads", "1"]);
    let mut filter = String::new();
    for i in 0..SPRITE_FRAMES {
        let t = dur * (i as f64 + 0.5) / SPRITE_FRAMES as f64;
        cmd.args([
            "-ss",
            &format!("{t:.3}"),
            "-noaccurate_seek",
            "-threads",
            "1",
            "-i",
        ])
        .arg(video);
        filter.push_str(&format!(
            "[{i}:v]scale={CELL_W}:{CELL_H}:force_original_aspect_ratio=decrease,pad={CELL_W}:{CELL_H}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuvj420p[v{i}];"
        ));
    }
    let layout = (0..SPRITE_FRAMES)
        .map(|i| format!("{}_{}", (i % 4) as i32 * CELL_W, (i / 4) as i32 * CELL_H))
        .collect::<Vec<_>>()
        .join("|");
    for i in 0..SPRITE_FRAMES {
        filter.push_str(&format!("[v{i}]"));
    }
    filter.push_str(&format!(
        "xstack=inputs={SPRITE_FRAMES}:layout={layout}:fill=black[out]"
    ));
    cmd.args(["-filter_complex", &filter, "-map", "[out]"])
        .args(["-frames:v", "1", "-threads", "1", "-q:v", "5"])
        .arg(out);
    cmd.status().map(|s| s.success()).unwrap_or(false) && out.exists()
}