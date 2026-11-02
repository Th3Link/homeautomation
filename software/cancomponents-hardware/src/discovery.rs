//! Announces this node to the gateway ([`cancomponents_core::discovery`],
//! ADR 0021): `Discovery` with the node's name after start, after a change
//! of the name and when the gateway asks, repeated at random intervals
//! until the gateway's `DiscoveryAck` arrives.

use crate::can::{send_can_message, DEVICE_ID};
use crate::console_log;
use cancomponents_core::can_message_type::CanMessageType;
use cancomponents_core::discovery::{encode, first_delay_ms, retry_delay_ms, FAST_TRIES};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Timer;

/// Start (or keep) announcing.
static ANNOUNCE: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// The gateway acked.
static ACKED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Starts the announcing task and the first announcement.
pub fn init(spawner: &Spawner) {
    spawner.spawn(discovery_task().unwrap());
    announce();
}

/// Announces the node (again): after a change of its name, or because the
/// gateway asked (`Discovery` R frame).
pub fn announce() {
    ANNOUNCE.signal(());
}

/// Handles `DiscoveryAck`.
pub fn acked() {
    ACKED.signal(());
}

#[embassy_executor::task]
async fn discovery_task() {
    let rng = esp_hal::rng::Rng::new();
    loop {
        ANNOUNCE.wait().await;
        // Nodes started (or asked) together get different random numbers
        // even if their generators start alike: mix in the device id.
        let id = u32::from(*DEVICE_ID.lock().await);
        let random = move || rng.random() ^ id.wrapping_mul(0x9E37_79B9);

        Timer::after_millis(u64::from(first_delay_ms(random()))).await;
        ACKED.reset();
        let mut sent = 0;
        loop {
            // The current name: it may have changed while announcing.
            let name = crate::device::device().await.name().clone();
            send_can_message(CanMessageType::Discovery, encode(&name).as_slice(), false).await;
            sent += 1;
            let wait = Timer::after_millis(u64::from(retry_delay_ms(sent, random())));
            if let Either::First(()) = select(ACKED.wait(), wait).await {
                break;
            }
            if sent == FAST_TRIES {
                console_log!("discovery: no ack from the gateway yet, trying less often");
            }
        }
        console_log!("discovery: acked after {sent} announcement(s)");
    }
}
