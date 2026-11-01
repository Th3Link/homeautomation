//! Home Assistant MQTT discovery ([`gateway_core::ha_discovery`], ADR
//! 0022): when it is switched on, the gateway publishes a retained
//! configuration on the primary broker for every device, sensor, presence
//! sensor and button it has seen, so that they show up in Home Assistant
//! without setting them up by hand.
//!
//! A configuration is published when something it describes is new or
//! changed (a device's name or version, a sensor's name and so its topic),
//! after every connection to the broker, and when Home Assistant says it
//! started (`homeassistant/status` = `online`). What was published is
//! remembered as a hash per topic, so nothing goes out twice.

use crate::config::config;
use crate::console_log;
use crate::device_list::DEVICE_LIST;
use crate::sensor_list::{SENSOR_INFO, SENSOR_LIST};
use alloc::vec::Vec;
use core::fmt::Write as _;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use gateway_core::config::Key;
use gateway_core::device_type::DeviceType;
use gateway_core::ha_discovery::{self, ButtonTrigger, Device, Entity};
use gateway_core::sensor_table::{named_by_node, SensorKind};
use gateway_core::topics::{button, device};
use heapless::String;

/// Whether discovery is switched on (`ha-discovery`, `PATCH /mqtt`).
static ENABLED: AtomicBool = AtomicBool::new(false);
/// Something may need publishing.
static CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Devices: which buttons (bits 0-15) and whether a presence sensor were
/// seen.
struct Features {
    key: u32,
    buttons: u16,
    presence: bool,
}

const MAX_FEATURES: usize = 64;
static FEATURES: Mutex<CriticalSectionRawMutex, heapless::Vec<Features, MAX_FEATURES>> =
    Mutex::new(heapless::Vec::new());

/// What was published: hash of the topic → hash of the configuration.
const MAX_PUBLISHED: usize = 256;
static PUBLISHED: Mutex<
    CriticalSectionRawMutex,
    heapless::index_map::FnvIndexMap<u32, u32, MAX_PUBLISHED>,
> = Mutex::new(heapless::index_map::FnvIndexMap::new());

pub async fn init(spawner: &Spawner) {
    let on = config()
        .await
        .get_bool(Key::HomeAssistant)
        .await
        .unwrap_or(false);
    ENABLED.store(on, Ordering::Relaxed);
    spawner.spawn(ha_task().unwrap());
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Switches discovery on or off and stores it.
pub async fn set_enabled(on: bool) -> Result<(), ()> {
    config().await.set_bool(Key::HomeAssistant, on).await?;
    ENABLED.store(on, Ordering::Relaxed);
    if on {
        republish().await;
    }
    Ok(())
}

/// Something that discovery describes may have changed: a new device, a
/// name, a new sensor.
pub fn changed() {
    CHANGED.signal(());
}

/// Publishes everything again: after a connection to the broker, and when
/// Home Assistant started (it may have lost what it had).
pub async fn republish() {
    PUBLISHED.lock().await.clear();
    changed();
}

/// A device sent a button event.
pub async fn observe_button(key: u32, button: u8) {
    let Some(bit) = 1u16.checked_shl(u32::from(button)) else {
        return;
    };
    let new = update_features(key, |f| {
        let new = f.buttons & bit == 0;
        f.buttons |= bit;
        new
    })
    .await;
    if new {
        changed();
    }
}

/// A device sent a presence event.
pub async fn observe_presence(key: u32) {
    if update_features(key, |f| !core::mem::replace(&mut f.presence, true)).await {
        changed();
    }
}

/// Applies `update` to `key`'s features (new ones start empty); returns
/// what it returned: whether something is new.
async fn update_features(key: u32, update: impl FnOnce(&mut Features) -> bool) -> bool {
    let mut features = FEATURES.lock().await;
    if let Some(f) = features.iter_mut().find(|f| f.key == key) {
        return update(f);
    }
    let mut f = Features {
        key,
        buttons: 0,
        presence: false,
    };
    let new = update(&mut f);
    let _ = features.push(f);
    new
}

#[embassy_executor::task]
async fn ha_task() {
    crate::heap::boxed(run).await
}

async fn run() {
    loop {
        CHANGED.wait().await;
        // Things come in bursts (a device's settings, a node's sensors):
        // let them settle.
        Timer::after(Duration::from_secs(2)).await;
        if enabled() && crate::mqtt::is_connected(crate::mqtt::PRIMARY) {
            publish_changes().await;
        }
    }
}

/// A device as the device list knows it.
struct Known {
    key: u32,
    name: String<10>,
    version: String<12>,
    model: &'static str,
}

/// A sensor as the sensor list knows it.
struct Reading {
    key: u32,
    kind: SensorKind,
    sub_id: u64,
    name: String<8>,
}

async fn publish_changes() {
    let devices: Vec<Known> = DEVICE_LIST
        .lock()
        .await
        .iter()
        .map(|e| Known {
            key: e.id,
            name: e.custom_string.clone(),
            version: e.version.clone(),
            model: DeviceType::from((e.id >> 16) as u8).name(),
        })
        .collect();
    let readings: Vec<Reading> = {
        let sensors = SENSOR_LIST.lock().await;
        let info = SENSOR_INFO.lock().await;
        sensors
            .iter()
            .map(|e| Reading {
                key: e.device_key,
                kind: e.kind,
                sub_id: e.sub_id,
                name: info
                    .find(e.device_key, e.sub_id)
                    .and_then(|i| i.name)
                    .and_then(|n| String::try_from(n.as_str()).ok())
                    .unwrap_or_default(),
            })
            .collect()
    };
    let features: Vec<(u32, u16, bool)> = FEATURES
        .lock()
        .await
        .iter()
        .map(|f| (f.key, f.buttons, f.presence))
        .collect();

    let mut sent = 0;
    for known in &devices {
        let device = Device {
            key: known.key,
            name: &known.name,
            model: known.model,
            version: &known.version,
        };
        for reading in readings.iter().filter(|r| r.key == known.key) {
            let topic = if reading.kind == SensorKind::Brightness {
                device::ambient_light_topic(known.key, &known.name)
            } else {
                device::fixed_point_sensor_topic(
                    reading.kind.message(),
                    known.key,
                    &known.name,
                    reading.sub_id,
                    &reading.name,
                )
            };
            // A device's DS18B20s are told apart by name or id; its other
            // sensors are one of a kind.
            let mut label: String<12> = String::new();
            if named_by_node(reading.kind, reading.sub_id) {
                if reading.name.is_empty() {
                    let _ = write!(label, "0x{:08x}", reading.sub_id as u32);
                } else {
                    let _ = label.push_str(&reading.name);
                }
            }
            let entity = Entity::Sensor {
                kind: reading.kind,
                sub_id: reading.sub_id,
                label: &label,
                state_topic: &topic,
            };
            sent += publish(&device, &entity).await;
        }
        let Some(&(_, buttons, presence)) = features.iter().find(|f| f.0 == known.key) else {
            continue;
        };
        if presence {
            let topic = button::presence_topic(known.key, &known.name);
            sent += publish(
                &device,
                &Entity::Presence {
                    state_topic: &topic,
                },
            )
            .await;
        }
        let topic = button::button_topic(known.key, &known.name);
        for number in (0..16u8).filter(|n| buttons & (1 << n) != 0) {
            for trigger in ButtonTrigger::ALL {
                let entity = Entity::Button {
                    button: number,
                    trigger,
                    state_topic: &topic,
                };
                sent += publish(&device, &entity).await;
            }
        }
    }
    if sent > 0 {
        console_log!("ha: {sent} discovery configuration(s) published");
    }
}

/// Publishes `entity`'s configuration if it isn't published like this
/// already; 1 if it was.
async fn publish(device: &Device<'_>, entity: &Entity<'_>) -> usize {
    let topic = ha_discovery::topic(device.key, entity);
    let Some(payload) = ha_discovery::payload(device, entity) else {
        return 0;
    };
    let (topic_hash, payload_hash) = (fnv(topic.as_bytes()), fnv(payload.as_bytes()));
    if PUBLISHED.lock().await.get(&topic_hash) == Some(&payload_hash) {
        return 0;
    }
    if !crate::mqtt::publish_retained(&topic, &payload).await {
        return 0;
    }
    let mut published = PUBLISHED.lock().await;
    if published.insert(topic_hash, payload_hash).is_err() {
        // Full: start over rather than miss changes.
        published.clear();
    }
    1
}

/// FNV-1a, 32 bits.
fn fnv(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5, |h, &b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}
