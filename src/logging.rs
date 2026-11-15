//! A tiny logger: no dependencies, timestamps in epoch seconds so logs can be
//! correlated with other tools, level filtering via `--verbose`.

use std::{
    fmt,
    sync::atomic::{AtomicU8, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
}

static LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);

pub fn set_level(level: Level) {
    LEVEL.store(level as u8, Ordering::Relaxed);
}

pub fn enabled(level: Level) -> bool {
    (level as u8) <= LEVEL.load(Ordering::Relaxed)
}

pub fn emit(level: Level, args: fmt::Arguments<'_>) {
    if !enabled(level) {
        return;
    }

    let tag = match level {
        Level::Error => "error",
        Level::Warn => "warn ",
        Level::Info => "info ",
        Level::Debug => "debug",
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    eprintln!("[{timestamp} {tag}] {args}");
}

macro_rules! error {
    ($($arg:tt)*) => {
        $crate::logging::emit($crate::logging::Level::Error, format_args!($($arg)*))
    };
}

macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::logging::emit($crate::logging::Level::Warn, format_args!($($arg)*))
    };
}

macro_rules! info {
    ($($arg:tt)*) => {
        $crate::logging::emit($crate::logging::Level::Info, format_args!($($arg)*))
    };
}

macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::logging::emit($crate::logging::Level::Debug, format_args!($($arg)*))
    };
}
