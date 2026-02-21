use super::*;
use std::sync::{Mutex, OnceLock};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
    let lock = ENV_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    f()
}

fn set_var(key: &str, value: &str) {
    std::env::set_var(key, value)
}

fn remove_var(key: &str) {
    std::env::remove_var(key)
}

#[test]
fn log_level_from_str_and_as_str_cover_known_values() {
    assert_eq!(LogLevel::from_str("error"), Some(LogLevel::Error));
    assert_eq!(LogLevel::from_str("Warning"), Some(LogLevel::Warn));
    assert_eq!(LogLevel::from_str("INFO"), Some(LogLevel::Info));
    assert_eq!(LogLevel::from_str("debug"), Some(LogLevel::Debug));
    assert_eq!(LogLevel::from_str("TrAcE"), Some(LogLevel::Trace));
    assert_eq!(LogLevel::from_str("unknown"), None);

    assert_eq!(LogLevel::Error.as_str(), "ERROR");
    assert_eq!(LogLevel::Warn.as_str(), "WARN");
    assert_eq!(LogLevel::Info.as_str(), "INFO");
    assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
    assert_eq!(LogLevel::Trace.as_str(), "TRACE");
}

#[test]
fn read_config_uses_defaults_when_env_is_unset() {
    with_env_lock(|| {
        remove_var("HUNKY_LOG");
        remove_var("HUNKY_LOG_LEVEL");
        remove_var("HUNKY_LOG_FILE");
        remove_var("HUNKY_LOG_FILTERED_EVENTS");

        let cfg = read_config();
        assert!(!cfg.enabled);
        assert_eq!(cfg.level, LogLevel::Info);
        assert_eq!(cfg.file_path, "hunky.log");
        assert!(!cfg.filtered_events_enabled);
    });
}

#[test]
fn read_config_parses_enabled_flags_and_level() {
    with_env_lock(|| {
        set_var("HUNKY_LOG", "yes");
        set_var("HUNKY_LOG_LEVEL", "trace");
        set_var("HUNKY_LOG_FILE", "/tmp/hunky-test.log");
        set_var("HUNKY_LOG_FILTERED_EVENTS", "on");

        let cfg = read_config();
        assert!(cfg.enabled);
        assert_eq!(cfg.level, LogLevel::Trace);
        assert_eq!(cfg.file_path, "/tmp/hunky-test.log");
        assert!(cfg.filtered_events_enabled);
    });
}

#[test]
fn read_config_falls_back_to_info_for_invalid_level() {
    with_env_lock(|| {
        set_var("HUNKY_LOG", "1");
        set_var("HUNKY_LOG_LEVEL", "verbose");
        remove_var("HUNKY_LOG_FILE");
        remove_var("HUNKY_LOG_FILTERED_EVENTS");

        let cfg = read_config();
        assert!(cfg.enabled);
        assert_eq!(cfg.level, LogLevel::Info);
    });
}
