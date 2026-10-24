//! `GET /api/v1/system`.

use picoserve::response::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct System {
    firmware_version: &'static str,
    uptime_s: u64,
    heap_used: usize,
    heap_free: usize,
}

pub async fn get_system() -> Json<System> {
    Json(System {
        firmware_version: env!("VERGEN_GIT_DESCRIBE"),
        uptime_s: embassy_time::Instant::now().as_secs(),
        heap_used: esp_alloc::HEAP.used(),
        heap_free: esp_alloc::HEAP.free(),
    })
}
