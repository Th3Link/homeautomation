//! Sending `LampGroup` commands, paced for the legacy lamp controllers.
//!
//! Those controllers (`LegacyLamps`) handle one change per second, so their
//! commands go through [`LampPacer`]: gathered into blocks, newest value
//! per lamp wins, one block per second. Commands for any other device go
//! out directly.

use embassy_futures::select::select;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Instant, Timer};
use gateway_core::can_id::CanId;
use gateway_core::can_message_type::CanMessageType;
use gateway_core::device_type::DeviceType;
use gateway_core::lamp::LampMsg;
use gateway_core::lamp_pacer::{LampPacer, Target, MIN_INTERVAL_MS};

/// Controllers (or controller banks) paced at the same time.
const SLOTS: usize = 8;

static PACER: Mutex<CriticalSectionRawMutex, LampPacer<SLOTS>> =
    Mutex::new(LampPacer::new(MIN_INTERVAL_MS));
static WAKE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Sends `msg` to the device `device_key`: the 8-byte form with its bank
/// if `with_bank`, else the 4-byte form.
pub async fn send(device_key: u32, msg: LampMsg, with_bank: bool) {
    let target = Target {
        device_key,
        bank: with_bank.then_some(msg.bank),
    };
    if paced(device_key)
        && PACER
            .lock()
            .await
            .push(Instant::now().as_millis(), target, msg)
    {
        WAKE.signal(());
        return;
    }
    send_now(target, msg).await;
}

/// Whether the device is one of the controllers that need pacing.
fn paced(device_key: u32) -> bool {
    CanId::from(device_key).device_type == u8::from(DeviceType::LegacyLamps)
}

async fn send_now(target: Target, msg: LampMsg) {
    let id = target.device_key + u32::from(u8::from(CanMessageType::LampGroup));
    match target.bank {
        Some(_) => crate::can::send_raw(id, &msg.to_bytes_full(), false).await,
        None => crate::can::send_raw(id, &msg.to_bytes_short(), false).await,
    }
}

/// Sends the paced blocks when they are due.
#[embassy_executor::task]
pub async fn pacer_task() {
    loop {
        let next = PACER.lock().await.next_due();
        match next {
            Some(due) => {
                select(WAKE.wait(), Timer::at(Instant::from_millis(due))).await;
            }
            None => WAKE.wait().await,
        }
        loop {
            let block = PACER.lock().await.take_due(Instant::now().as_millis());
            let Some(block) = block else { break };
            for frame in block.frames {
                send_now(block.target, frame).await;
            }
        }
    }
}
