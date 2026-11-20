use std::path::Path;
use std::process::Command;

use serde::Deserialize;

#[derive(Debug)]
pub struct ProbeInfo {
    pub duration_ms: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

#[derive(Deserialize)]
struct FfprobeOut {
    format: Option<Format>,
    streams: Option<Vec<Stream>>,
}

#[derive(Deserialize)]
struct Format {
    duration: Option<String>,
}

#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
}

pub fn probe(path: &Path) -> Option<ProbeInfo> {
    let mut cmd = Command::new("ffprobe");
    cmd.args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let parsed: FfprobeOut = serde_json::from_slice(&out.stdout).ok()?;
    let duration_ms = parsed
        .format?
        .duration?
        .parse::<f64>()
        .ok()
        .map(|s| (s * 1000.0) as i64)?;
    let video = parsed
        .streams
        .unwrap_or_default()
        .into_iter()
        .find(|s| s.codec_type.as_deref() == Some("video"));
    Some(ProbeInfo {
        duration_ms,
        width: video.as_ref().and_then(|s| s.width),
        height: video.as_ref().and_then(|s| s.height),
    })
}