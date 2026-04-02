use regex::Regex;
use std::sync::LazyLock;

pub struct ParsedProgress {
    pub percent: f32,
    pub current_frame: Option<u64>,
    pub speed: Option<String>,
    pub eta_seconds: Option<f64>,
}

static FRAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"frame=\s*(\d+)").unwrap());
static TIME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"time=(\d+):(\d+):(\d+)\.(\d+)").unwrap());
static SPEED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"speed=\s*([\d.]+)x").unwrap());

pub fn parse_progress_line(line: &str, total_duration: f64) -> Option<ParsedProgress> {
    // We need at least the time field to compute progress
    let time_caps = TIME_RE.captures(line)?;

    let hours: f64 = time_caps[1].parse().unwrap_or(0.0);
    let minutes: f64 = time_caps[2].parse().unwrap_or(0.0);
    let seconds: f64 = time_caps[3].parse().unwrap_or(0.0);
    let centiseconds: f64 = time_caps[4].parse().unwrap_or(0.0);

    let current_time = hours * 3600.0 + minutes * 60.0 + seconds + centiseconds / 100.0;

    let percent = if total_duration > 0.0 {
        (current_time / total_duration * 100.0).min(100.0)
    } else {
        0.0
    };

    let current_frame = FRAME_RE
        .captures(line)
        .and_then(|c| c[1].parse().ok());

    let speed = SPEED_RE
        .captures(line)
        .map(|c| format!("{}x", &c[1]));

    let eta_seconds = SPEED_RE
        .captures(line)
        .and_then(|c| c[1].parse::<f64>().ok())
        .and_then(|spd| {
            if spd > 0.0 && total_duration > 0.0 {
                Some((total_duration - current_time) / spd)
            } else {
                None
            }
        });

    Some(ParsedProgress {
        percent: percent as f32,
        current_frame,
        speed,
        eta_seconds,
    })
}
