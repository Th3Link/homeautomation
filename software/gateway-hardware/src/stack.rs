//! How deep the main stack has gone. Everything runs on it: the executor,
//! every task's poll, MbedTLS's handshake, flash access. An overflow only
//! shows as a panic (esp-hal's stack guard), so [`paint`] fills the unused
//! part with a pattern at boot, and [`untouched`] tells how much of it no
//! code has written since: the margin that is left.

const PATTERN: u32 = 0xA5A5_5A5A;
/// esp-hal's stack guard sits 60 bytes above the bottom; stay clear of it.
const GUARD: usize = 64;
/// What [`paint`] leaves below its own frame (interrupts, its callees).
const SPARE: usize = 4096;

extern "C" {
    static _stack_end_cpu0: u32;
    static _stack_start_cpu0: u32;
}

fn bottom() -> usize {
    (&raw const _stack_end_cpu0) as usize + GUARD
}

/// The main stack's size in bytes.
pub fn size() -> usize {
    (&raw const _stack_start_cpu0) as usize - (&raw const _stack_end_cpu0) as usize
}

/// Fills the stack below the current frame with the pattern. Call once,
/// early in `main`.
pub fn paint() {
    let marker = 0u8;
    let here = core::ptr::addr_of!(marker) as usize;
    let top = here.min((&raw const _stack_start_cpu0) as usize) - SPARE;
    let mut word = bottom();
    while word < top {
        // SAFETY: between the guard and the current frame, so unused, and
        // word-aligned (the stack's bottom is).
        unsafe { (word as *mut u32).write_volatile(PATTERN) };
        word += 4;
    }
}

/// Bytes at the bottom of the stack no code has written to since [`paint`].
pub fn untouched() -> usize {
    let mut word = bottom();
    // SAFETY: inside the stack, word-aligned; reading is harmless.
    while unsafe { (word as *const u32).read_volatile() } == PATTERN {
        word += 4;
    }
    word - bottom()
}
