use regex::Regex;
use std::sync::LazyLock;

pub struct ParsedProgress {
    pub percent: f32,
    pub current_frame: Option<u64>,
    pub speed: Option<String>,
    pub eta_seconds: Option<f64>,
}

static FRAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"frame=\s*(\d+)").unwrap());
static TIME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"time=(\d+):(\d+):(\d+)\.(\d+)").unwrap());
static SPEED_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"speed=\s*([\d.]+)x").unwrap());

pub fn parse_progress_line(line: &str, total_duration: f64) -> Option<ParsedProgress> {
    // We need at least the time field to compute progress
    let time_caps = TIME_RE.captures(line)?;

    let hours: f64 = time_caps[1].parse().unwrap_or(0.0);
    let minutes: f64 = time_caps[2].parse().unwrap_or(0.0);
    let seconds: f64 = time_caps[3].parse().unwrap_or(0.0);
    let fractional: f64 = format!("0.{}", &time_caps[4]).parse().unwrap_or(0.0);

    let current_time = hours * 3600.0 + minutes * 60.0 + seconds + fractional;

    let percent = if total_duration > 0.0 {
        (current_time / total_duration * 100.0).min(100.0)
    } else {
        0.0
    };

    let current_frame = FRAME_RE.captures(line).and_then(|c| c[1].parse().ok());

    let speed = SPEED_RE.captures(line).map(|c| format!("{}x", &c[1]));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typical_progress_line() {
        let line = "frame=  120 fps=30 time=00:00:04.00 speed=1.5x";
        let p = parse_progress_line(line, 10.0).unwrap();
        assert!((p.percent - 40.0).abs() < 0.1);
        assert_eq!(p.current_frame, Some(120));
        assert_eq!(p.speed.as_deref(), Some("1.5x"));
        assert!(p.eta_seconds.unwrap() > 0.0);
    }

    #[test]
    fn time_only_line() {
        let line = "time=00:01:00.00";
        let p = parse_progress_line(line, 120.0).unwrap();
        assert!((p.percent - 50.0).abs() < 0.1);
        assert_eq!(p.current_frame, None);
        assert_eq!(p.speed, None);
        assert_eq!(p.eta_seconds, None);
    }

    #[test]
    fn capped_at_100_percent() {
        let line = "time=00:02:00.00 speed=1.0x";
        let p = parse_progress_line(line, 60.0).unwrap();
        assert!((p.percent - 100.0).abs() < 0.01);
    }

    #[test]
    fn zero_duration() {
        let line = "time=00:00:05.00";
        let p = parse_progress_line(line, 0.0).unwrap();
        assert!((p.percent - 0.0).abs() < 0.01);
    }

    #[test]
    fn garbled_input_returns_none() {
        assert!(parse_progress_line("garbage data here", 10.0).is_none());
        assert!(parse_progress_line("", 10.0).is_none());
    }

    #[test]
    fn eta_accuracy() {
        // 5s elapsed at 2x speed, 10s total duration => (10-5)/2 = 2.5s remaining
        let line = "time=00:00:05.00 speed=2.0x";
        let p = parse_progress_line(line, 10.0).unwrap();
        assert!((p.eta_seconds.unwrap() - 2.5).abs() < 0.1);
    }
}
