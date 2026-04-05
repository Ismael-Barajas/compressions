use std::fs;
use std::path::PathBuf;

use tauri::{AppHandle, Manager};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::types::LogEntry;

const MAX_LOG_LINES: usize = 2000;

pub fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let log_dir = dir.join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| format!("Failed to create log dir: {}", e))?;
    Ok(log_dir)
}

pub fn log_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(log_dir(app)?.join("compressions.log"))
}

/// Initialize tracing with dual output: stderr + rolling log file.
/// Returns the WorkerGuard which must be held for the lifetime of the app.
pub fn init_tracing(log_dir: PathBuf) -> WorkerGuard {
    let file_appender = tracing_appender::rolling::daily(&log_dir, "compressions.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("compressions_lib=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true),
        )
        .with(fmt::layer().with_writer(std::io::stderr).with_target(true))
        .init();

    guard
}

/// Read log entries from the current log file, returning the most recent `max_lines`.
pub fn read_log_entries(
    app: &AppHandle,
    max_lines: Option<usize>,
) -> Result<Vec<LogEntry>, String> {
    let dir = log_dir(app)?;

    // Find all log files and sort them (most recent last)
    let mut log_files: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read log dir: {}", e))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("compressions.log"))
                .unwrap_or(false)
        })
        .collect();
    log_files.sort();

    let limit = max_lines.unwrap_or(MAX_LOG_LINES);
    let mut entries = Vec::new();

    for path in &log_files {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(entry) = parse_log_line(line) {
                entries.push(entry);
            }
        }
    }

    // Keep only the most recent entries
    if entries.len() > limit {
        entries.drain(..entries.len() - limit);
    }

    Ok(entries)
}

/// Parse a tracing fmt log line like:
/// `2026-04-04T12:00:00.000Z  INFO compressions_lib::commands::video: Compressing video input="/path"`
fn parse_log_line(line: &str) -> Option<LogEntry> {
    // Format: <timestamp> <LEVEL> <target>: <message>
    // The timestamp ends at the first double-space or after the timezone marker
    let rest = line.trim();
    if rest.is_empty() {
        return None;
    }

    // Find the level keyword
    let levels = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];
    let mut level_start = None;
    let mut level_str = "";

    for l in &levels {
        if let Some(pos) = rest.find(l) {
            // Verify it's surrounded by whitespace
            let before_ok = pos == 0
                || rest
                    .as_bytes()
                    .get(pos - 1)
                    .map(|b| b.is_ascii_whitespace())
                    .unwrap_or(false);
            let after_pos = pos + l.len();
            let after_ok = after_pos >= rest.len()
                || rest
                    .as_bytes()
                    .get(after_pos)
                    .map(|b| b.is_ascii_whitespace() || *b == b':')
                    .unwrap_or(true);
            if before_ok && after_ok {
                level_start = Some(pos);
                level_str = l;
                break;
            }
        }
    }

    let level_pos = level_start?;
    let timestamp = rest[..level_pos].trim().to_string();
    let after_level = &rest[level_pos + level_str.len()..].trim_start();

    // Split target: message
    let (target, message) = if let Some(colon_pos) = after_level.find(": ") {
        let t = after_level[..colon_pos].trim();
        let m = after_level[colon_pos + 2..].trim();
        (Some(t.to_string()), m.to_string())
    } else {
        (None, after_level.to_string())
    };

    Some(LogEntry {
        timestamp,
        level: level_str.to_string(),
        message,
        target,
    })
}

/// Clear all log files.
pub fn clear_logs(app: &AppHandle) -> Result<(), String> {
    let dir = log_dir(app)?;
    let entries = fs::read_dir(&dir).map_err(|e| format!("Failed to read log dir: {}", e))?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("compressions.log"))
            .unwrap_or(false)
        {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_info_line() {
        let line = "2026-04-04T12:00:00.000Z  INFO compressions_lib::commands::video: Starting compression input=\"/path\"";
        let entry = parse_log_line(line).unwrap();
        assert_eq!(entry.level, "INFO");
        assert_eq!(
            entry.target.as_deref(),
            Some("compressions_lib::commands::video")
        );
        assert!(entry.message.contains("Starting compression"));
        assert!(entry.timestamp.contains("2026"));
    }

    #[test]
    fn parse_warn_line() {
        let line = "2026-04-04T12:00:00.000Z  WARN compressions_lib::pdf: PDF failed";
        let entry = parse_log_line(line).unwrap();
        assert_eq!(entry.level, "WARN");
    }

    #[test]
    fn parse_error_line() {
        let line = "2026-04-04T12:00:00.000Z ERROR compressions_lib::image: Encode error";
        let entry = parse_log_line(line).unwrap();
        assert_eq!(entry.level, "ERROR");
    }

    #[test]
    fn parse_debug_line() {
        let line = "2026-04-04T12:00:00.000Z DEBUG compressions_lib: debug msg";
        let entry = parse_log_line(line).unwrap();
        assert_eq!(entry.level, "DEBUG");
    }

    #[test]
    fn empty_line_returns_none() {
        assert!(parse_log_line("").is_none());
        assert!(parse_log_line("   ").is_none());
    }

    #[test]
    fn garbage_returns_none() {
        assert!(parse_log_line("just some random text").is_none());
    }
}
