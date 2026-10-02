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

use core::sync::atomic::AtomicBool;

/// Set by the firmware's console module for as long as the interactive
/// console is active.
pub static CLI_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Like `esp_println::println!`, but muted while [`CLI_ACTIVE`] is set.
#[macro_export]
macro_rules! console_log {
    ($($arg:tt)*) => {
        if !$crate::logging::CLI_ACTIVE.load(::core::sync::atomic::Ordering::Relaxed) {
            ::esp_println::println!($($arg)*);
        }
    };
}
