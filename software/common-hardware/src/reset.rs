//! Restart after a panic instead of halting.
//!
//! `esp-backtrace` prints the panic message and backtrace (CPU exceptions
//! end up there too, esp-hal's exception handler panics) and then halts the
//! core forever, so an unattended device stays dead until it's
//! power-cycled. With its `custom-halt` feature it calls an
//! `extern "Rust" fn custom_halt() -> !` instead; each firmware defines
//! that symbol in its binary crate and forwards to [`restart_after_panic`].
//! (The symbol lives in the binary, not here: a `#[no_mangle]` item that
//! only exists in a dependency's rlib isn't guaranteed to be picked up by
//! the linker.)
//!
//! The restart shows up as `CoreSw` in the next boot's reset reason
//! (see [`esp_hal::system::reset_reason`]), which each firmware logs right
//! at boot, so a panic loop is visible on the console.

/// Time the panic output gets to leave the UART TX FIFO before the reset.
const FLUSH_DELAY_US: u32 = 1_000_000;

/// Waits for the panic output to be sent, then resets the chip.
///
/// Runs in the panic path, possibly with interrupts disabled or before
/// `esp_hal::init()`, so it busy-waits with the ROM's cycle-counter delay
/// rather than anything timer- or interrupt-based.
pub fn restart_after_panic() -> ! {
    esp_hal::rom::ets_delay_us(FLUSH_DELAY_US);
    esp_hal::system::software_reset()
}
