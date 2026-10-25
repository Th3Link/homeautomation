//! The `SensorConfig` CAN messages of a node: the list of its temperature
//! sensors, their names and their roles in the heating controller (see
//! [`cancomponents_core::sensor_config`] for the frames and
//! [`cancomponents_core::sensor_names`] for the bookkeeping, which is where
//! the logic lives and is tested). This module carries it out: it answers
//! with the sensors of [`crate::temperature`], keeps the names in flash and
//! writes the roles through the heating controller's own settings.
//!
//! A bare R frame asks for the whole list. It is answered by a task of its
//! own, a few milliseconds between the frames, so that neither the CAN
//! receive task waits for it nor the transmit queue overflows.

use crate::can::send_can_message;
use crate::config::config;
use crate::console_log;
use crate::error::{report_error, Component, ErrorCode, Severity};
use crate::{heating, temperature};
use cancomponents_core::can_id::CanId;
use cancomponents_core::can_message_type::CanMessageType;
use cancomponents_core::sensor_config::{SensorConfig, SensorName, SensorRole};
use cancomponents_core::sensor_names::{
    assign_role, guard_matches, role_of, NameError, SensorNames, ENTRY_LEN, MAX_NAMED,
};
use core::cell::Cell;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};

/// Flash key of the first sensor name entry; entry `n` lives at
/// `NAMES_KEY_BASE + n` (the serial, then the name). Raw keys, clear of
/// [`crate::config::Key`] and the tank sensor keys (0x60…).
const NAMES_KEY_BASE: u8 = 0x40;
/// Gap between the frames of a list, and between the two frames of a sensor.
const FRAME_GAP: Duration = Duration::from_millis(3);

static NAMES: BlockingMutex<CriticalSectionRawMutex, Cell<SensorNames>> =
    BlockingMutex::new(Cell::new(SensorNames::new()));
/// Set to have the whole list sent.
static SEND_LIST: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Why a request was refused; the number goes into the error report
/// (`Component::Sensor`, `InvalidData`) after the slot.
#[derive(Clone, Copy)]
enum Refusal {
    NoSuchSlot = 1,
    NameTaken = 2,
    NamesFull = 3,
    Storage = 4,
    GuardMismatch = 5,
    NotAController = 6,
    NoSuchRole = 7,
    Malformed = 8,
}

/// The name of the sensor with this serial, if it has one.
pub fn name_of(serial: &[u8; 6]) -> Option<SensorName> {
    NAMES.lock(|names| names.get().get(serial))
}

/// Loads the names from flash and starts the task that sends the list.
pub async fn init(spawner: &Spawner) {
    for index in 0..MAX_NAMED {
        let stored = config()
            .await
            .get_bytes::<ENTRY_LEN>(NAMES_KEY_BASE + index as u8)
            .await;
        if let Some(bytes) = stored {
            NAMES.lock(|cell| {
                let mut names = cell.get();
                names.load_entry(index, &bytes);
                cell.set(names);
            });
        }
    }
    spawner.spawn(list_task().unwrap());
}

/// Names the sensor with serial `serial` and stores the name.
pub async fn set_name(serial: [u8; 6], name: SensorName) -> Result<(), &'static str> {
    let mut names = NAMES.lock(Cell::get);
    let index = names.set(serial, name).map_err(|error| match error {
        NameError::Taken => "another sensor has this name",
        NameError::Full => "too many sensors have names",
    })?;
    let entry = names.entry_bytes(index).ok_or("no such entry")?;
    config()
        .await
        .set_bytes(NAMES_KEY_BASE + index as u8, &entry)
        .await
        .map_err(|()| "storing the name failed")?;
    NAMES.lock(|cell| cell.set(names));
    Ok(())
}

/// The role the controller gives the sensor with topic id `id`.
pub fn role_for(id: u32) -> SensorRole {
    let settings = heating::settings();
    role_of(id, &settings.tank_sensor_ids, settings.flow_sensor_id)
}

#[embassy_executor::task]
async fn list_task() {
    loop {
        SEND_LIST.wait().await;
        for sensor in temperature::sensors() {
            send_sensor(sensor.slot as u8).await;
            Timer::after(FRAME_GAP).await;
        }
    }
}

async fn send(message: SensorConfig) {
    if let Ok(payload) = message.to_payload() {
        send_can_message(CanMessageType::SensorConfig, payload.as_slice(), false).await;
    }
}

/// Sends the info frame of the sensor in `slot`, and its name frame if it has
/// a name.
async fn send_sensor(slot: u8) {
    let Some(sensor) = temperature::sensors()
        .into_iter()
        .find(|s| s.slot == usize::from(slot))
    else {
        return;
    };
    let serial = sensor.rom.serial();
    send(SensorConfig::Info {
        slot,
        serial,
        lost: sensor.lost,
        settling: sensor.settling,
        role: role_for(sensor.rom.topic_id()),
    })
    .await;
    if let Some(name) = name_of(&serial) {
        Timer::after(FRAME_GAP).await;
        send(SensorConfig::Name { slot, name }).await;
    }
}

async fn refuse(slot: u8, why: Refusal) {
    console_log!("sensors: request for slot {slot} refused ({})", why as u8);
    report_error(
        Component::Sensor,
        ErrorCode::InvalidData,
        Severity::Warning,
        CanMessageType::SensorConfig as u8,
        &[slot, why as u8],
    )
    .await;
}

/// Handles a `SensorConfig` frame.
pub async fn handler(_id: CanId, data: &[u8], remote_request: bool) {
    if remote_request {
        SEND_LIST.signal(());
        return;
    }
    let Ok(message) = SensorConfig::from_bytes(data) else {
        refuse(data.first().map_or(0, |b| b & 0b1_1111), Refusal::Malformed).await;
        return;
    };
    match message {
        SensorConfig::InfoRequest { slot } => send_sensor(slot).await,
        SensorConfig::Name { slot, name } => {
            let Some(sensor) = sensor_in(slot) else {
                return refuse(slot, Refusal::NoSuchSlot).await;
            };
            match set_name(sensor.rom.serial(), name).await {
                Ok(()) => send_sensor(slot).await,
                Err("another sensor has this name") => refuse(slot, Refusal::NameTaken).await,
                Err("too many sensors have names") => refuse(slot, Refusal::NamesFull).await,
                Err(_) => refuse(slot, Refusal::Storage).await,
            }
        }
        SensorConfig::Role { slot, role, guard } => assign(slot, role, guard).await,
        SensorConfig::Forget => temperature::forget_lost_sensors(),
        // What a node sends, not what it takes.
        SensorConfig::Info { .. } => {}
    }
}

fn sensor_in(slot: u8) -> Option<temperature::SensorStatus> {
    temperature::sensors()
        .into_iter()
        .find(|s| s.slot == usize::from(slot))
}

/// Gives the sensor in `slot` a role — if it really is the sensor the sender
/// meant (`guard`).
async fn assign(slot: u8, role: SensorRole, guard: [u8; 4]) {
    if !heating::is_heating_controller().await {
        return refuse(slot, Refusal::NotAController).await;
    }
    let Some(sensor) = sensor_in(slot) else {
        return refuse(slot, Refusal::NoSuchSlot).await;
    };
    if !guard_matches(&sensor.rom.serial(), &guard) {
        return refuse(slot, Refusal::GuardMismatch).await;
    }
    let before = heating::settings();
    let mut tank = before.tank_sensor_ids;
    let mut flow = before.flow_sensor_id;
    if !assign_role(sensor.rom.topic_id(), role, &mut tank, &mut flow) {
        return refuse(slot, Refusal::NoSuchRole).await;
    }
    let mut stored = true;
    for (position, (new, old)) in tank.iter().zip(before.tank_sensor_ids).enumerate() {
        if *new != old {
            stored &= heating::set_tank_sensor(position, *new).await.is_ok();
        }
    }
    if flow != before.flow_sensor_id {
        stored &= heating::set_flow_sensor(flow).await.is_ok();
    }
    if !stored {
        return refuse(slot, Refusal::Storage).await;
    }
    send_sensor(slot).await;
}
