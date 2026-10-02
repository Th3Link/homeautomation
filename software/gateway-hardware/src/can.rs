//! CAN/TWAI transport: bus init, frame dispatch, and the send queue.
//!
//! Mirrors `library/esp32-ha-lib/CAN.cpp`'s transport mechanics
//! (`cancomponent-rs/cc-hardware/src/can.rs`'s task/channel structure is
//! the closer Rust reference), but **promiscuous**: the original gateway
//! runs with `enable_filter=false` (it needs to see every device's
//! traffic to bridge it to MQTT, unlike a single-purpose node), so the
//! hardware acceptance filter here is wired wide open
//! (`ACCEPT_ALL_ID`/`ACCEPT_ALL_RTR`) rather than narrowed to one
//! device's own id/type. There's also no single "this device" dispatch
//! table walk — [`dispatch`] calls each interested handler directly,
//! which is behaviorally the same as the original's
//! `ICANDispatcher` fan-out for every live message type (none of them
//! actually contend over the same `msg_type`).

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embedded_can::Frame;
use esp_hal::gpio::{InputPin, OutputPin};
use esp_hal::twai::filter::SingleExtendedFilter;
use esp_hal::twai::{self, EspTwaiFrame, TimingConfig, TwaiMode};
use esp_hal::Async;
use gateway_core::can_id::CanId;
use gateway_core::can_message_type::CanMessageType;

use crate::console_log;
use crate::device_list::{now_minutes, DEVICE_LIST};

/// Outgoing frames queued by [`send_can_message`]/[`send_raw`], drained by
/// `can_send_task`.
pub static CAN_CHANNEL: Channel<CriticalSectionRawMutex, EspTwaiFrame, 32> = Channel::new();

/// Mirrors `config::Key::CanId`; seeded at boot, updated live if the
/// console's device-id command changes it (the acceptance filter itself is
/// wide open regardless, see the module doc, so unlike a node there's no
/// "reprogram the filter" step needed on change).
pub static DEVICE_ID: AtomicU8 = AtomicU8::new(0xFF);
/// See [`DEVICE_ID`]; same caveats, for `config::Key::CanType`.
pub static DEVICE_TYPE: AtomicU8 = AtomicU8::new(0xFF);
/// Set by an incoming `UpdateSilence` message; while set,
/// [`send_can_message`] drops everything above [`CanMessageType::FlashVerify`]
/// (matches `CAN::send(MSG_ID_t, ...)`'s `m_update_silence` gate exactly —
/// [`send_raw`], used for CAN-OTA orchestration and debug passthrough, is
/// deliberately never gated by it, matching the original's separate
/// `send(uint32_t, ...)` overload).
pub static SILENCE: AtomicBool = AtomicBool::new(false);

const ACCEPT_ALL_ID: [u8; 29] = [b'x'; 29];
const ACCEPT_ALL_RTR: [u8; 1] = *b"x";

/// Starts the TWAI peripheral in fully-promiscuous mode and spawns the
/// send/receive tasks. Blocks while the bus reports "bus off" (no other
/// node acking, or a wiring issue).
pub async fn init(
    twai: esp_hal::peripherals::TWAI0<'static>,
    rx: impl InputPin + 'static,
    tx: impl OutputPin + 'static,
    spawner: &Spawner,
) {
    // NOTE: fixed regardless of `config::Key::CanBitrate` — the original
    // gateway's `CAN::init` never actually applies the persisted bitrate to
    // the peripheral either (it's stored/settable but dynamic switching
    // isn't wired up).
    const TC: TimingConfig = TimingConfig {
        baud_rate_prescaler: 80,
        sync_jump_width: 3,
        tseg_1: 15,
        tseg_2: 4,
        triple_sample: false,
    };
    const TWAI_BAUDRATE: twai::BaudRate = twai::BaudRate::Custom(TC);

    let mut twai_config =
        twai::TwaiConfiguration::new(twai, rx, tx, TWAI_BAUDRATE, TwaiMode::Normal);
    let filter = SingleExtendedFilter::new(&ACCEPT_ALL_ID, &ACCEPT_ALL_RTR);
    twai_config.set_filter(filter);
    let twai = twai_config.into_async().start();
    while twai.is_bus_off() {
        console_log!("waiting for bus_off");
        embassy_time::Timer::after_millis(100).await;
    }
    let (rx, tx) = twai.split();

    spawner.spawn(can_send_task(tx).unwrap());
    spawner.spawn(can_receive_task(rx).unwrap());
}

/// Handles one received frame: updates the [`SILENCE`] latch, feeds
/// [`crate::device_list`] (replying with a `RequestParameter` poll on a
/// first sighting), and routes it to whichever bridge cares about its
/// `msg_type`.
pub async fn dispatch(frame: &EspTwaiFrame) {
    let id = match frame.id() {
        embedded_can::Id::Extended(id) => CanId::from(id),
        embedded_can::Id::Standard(id) => {
            console_log!("WARN: ignoring standard ID: {:?}", id);
            return;
        }
    };
    let data = frame.data();
    let request = frame.is_remote_frame();

    if id.msg_type == CanMessageType::UpdateSilence {
        if let Some(&b) = data.first() {
            SILENCE.store(b != 0, Ordering::Relaxed);
        }
    }

    {
        let mut table = DEVICE_LIST.lock().await;
        let sighting = table.observe(id, data, now_minutes());
        drop(table);
        if let Some(req) = sighting {
            send_raw(
                gateway_core::can_ota::target_id(req.device_key, CanMessageType::RequestParameter),
                &[],
                true,
            )
            .await;
        }
    }

    crate::translate::dispatch(id, data, request).await;
}

/// Queues an outgoing frame addressed from this device's own id/type
/// (`CAN::send(MSG_ID_t, ...)`). A no-op while [`SILENCE`] is set and
/// `msg_type` is above [`CanMessageType::FlashVerify`]. `data` must be 8
/// bytes or fewer.
pub async fn send_can_message(msg_type: CanMessageType, data: &[u8], request: bool) {
    if SILENCE.load(Ordering::Relaxed) && u8::from(msg_type) > u8::from(CanMessageType::FlashVerify)
    {
        return;
    }
    let id = CanId::new(
        DEVICE_TYPE.load(Ordering::Relaxed),
        DEVICE_ID.load(Ordering::Relaxed),
        msg_type,
    );
    send_raw(id.into(), data, request).await;
}

/// Queues an outgoing frame with an already-fully-composed 29-bit id
/// (`CAN::send(uint32_t, ...)`) — never gated by [`SILENCE`]. Used for
/// CAN-OTA orchestration and the debug passthrough, both of which target
/// arbitrary devices rather than speaking as this one.
pub async fn send_raw(can_id: u32, data: &[u8], request: bool) {
    let ext = embedded_can::ExtendedId::new(can_id & 0x1FFF_FFFF)
        .expect("29-bit mask always fits an ExtendedId");
    let ext: esp_hal::twai::ExtendedId = ext.into();
    let frame = if request {
        EspTwaiFrame::new_remote(ext, data.len()).expect("request payload length fits a CAN frame")
    } else {
        EspTwaiFrame::new(ext, data).expect("payload fits a CAN frame")
    };
    CAN_CHANNEL.send(frame).await;
}

#[embassy_executor::task]
async fn can_receive_task(mut rx: twai::TwaiRx<'static, Async>) {
    console_log!("can_receive_task started");
    loop {
        if let Ok(frame) = rx.receive_async().await {
            dispatch(&frame).await;
        } else {
            console_log!("recv error");
        }
    }
}

/// Drains [`CAN_CHANNEL`] and transmits each frame. A failed transmission
/// (e.g. nothing on the bus acknowledges it) is logged and dropped rather
/// than crashing the gateway.
#[embassy_executor::task]
async fn can_send_task(mut tx: twai::TwaiTx<'static, Async>) {
    console_log!("can_send_task started");
    loop {
        let frame = CAN_CHANNEL.receive().await;
        if let Err(e) = tx.transmit_async(&frame).await {
            console_log!("send error: {e:?}");
        }
    }
}
