//! Futures on the heap, built without a detour over the caller's stack
//! frame.
//!
//! Everything runs on the one main stack (see [`crate::stack`]), and an
//! async function's poll keeps room in its frame for every value it builds.
//! `Box::pin(f())` builds `f`'s future there first and only then moves it to
//! the heap, so a 10 KB future costs its caller 10 KB of stack for as long as
//! the caller runs, on top of what polling the future needs.

use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;

/// `Box::pin(make())`, with the future built in this function's own frame,
/// which is gone before the future is polled. Returned as `dyn` so that
/// polling it can't be inlined into the caller's poll either: its frame
/// is on the stack only while it runs.
#[inline(never)]
pub fn boxed<'a, T, F: Future<Output = T> + 'a>(
    make: impl FnOnce() -> F,
) -> Pin<Box<dyn Future<Output = T> + 'a>> {
    Box::pin(make())
}
