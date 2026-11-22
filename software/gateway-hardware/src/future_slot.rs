//! Storage for the single active HTTPS request or NTS key exchange.
//! The caller holds the TLS session lock until the future completes or is cancelled.

use core::future::Future;
use core::mem::{align_of, size_of, MaybeUninit};
use core::pin::Pin;
use core::sync::atomic::{AtomicU8, Ordering};
use core::task::{Context, Poll};

#[repr(align(16))]
struct Frame<const N: usize>([MaybeUninit<u8>; N]);

impl<const N: usize> Frame<N> {
    /// Build outside the caller's poll frame. Poll through a trait object to
    /// keep the selected handler's stack frame separate from its caller.
    #[inline(never)]
    fn place<'a, F: Future + 'a>(
        &'a mut self,
        mask: u8,
        make: impl FnOnce() -> F,
    ) -> Placed<'a, F::Output> {
        const { assert!(size_of::<F>() <= N && align_of::<F>() <= align_of::<Self>()) };
        let ptr = self.0.as_mut_ptr().cast::<F>();
        // SAFETY: the aligned slot is exclusively borrowed for the future's
        // lifetime. Placed drops it in place before that borrow ends.
        unsafe {
            ptr.write(make());
            Placed(Pin::new_unchecked(&mut *ptr), mask)
        }
    }
}

struct Placed<'a, T>(Pin<&'a mut (dyn Future<Output = T> + 'a)>, u8);

impl<T> Future for Placed<'_, T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        self.0.as_mut().poll(cx)
    }
}

impl<T> Drop for Placed<'_, T> {
    fn drop(&mut self) {
        // SAFETY: Frame::place initialized this object and transferred
        // ownership here. The underlying byte array has no destructor.
        unsafe { core::ptr::drop_in_place(self.0.as_mut().get_unchecked_mut()) };
        TAKEN.fetch_and(!self.1, Ordering::Release);
    }
}

// These are the four nested lifetimes of one web request. NTS-KE runs
// under the same TLS session lock and can use the entire area.
#[repr(C)]
struct WebFrames {
    session: Frame<704>,
    http: Frame<2816>,
    route: Frame<3712>,
    handler: Frame<3680>,
}

#[unsafe(link_section = ".dram2_uninit")]
static mut FRAMES: MaybeUninit<WebFrames> = MaybeUninit::uninit();
static TAKEN: AtomicU8 = AtomicU8::new(0);

/// Acquire disjoint fields for the web server or every field for NTS. The
/// bit remains set until Placed drops the actual future, including cancellation.
///
/// # Safety
/// `field` must point to a complete aligned Frame in FRAMES. Any overlapping
/// regions must share at least one bit in their masks.
#[inline(never)]
unsafe fn acquire<'a, F: Future + 'a, const N: usize>(
    mask: u8,
    field: *mut Frame<N>,
    make: impl FnOnce() -> F,
) -> Placed<'a, F::Output> {
    assert_eq!(TAKEN.fetch_or(mask, Ordering::Acquire) & mask, 0);
    // SAFETY: all callers pass either a disjoint field and its unique bit,
    // or the entire aligned backing store and every bit. Its bytes may
    // be uninitialized. Only Placed releases them for the next caller.
    unsafe { (&mut *field).place(mask, make) }
}

macro_rules! web_frame {
    ($field:ident, $bit:expr) => {
        pub fn $field<'a, F: Future + 'a>(
            make: impl FnOnce() -> F,
        ) -> impl Future<Output = F::Output> + 'a {
            let base = (&raw mut FRAMES).cast::<WebFrames>();
            // SAFETY: project a raw pointer without borrowing any sibling field.
            unsafe { acquire($bit, &raw mut (*base).$field, make) }
        }
    };
}

web_frame!(session, 1);
web_frame!(http, 2);
web_frame!(route, 4);
web_frame!(handler, 8);

pub fn nts<'a, F: Future + 'a>(make: impl FnOnce() -> F) -> impl Future<Output = F::Output> + 'a {
    let entire = (&raw mut FRAMES).cast::<Frame<{ size_of::<WebFrames>() }>>();
    // SAFETY: WebFrames is at least 16-byte aligned, spans this entire Frame
    // and 0b1111 reserves all four fields.
    unsafe { acquire(0b1111, entire, make) }
}
