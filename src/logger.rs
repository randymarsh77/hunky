use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl LogLevel {
    fn from_str(input: &str) -> Option<Self> {
        match input.to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        }
    }
}

#[derive(Clone, Debug)]
struct LoggerConfig {
    enabled: bool,
    level: LogLevel,
    file_path: String,
    filtered_events_enabled: bool,
}

static LOGGER_CONFIG: OnceLock<LoggerConfig> = OnceLock::new();
static LOG_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn read_config() -> LoggerConfig {
    let enabled = std::env::var("HUNKY_LOG")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false);

    let level = std::env::var("HUNKY_LOG_LEVEL")
        .ok()
        .as_deref()
        .and_then(LogLevel::from_str)
        .unwrap_or(LogLevel::Info);

    let file_path = std::env::var("HUNKY_LOG_FILE").unwrap_or_else(|_| "hunky.log".to_string());

    let filtered_events_enabled = std::env::var("HUNKY_LOG_FILTERED_EVENTS")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false);

    LoggerConfig {
        enabled,
        level,
        file_path,
        filtered_events_enabled,
    }
}

fn config() -> &'static LoggerConfig {
    LOGGER_CONFIG.get_or_init(read_config)
}

pub fn init() {
    let _ = config();
}

pub fn enabled(level: LogLevel) -> bool {
    let cfg = config();
    cfg.enabled && level <= cfg.level
}

pub fn filtered_events_enabled() -> bool {
    let cfg = config();
    cfg.enabled && cfg.filtered_events_enabled
}

pub fn log(level: LogLevel, msg: impl AsRef<str>) {
    if !enabled(level) {
        return;
    }

    let cfg = config();
    let write_lock = LOG_WRITE_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = write_lock.lock().unwrap_or_else(|e| e.into_inner());

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg.file_path)
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(file, "[{}] [{}] {}", ts, level.as_str(), msg.as_ref());
    }
}

#[allow(dead_code)]
pub fn error(msg: impl AsRef<str>) {
    log(LogLevel::Error, msg);
}

pub fn warn(msg: impl AsRef<str>) {
    log(LogLevel::Warn, msg);
}

#[allow(dead_code)]
pub fn info(msg: impl AsRef<str>) {
    log(LogLevel::Info, msg);
}

pub fn debug(msg: impl AsRef<str>) {
    log(LogLevel::Debug, msg);
}

pub fn trace(msg: impl AsRef<str>) {
    log(LogLevel::Trace, msg);
}
