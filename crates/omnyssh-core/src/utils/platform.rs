use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Rolling log files older than this many days are pruned at startup.
pub const LOG_RETENTION_DAYS: u64 = 7;

/// Prefix shared by every rolling log file (`omnyssh.log`, `omnyssh.log.YYYY-MM-DD`).
const LOG_FILE_PREFIX: &str = "omnyssh.log";

/// Returns the path to the user's SSH config file.
///
/// - Linux / macOS: `~/.ssh/config`
/// - Windows:       `%USERPROFILE%\.ssh\config`
///
/// Uses the `dirs` crate so we never hardcode `~`.
pub fn ssh_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ssh").join("config"))
}

/// Returns the application config directory.
///
/// - Linux:   `~/.config/omnyssh/`
/// - macOS:   `~/Library/Application Support/omnyssh/`
/// - Windows: `%APPDATA%\omnyssh\`
pub fn app_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("omnyssh"))
}

/// Returns the path to the main application config file.
pub fn app_config_path() -> Option<PathBuf> {
    app_config_dir().map(|d| d.join("config.toml"))
}

/// Returns the path to the hosts config file.
pub fn hosts_config_path() -> Option<PathBuf> {
    app_config_dir().map(|d| d.join("hosts.toml"))
}

/// Returns the path to the snippets config file.
pub fn snippets_config_path() -> Option<PathBuf> {
    app_config_dir().map(|d| d.join("snippets.toml"))
}

/// Removes rolling log files in `log_dir` older than `max_age_days`.
///
/// Best-effort and fault-tolerant: a missing directory, an unreadable entry,
/// or a failed delete is skipped rather than propagated, so log cleanup never
/// blocks application startup. File age is taken from the filesystem
/// modification time, which keeps the logic identical on every OS. Only files
/// whose name starts with `omnyssh.log` are considered, so config and other
/// data files in the same directory are never touched.
///
/// Returns the number of files removed.
pub fn cleanup_old_logs(log_dir: &Path, max_age_days: u64) -> usize {
    let entries = match std::fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };

    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);
    let now = SystemTime::now();
    let mut removed = 0;

    for entry in entries.flatten() {
        let path = entry.path();

        let is_log_file = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with(LOG_FILE_PREFIX))
            .unwrap_or(false);
        if !is_log_file {
            continue;
        }

        let modified = match entry.metadata().and_then(|meta| meta.modified()) {
            Ok(modified) => modified,
            Err(_) => continue,
        };

        // `duration_since` errors on files dated in the future (clock skew);
        // treat those as recent and keep them.
        if let Ok(age) = now.duration_since(modified) {
            if age > max_age && std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }

    removed
}
