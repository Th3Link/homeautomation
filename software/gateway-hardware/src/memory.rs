//! High-water marks for the allocator and the main stack, both since boot.

use core::sync::atomic::{AtomicU32, Ordering};

static BOOT_ID: AtomicU32 = AtomicU32::new(0);

pub fn init() {
    BOOT_ID.store(esp_hal::rng::Rng::new().random(), Ordering::Relaxed);
}

pub struct Memory {
    pub boot_id: u32,
    pub uptime_ms: u64,
    pub heap_size: usize,
    pub heap_used: usize,
    pub heap_free: usize,
    pub heap_peak_used: usize,
    pub heap_min_free: usize,
    pub stack_size: usize,
    pub stack_untouched: usize,
    pub stack_peak_used: usize,
}

impl Memory {
    pub fn capture() -> Self {
        let heap = esp_alloc::HEAP.stats();
        let stack_size = crate::stack::size();
        let stack_untouched = crate::stack::untouched();
        Self {
            boot_id: BOOT_ID.load(Ordering::Relaxed),
            uptime_ms: embassy_time::Instant::now().as_millis(),
            heap_size: heap.size,
            heap_used: heap.current_usage,
            heap_free: heap.size - heap.current_usage,
            heap_peak_used: heap.max_usage,
            heap_min_free: heap.size - heap.max_usage,
            stack_size,
            stack_untouched,
            stack_peak_used: stack_size - stack_untouched,
        }
    }
}

#[cfg(feature = "memory-profile")]
pub fn log() {
    let m = Memory::capture();
    // Field order is shared with the simulator's serial parser.
    crate::console_log!(
        "memory: {} {} {} {} {} {} {}",
        m.boot_id,
        m.uptime_ms,
        m.heap_size,
        m.heap_used,
        m.heap_peak_used,
        m.stack_size,
        m.stack_untouched
    );
}

#[cfg(feature = "memory-profile")]
#[embassy_executor::task]
pub async fn sample_task() {
    let mut interval = embassy_time::Ticker::every(embassy_time::Duration::from_secs(2));
    loop {
        log();
        interval.next().await;
    }
}

#[cfg(feature = "memory-profile")]
pub fn probe() -> bool {
    let mut bytes = alloc::vec::Vec::<u8>::new();
    if bytes.try_reserve_exact(8192).is_err() {
        return false;
    }
    core::hint::black_box(&mut bytes);
    drop(bytes);
    true
}
