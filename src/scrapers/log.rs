//! Scraper error logging

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use chrono::Local;

static LOG_FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Initialize the log file path
pub fn init_log() -> Option<PathBuf> {
    let config_dir = dirs::config_dir()?.join("littlejohn");
    std::fs::create_dir_all(&config_dir).ok()?;
    let log_path = config_dir.join("scraper.log");

    // Truncate log file on startup
    if let Ok(mut file) = File::create(&log_path) {
        let _ = writeln!(file, "=== Scraper Log Started {} ===", Local::now().format("%Y-%m-%d %H:%M:%S"));
    }

    if let Ok(mut guard) = LOG_FILE.lock() {
        *guard = Some(log_path.clone());
    }

    Some(log_path)
}

/// Log a scraper error
pub fn log_error(source: &str, message: &str) {
    let timestamp = Local::now().format("%H:%M:%S");
    let log_line = format!("[{}] [{}] ERROR: {}", timestamp, source, message);

    // Also print to stderr for debugging
    eprintln!("{}", log_line);

    if let Ok(guard) = LOG_FILE.lock() {
        if let Some(ref path) = *guard {
            if let Ok(mut file) = OpenOptions::new().append(true).open(path) {
                let _ = writeln!(file, "{}", log_line);
            }
        }
    }
}

/// Log a scraper info message
pub fn log_info(source: &str, message: &str) {
    let timestamp = Local::now().format("%H:%M:%S");
    let log_line = format!("[{}] [{}] INFO: {}", timestamp, source, message);

    if let Ok(guard) = LOG_FILE.lock() {
        if let Some(ref path) = *guard {
            if let Ok(mut file) = OpenOptions::new().append(true).open(path) {
                let _ = writeln!(file, "{}", log_line);
            }
        }
    }
}

/// Get the log file path
pub fn get_log_path() -> Option<PathBuf> {
    LOG_FILE.lock().ok().and_then(|g| g.clone())
}

/// Read recent log entries (last N lines)
pub fn read_recent_logs(n: usize) -> Vec<String> {
    let path = match get_log_path() {
        Some(p) => p,
        None => return vec!["Log not initialized".to_string()],
    };

    match std::fs::read_to_string(&path) {
        Ok(content) => {
            content.lines()
                .rev()
                .take(n)
                .map(String::from)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        }
        Err(_) => vec!["Could not read log file".to_string()],
    }
}
