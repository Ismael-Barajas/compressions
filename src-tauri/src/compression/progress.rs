use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedProgress {
    pub percent: f32,
    pub current_frame: Option<u64>,
    pub speed: Option<String>,
    pub eta_seconds: Option<f64>,
}

/// Incremental parser for FFmpeg `-progress pipe:2` output.
///
/// FFmpeg emits progress as a block of `key=value` lines terminated by a
/// `progress=continue|end` line. Feed each stderr line to [`feed_line`]; a
/// [`ParsedProgress`] is returned once per completed block. Because the whole
/// block is accumulated, `speed` (and therefore ETA) is available alongside
/// `out_time`, which a single-line regex approach can never see.
#[derive(Debug, Default)]
pub struct ProgressParser {
    total_duration: f64,
    frame: Option<u64>,
    out_time_secs: Option<f64>,
    speed: Option<f64>,
}

impl ProgressParser {
    pub fn new(total_duration: f64) -> Self {
        Self {
            total_duration,
            ..Default::default()
        }
    }

    /// Feed one line of FFmpeg stderr. Returns a progress snapshot when a block ends.
    pub fn feed_line(&mut self, line: &str) -> Option<ParsedProgress> {
        let line = line.trim();
        let (key, value) = line.split_once('=')?;
        let value = value.trim();
        match key.trim() {
            "frame" => self.frame = value.parse().ok(),
            // `out_time_us` (µs) is the most precise; `out_time` (HH:MM:SS.micro) is
            // the fallback for older builds. Ignore `out_time_ms`, which is actually
            // microseconds in modern FFmpeg and would be misparsed.
            "out_time_us" => {
                if let Ok(us) = value.parse::<i64>() {
                    if us >= 0 {
                        self.out_time_secs = Some(us as f64 / 1_000_000.0);
                    }
                }
            }
            "out_time" => {
                if self.out_time_secs.is_none() {
                    self.out_time_secs = parse_hms(value);
                }
            }
            "speed" => {
                self.speed = value.strip_suffix('x').and_then(|s| s.trim().parse().ok());
            }
            "progress" => {
                let snapshot = self.snapshot();
                self.reset_block();
                return snapshot;
            }
            _ => {}
        }
        None
    }

    fn reset_block(&mut self) {
        self.frame = None;
        self.out_time_secs = None;
        self.speed = None;
    }

    fn snapshot(&self) -> Option<ParsedProgress> {
        let current_time = self.out_time_secs?;
        let percent = if self.total_duration > 0.0 {
            (current_time / self.total_duration * 100.0).min(100.0)
        } else {
            0.0
        };
        let eta_seconds = self.speed.and_then(|spd| {
            if spd > 0.0 && self.total_duration > 0.0 {
                Some(((self.total_duration - current_time) / spd).max(0.0))
            } else {
                None
            }
        });
        Some(ParsedProgress {
            percent: percent as f32,
            current_frame: self.frame,
            speed: self.speed.map(|s| format!("{}x", s)),
            eta_seconds,
        })
    }
}

fn parse_hms(value: &str) -> Option<f64> {
    // HH:MM:SS.fraction
    let mut parts = value.split(':');
    let h: f64 = parts.next()?.trim().parse().ok()?;
    let m: f64 = parts.next()?.trim().parse().ok()?;
    let s: f64 = parts.next()?.trim().parse().ok()?;
    if h < 0.0 || m < 0.0 || s < 0.0 {
        return None;
    }
    Some(h * 3600.0 + m * 60.0 + s)
}

/// Rate-limits progress events sent over IPC. FFmpeg can report several times per
/// second; each event costs an IPC round trip plus a React render, so only forward
/// updates that moved the bar visibly or that are old enough to keep the ETA fresh.
#[derive(Debug)]
pub struct ProgressThrottle {
    min_interval: Duration,
    min_delta_percent: f32,
    last_sent_at: Option<Instant>,
    last_sent_percent: f32,
}

impl ProgressThrottle {
    pub const DEFAULT_INTERVAL: Duration = Duration::from_millis(250);
    pub const DEFAULT_DELTA_PERCENT: f32 = 0.5;

    pub fn new(min_interval: Duration, min_delta_percent: f32) -> Self {
        Self {
            min_interval,
            min_delta_percent,
            last_sent_at: None,
            last_sent_percent: -1.0,
        }
    }

    /// Returns true if a progress event at `percent` should be forwarded now.
    /// Records the send when it returns true.
    pub fn should_send(&mut self, percent: f32) -> bool {
        self.should_send_at(percent, Instant::now())
    }

    pub fn should_send_at(&mut self, percent: f32, now: Instant) -> bool {
        let first = self.last_sent_at.is_none();
        let moved_enough = (percent - self.last_sent_percent).abs() >= self.min_delta_percent;
        let old_enough = self
            .last_sent_at
            .map(|t| now.duration_since(t) >= self.min_interval)
            .unwrap_or(true);
        let reached_end = percent >= 100.0 && self.last_sent_percent < 100.0;
        if first || reached_end || (moved_enough && old_enough) {
            self.last_sent_at = Some(now);
            self.last_sent_percent = percent;
            true
        } else {
            false
        }
    }
}

impl Default for ProgressThrottle {
    fn default() -> Self {
        Self::new(Self::DEFAULT_INTERVAL, Self::DEFAULT_DELTA_PERCENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_block(parser: &mut ProgressParser, lines: &[&str]) -> Option<ParsedProgress> {
        let mut out = None;
        for line in lines {
            if let Some(p) = parser.feed_line(line) {
                out = Some(p);
            }
        }
        out
    }

    #[test]
    fn parses_full_progress_block() {
        let mut parser = ProgressParser::new(10.0);
        let p = feed_block(
            &mut parser,
            &[
                "frame=120",
                "fps=30.00",
                "out_time_us=4000000",
                "out_time_ms=4000000",
                "out_time=00:00:04.000000",
                "speed=1.5x",
                "progress=continue",
            ],
        )
        .unwrap();
        assert!((p.percent - 40.0).abs() < 0.1);
        assert_eq!(p.current_frame, Some(120));
        assert_eq!(p.speed.as_deref(), Some("1.5x"));
        assert!((p.eta_seconds.unwrap() - 4.0).abs() < 0.1);
    }

    #[test]
    fn emits_only_at_block_end() {
        let mut parser = ProgressParser::new(10.0);
        assert!(parser.feed_line("frame=1").is_none());
        assert!(parser.feed_line("out_time_us=1000000").is_none());
        assert!(parser.feed_line("progress=continue").is_some());
        // Next block starts fresh
        assert!(parser.feed_line("progress=continue").is_none());
    }

    #[test]
    fn falls_back_to_out_time_string() {
        let mut parser = ProgressParser::new(120.0);
        let p = feed_block(&mut parser, &["out_time=00:01:00.00", "progress=continue"]).unwrap();
        assert!((p.percent - 50.0).abs() < 0.1);
        assert_eq!(p.speed, None);
        assert_eq!(p.eta_seconds, None);
    }

    #[test]
    fn capped_at_100_percent() {
        let mut parser = ProgressParser::new(60.0);
        let p = feed_block(
            &mut parser,
            &["out_time_us=120000000", "speed=1x", "progress=end"],
        )
        .unwrap();
        assert!((p.percent - 100.0).abs() < 0.01);
        assert_eq!(p.eta_seconds, Some(0.0));
    }

    #[test]
    fn zero_duration_reports_zero_percent() {
        let mut parser = ProgressParser::new(0.0);
        let p = feed_block(&mut parser, &["out_time_us=5000000", "progress=continue"]).unwrap();
        assert!((p.percent - 0.0).abs() < 0.01);
    }

    #[test]
    fn negative_out_time_is_ignored() {
        // FFmpeg reports negative out_time before the first frame is muxed.
        let mut parser = ProgressParser::new(10.0);
        assert!(feed_block(
            &mut parser,
            &["out_time_us=-9223372036854775808", "progress=continue"]
        )
        .is_none());
    }

    #[test]
    fn garbage_returns_none() {
        let mut parser = ProgressParser::new(10.0);
        assert!(parser.feed_line("garbage data here").is_none());
        assert!(parser.feed_line("").is_none());
    }

    #[test]
    fn throttle_sends_first_and_end() {
        let mut t = ProgressThrottle::default();
        let now = Instant::now();
        assert!(t.should_send_at(0.0, now));
        assert!(!t.should_send_at(0.1, now));
        assert!(t.should_send_at(100.0, now));
    }

    #[test]
    fn throttle_requires_both_delta_and_interval() {
        let mut t = ProgressThrottle::new(Duration::from_millis(250), 0.5);
        let now = Instant::now();
        assert!(t.should_send_at(0.0, now));
        // Moved enough, but too soon
        assert!(!t.should_send_at(5.0, now + Duration::from_millis(100)));
        // Old enough, but did not move
        assert!(!t.should_send_at(0.1, now + Duration::from_millis(300)));
        // Both
        assert!(t.should_send_at(5.0, now + Duration::from_millis(300)));
    }
}
