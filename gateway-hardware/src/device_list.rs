//! Async singleton wrapper around [`gateway_core::device_list::DeviceTable`],
//! fed by [`crate::can`]'s receive task and read by [`crate::web`] for
//! `/state.json`.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use gateway_core::device_list::DeviceTable;

/// Matches the original `DeviceList::DEVICE_LIST_SIZE`.
pub const DEVICE_LIST_SIZE: usize = 100;

pub static DEVICE_LIST: Mutex<CriticalSectionRawMutex, DeviceTable<DEVICE_LIST_SIZE>> =
    Mutex::new(DeviceTable::new());

/// "Now", in minutes, for [`gateway_core::device_list::DeviceTable::observe`]'s
/// `last_seen` tracking — matches the original's
/// `esp_timer_get_time()`-derived minute counter.
pub fn now_minutes() -> u32 {
    (embassy_time::Instant::now().as_secs() / 60) as u32
}
