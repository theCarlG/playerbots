// Log bridge to CMaNGOS `sLog`.
//
// The C++ side installs a sink via `playerbot_set_log_sink`; we store the
// pointer atomically and fan every log call through it. Before install (and
// in unit tests where no C++ is linked) the sink is null and calls are no-ops.

use std::ffi::CString;
use std::sync::atomic::{AtomicPtr, Ordering};

/// Matches the `PLAYERBOT_LOG_*` defines in `botffi.h`.
#[repr(u8)]
#[derive(Copy, Clone, Debug)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

/// `void (*)(uint8_t, const char*)` — the C sink type.
pub type LogSinkFn = unsafe extern "C" fn(level: u8, msg: *const std::os::raw::c_char);

// AtomicPtr<()> holding a transmuted LogSinkFn. Null = no sink installed.
static SINK: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Install (or clear, with `None`) the global log sink. Called from
/// `playerbot_set_log_sink`.
pub fn set_sink(sink: Option<LogSinkFn>) {
    let ptr = match sink {
        Some(f) => f as *mut (),
        None => std::ptr::null_mut(),
    };
    SINK.store(ptr, Ordering::Release);
}

/// Emit a log line at the given level. Drops the message on allocation
/// failure or if no sink is installed.
pub fn log(level: LogLevel, msg: &str) {
    let raw = SINK.load(Ordering::Acquire);
    if raw.is_null() {
        return;
    }
    // SAFETY: `raw` was stored via `set_sink` with a valid LogSinkFn (or null,
    // handled above). Transmuting back to the same fn type is sound.
    let sink: LogSinkFn = unsafe { std::mem::transmute::<*mut (), LogSinkFn>(raw) };
    // Replace interior NULs so CString::new can't fail on user input.
    let cleaned: String = msg.replace('\0', "?");
    if let Ok(cstr) = CString::new(cleaned) {
        unsafe { sink(level as u8, cstr.as_ptr()) };
    }
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { $crate::logging::log($crate::logging::LogLevel::Error, &format!($($arg)*)) };
}
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::logging::log($crate::logging::LogLevel::Warn,  &format!($($arg)*)) };
}
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { $crate::logging::log($crate::logging::LogLevel::Info,  &format!($($arg)*)) };
}
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => { $crate::logging::log($crate::logging::LogLevel::Debug, &format!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    use std::sync::Mutex;

    // Global capture buffer for the test sink. Tests run in one process and
    // the sink is global, so serialize via a mutex.
    static CAPTURE: Mutex<Vec<(u8, String)>> = Mutex::new(Vec::new());
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    unsafe extern "C" fn capture_sink(level: u8, msg: *const std::os::raw::c_char) {
        let s = unsafe { CStr::from_ptr(msg) }
            .to_string_lossy()
            .into_owned();
        CAPTURE.lock().unwrap().push((level, s));
    }

    fn reset() {
        CAPTURE.lock().unwrap().clear();
        set_sink(None);
    }

    #[test]
    fn no_sink_is_noop() {
        let _g = TEST_LOCK.lock().unwrap();
        reset();
        log(LogLevel::Error, "dropped");
        assert!(CAPTURE.lock().unwrap().is_empty());
    }

    #[test]
    fn sink_receives_level_and_message() {
        let _g = TEST_LOCK.lock().unwrap();
        reset();
        set_sink(Some(capture_sink));
        log(LogLevel::Warn, "hello");
        log(LogLevel::Info, "world");
        let got = CAPTURE.lock().unwrap().clone();
        assert_eq!(got, vec![(1u8, "hello".into()), (2u8, "world".into())]);
        reset();
    }

    #[test]
    fn macros_format_and_dispatch() {
        let _g = TEST_LOCK.lock().unwrap();
        reset();
        set_sink(Some(capture_sink));
        crate::log_error!("code={}", 42);
        crate::log_debug!("x={} y={}", 1, 2);
        let got = CAPTURE.lock().unwrap().clone();
        assert_eq!(got, vec![(0u8, "code=42".into()), (3u8, "x=1 y=2".into())]);
        reset();
    }

    #[test]
    fn interior_nul_is_sanitized() {
        let _g = TEST_LOCK.lock().unwrap();
        reset();
        set_sink(Some(capture_sink));
        log(LogLevel::Error, "a\0b");
        let got = CAPTURE.lock().unwrap().clone();
        assert_eq!(got, vec![(0u8, "a?b".into())]);
        reset();
    }

    #[test]
    fn clearing_sink_stops_delivery() {
        let _g = TEST_LOCK.lock().unwrap();
        reset();
        set_sink(Some(capture_sink));
        log(LogLevel::Info, "one");
        set_sink(None);
        log(LogLevel::Info, "two");
        let got = CAPTURE.lock().unwrap().clone();
        assert_eq!(got, vec![(2u8, "one".into())]);
    }
}
