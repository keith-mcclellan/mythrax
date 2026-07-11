use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct AuditResults {
    pub search_history_ok: bool,
    pub search_history_error: Option<String>,
    pub daemon_ok: bool,
    pub daemon_error: Option<String>,
}

pub async fn run_workspace_audit(workspace_path: &Path) -> AuditResults {
    let (search_history_ok, search_history_error) = check_search_history(workspace_path);
    let (daemon_ok, daemon_error) = verify_daemon_health().await;

    AuditResults {
        search_history_ok,
        search_history_error,
        daemon_ok,
        daemon_error,
    }
}

fn check_search_history(workspace_path: &Path) -> (bool, Option<String>) {
    let home = std::env::var("HOME").unwrap_or_default();

    let paths = [
        workspace_path.join("mythrax_search_history.log"),
        std::env::current_dir().unwrap_or_default().join("mythrax_search_history.log"),
        Path::new(&home).join(".mythrax").join("mythrax_search_history.log"),
    ];

    let mut log_path = None;
    for p in &paths {
        if p.exists() {
            log_path = Some(p);
            break;
        }
    }

    let log_path = match log_path {
        Some(p) => p,
        None => return (false, Some("mythrax_search_history.log not found".to_string())),
    };

    let content = match std::fs::read_to_string(log_path) {
        Ok(c) => c,
        Err(e) => return (false, Some(format!("Failed to read search history log: {}", e))),
    };

    let last_line = match content.lines().next_back() {
        Some(line) => line.trim(),
        None => return (false, Some("mythrax_search_history.log is empty".to_string())),
    };

    if last_line.is_empty() {
        return (false, Some("mythrax_search_history.log has no non-empty lines".to_string()));
    }

    let timestamp = match parse_log_timestamp(last_line) {
        Some(t) => t,
        None => return (false, Some(format!("Failed to parse timestamp from log line: '{}'", last_line))),
    };

    let now = SystemTime::now();
    let difference = match now.duration_since(timestamp) {
        Ok(d) => d,
        Err(_) => {
            match timestamp.duration_since(now) {
                Ok(d) => {
                    if d.as_secs() < 60 {
                        std::time::Duration::from_secs(0)
                    } else {
                        return (false, Some(format!("Timestamp is in the future by {} seconds", d.as_secs())));
                    }
                }
                Err(_) => std::time::Duration::from_secs(0),
            }
        }
    };

    if difference.as_secs() <= 600 {
        (true, None)
    } else {
        (false, Some(format!("Latest search query was {} seconds ago (must be within 10 minutes)", difference.as_secs())))
    }
}

fn parse_log_timestamp(line: &str) -> Option<SystemTime> {
    if line.starts_with('{')
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(line)
            && let Some(ts_str) = val.get("timestamp").and_then(|v| v.as_str()) {
                return parse_timestamp_str(ts_str);
            }
    parse_timestamp_str(line)
}

fn parse_timestamp_str(s: &str) -> Option<SystemTime> {
    use chrono::{Datelike, Timelike};

    // 1. Scan for UNIX timestamp (10 consecutive digits bounded by word boundaries)
    let mut start = 0;
    while let Some(idx) = s[start..].find(|c: char| c.is_ascii_digit()) {
        let absolute_idx = start + idx;
        let digits_len = s[absolute_idx..].chars().take_while(|c| c.is_ascii_digit()).count();
        if digits_len == 10 {
            let prev_char = s[..absolute_idx].chars().next_back();
            let next_char = s[absolute_idx + 10..].chars().next();
            let prev_boundary = prev_char.map_or(true, |c| !c.is_alphanumeric());
            let next_boundary = next_char.map_or(true, |c| !c.is_alphanumeric());
            if prev_boundary && next_boundary {
                if let Ok(secs) = s[absolute_idx..absolute_idx + 10].parse::<u64>() {
                    return Some(UNIX_EPOCH + std::time::Duration::from_secs(secs));
                }
            }
        }
        start = absolute_idx + digits_len;
    }

    // 2. Scan for flexible ISO-8601 date pattern
    let mut start = 0;
    while let Some(dash_idx) = s[start..].find(|c| c == '-' || c == '/') {
        let absolute_dash = start + dash_idx;
        if absolute_dash >= 4 {
            let start_idx = absolute_dash - 4;
            if s[start_idx..absolute_dash].chars().all(|c| c.is_ascii_digit()) {
                let mut parse_slice = &s[start_idx..];
                if let Ok(dt) = crate::parser::parse_flexible_date(&mut parse_slice) {
                    let days_since_epoch = days_from_civil(dt.date().year(), dt.date().month(), dt.date().day()) - days_from_civil(1970, 1, 1);
                    let seconds = days_since_epoch * 86400 
                        + (dt.time().hour() as i64) * 3600 
                        + (dt.time().minute() as i64) * 60 
                        + (dt.time().second() as i64);
                    if seconds >= 0 {
                        return Some(UNIX_EPOCH + std::time::Duration::from_secs(seconds as u64));
                    }
                }
            }
        }
        start = absolute_dash + 1;
    }

    None
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = y - (m <= 2) as i32;
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = (y - era * 400) as u32;
    let m_signed = m as i32;
    let doy = (153 * (m_signed + if m_signed > 2 { -3 } else { 9 }) + 2) / 5 + (d as i32) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + (doy as u32);
    (era as i64) * 146097 + (doe as i64) - 719468
}

async fn verify_daemon_health() -> (bool, Option<String>) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build();
    let client = match client {
        Ok(c) => c,
        Err(e) => return (false, Some(format!("Failed to build HTTP client: {}", e))),
    };

    match client.get("http://127.0.0.1:8090/v1/config/llm").send().await {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() || status == reqwest::StatusCode::UNAUTHORIZED {
                (true, None)
            } else {
                (false, Some(format!("Daemon returned unexpected status code: {}", status)))
            }
        }
        Err(e) => {
            (false, Some(format!("Failed to connect to Daemon: {}", e)))
        }
    }
}
