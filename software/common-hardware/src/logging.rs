//! Debug logging that shares UART0 with each firmware's commissioning
//! console.
//!
//! Plain `esp_println::println!` output racing with the console's line
//! editor corrupts the prompt/cursor position on screen.
//! [`console_log!`](crate::console_log) is a drop-in replacement that
//! silently drops its output while [`CLI_ACTIVE`] is set, i.e. whenever a
//! technician has actually dropped into the console. It has no effect
//! otherwise. The console module itself sets `CLI_ACTIVE` for as long as
//! it's active — `gateway-hardware`'s `console.rs` and
//! `cancomponents-hardware`'s `cli.rs` both do this.

use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

/// Set by the firmware's console module for as long as the interactive
/// console is active.
pub static CLI_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Like `esp_println::println!`, but muted while [`CLI_ACTIVE`] is set.
/// Every line also goes to the [`set_sink`] (muted or not).
#[macro_export]
macro_rules! console_log {
    ($($arg:tt)*) => {{
        $crate::logging::to_sink(format_args!($($arg)*));
        if !$crate::logging::CLI_ACTIVE.load(::core::sync::atomic::Ordering::Relaxed) {
            ::esp_println::println!($($arg)*);
        }
    }};
}

/// Where else the log lines go (the gateway's web UI keeps the latest).
pub type Sink = fn(core::fmt::Arguments<'_>);

static SINK: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Sends every [`console_log!`](crate::console_log) line to `sink` too.
pub fn set_sink(sink: Sink) {
    SINK.store(sink as *mut (), Ordering::Release);
}

/// Hands a log line to the [`set_sink`], if there is one.
pub fn to_sink(line: core::fmt::Arguments<'_>) {
    let sink = SINK.load(Ordering::Acquire);
    if !sink.is_null() {
        // SAFETY: only `set_sink` stores here, and it stores a `Sink`.
        let sink: Sink = unsafe { core::mem::transmute::<*mut (), Sink>(sink) };
        sink(line);
    }
}
