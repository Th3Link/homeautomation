//! Async singleton wrapper around [`gateway_core::sensor_table::SensorTable`],
//! fed by [`crate::translate`] with every fixed-point sensor message and read
//! by the web server (`/api/v1/sensors`).

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use gateway_core::sensor_table::SensorTable;

pub const SENSOR_LIST_SIZE: usize = 48;

pub static SENSOR_LIST: Mutex<CriticalSectionRawMutex, SensorTable<SENSOR_LIST_SIZE>> =
    Mutex::new(SensorTable::new());

/// "Now", in seconds, for the readings' age.
pub fn now_seconds() -> u32 {
    embassy_time::Instant::now().as_secs() as u32
}
