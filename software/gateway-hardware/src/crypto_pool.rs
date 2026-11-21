//! MbedTLS owns a bounded SRAM1 arena. Credentials and concurrent certificate
//! checks can outlive a session, so each allocation is freed independently.

use core::alloc::Layout;
use core::cell::RefCell;
use core::ffi::c_void;
use core::mem::MaybeUninit;
use core::ptr::{null_mut, NonNull};
use critical_section::Mutex;
use linked_list_allocator::Heap;

pub const SIZE: usize = 48 * 1024;
// Reserve an aligned header before each C allocation to recover its layout in free.
const ALIGN: usize = 16;

struct Pool {
    heap: Heap,
    peak: usize,
    failures: usize,
}
static POOL: Mutex<RefCell<Pool>> = Mutex::new(RefCell::new(Pool {
    heap: Heap::empty(),
    peak: 0,
    failures: 0,
}));

/// Called during boot before creating the first MbedTLS object.
pub fn init() {
    #[unsafe(link_section = ".dram2_uninit")]
    static mut ARENA: [MaybeUninit<u8>; SIZE] = [MaybeUninit::uninit(); SIZE];
    critical_section::with(|cs| {
        let mut pool = POOL.borrow_ref_mut(cs);
        assert!(pool.heap.bottom().is_null());
        // SAFETY: this arena is exclusive to POOL for the rest of the boot.
        unsafe {
            pool.heap.init((&raw mut ARENA).cast(), SIZE);
            mbedtls_rs::sys::mbedtls_platform_set_calloc_free(Some(calloc), Some(free));
        }
    });
}

unsafe extern "C" fn calloc(count: usize, size: usize) -> *mut c_void {
    critical_section::with(|cs| {
        let mut pool = POOL.borrow_ref_mut(cs);
        let layout = count
            .checked_mul(size)
            .and_then(|bytes| bytes.max(1).checked_add(ALIGN))
            .and_then(|bytes| Layout::from_size_align(bytes, ALIGN).ok());
        if let Some(layout) = layout {
            if let Ok(block) = pool.heap.allocate_first_fit(layout) {
                pool.peak = pool.peak.max(pool.heap.used());
                // SAFETY: the aligned allocation includes the header and payload.
                unsafe {
                    let ptr = block.as_ptr();
                    ptr.cast::<usize>().write(layout.size());
                    let data = ptr.add(ALIGN);
                    data.write_bytes(0, layout.size() - ALIGN);
                    return data.cast();
                }
            }
        }
        pool.failures += 1;
        null_mut()
    })
}

unsafe extern "C" fn free(data: *mut c_void) {
    if !data.is_null() {
        // SAFETY: MbedTLS returns the payload pointer from calloc exactly once.
        unsafe {
            let ptr = data.cast::<u8>().sub(ALIGN);
            let layout = Layout::from_size_align_unchecked(ptr.cast::<usize>().read(), ALIGN);
            critical_section::with(|cs| {
                POOL.borrow_ref_mut(cs)
                    .heap
                    .deallocate(NonNull::new_unchecked(ptr), layout);
            });
        }
    }
}

#[derive(serde::Serialize)]
pub struct Stats {
    pub size: usize,
    pub used: usize,
    pub peak: usize,
    pub failures: usize,
}

pub fn stats() -> Stats {
    critical_section::with(|cs| {
        let pool = POOL.borrow_ref(cs);
        Stats {
            size: pool.heap.size(),
            used: pool.heap.used(),
            peak: pool.peak,
            failures: pool.failures,
        }
    })
}
