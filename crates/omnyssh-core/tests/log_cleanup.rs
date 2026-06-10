//! Tests for startup log pruning.
//!
//! Verifies that `cleanup_old_logs` removes only stale rolling log files,
//! leaves recent logs and unrelated files untouched, and never panics on a
//! missing directory.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use omnyssh_core::utils::platform::cleanup_old_logs;

/// Creates a unique, empty scratch directory under the system temp dir.
fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("omnyssh-test-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Writes a file and backdates its modification time by `age_days`.
fn write_aged(dir: &Path, name: &str, age_days: u64) {
    let path = dir.join(name);
    let file = File::create(&path).expect("create file");
    let mtime = SystemTime::now() - Duration::from_secs(age_days * 24 * 60 * 60);
    file.set_modified(mtime).expect("set mtime");
}

#[test]
fn removes_logs_older_than_retention() {
    let dir = scratch_dir("old");
    write_aged(&dir, "omnyssh.log.2026-01-01", 30);
    write_aged(&dir, "omnyssh.log.2026-01-02", 14);

    let removed = cleanup_old_logs(&dir, 7);

    assert_eq!(removed, 2);
    assert!(!dir.join("omnyssh.log.2026-01-01").exists());
    assert!(!dir.join("omnyssh.log.2026-01-02").exists());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn keeps_recent_logs() {
    let dir = scratch_dir("recent");
    write_aged(&dir, "omnyssh.log.2026-05-29", 0);
    write_aged(&dir, "omnyssh.log.2026-05-25", 3);

    let removed = cleanup_old_logs(&dir, 7);

    assert_eq!(removed, 0);
    assert!(dir.join("omnyssh.log.2026-05-29").exists());
    assert!(dir.join("omnyssh.log.2026-05-25").exists());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn never_touches_non_log_files() {
    let dir = scratch_dir("config");
    write_aged(&dir, "config.toml", 90);
    write_aged(&dir, "hosts.toml", 90);
    write_aged(&dir, "snippets.toml", 90);
    write_aged(&dir, "omnyssh.log.2026-01-01", 90);

    let removed = cleanup_old_logs(&dir, 7);

    assert_eq!(removed, 1);
    assert!(dir.join("config.toml").exists());
    assert!(dir.join("hosts.toml").exists());
    assert!(dir.join("snippets.toml").exists());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_directory_is_a_no_op() {
    let dir = std::env::temp_dir().join("omnyssh-test-missing-does-not-exist");
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(cleanup_old_logs(&dir, 7), 0);
}
