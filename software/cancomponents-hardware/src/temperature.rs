//! DS18B20 temperature sensors on a 1-Wire bus (up to [`MAX_SENSORS`] on one
//! pin).
//!
//! A single task measures every [`SAMPLE_PERIOD`] (10 s) and keeps the
//! latest values in a shared bank, available to the rest of the firmware via
//! [`snapshot`]/[`reading`] and announced through [`subscribe`] — this is
//! what a later mixer controller should consume instead of going through
//! CAN. Independently, the readings are sent as `TemperatureSensor` frames
//! every [`send_interval_secs`] (default 30 s, persisted, see
//! [`set_send_interval_secs`]).
//!
//! All protocol logic (ROM search, CRC, scratchpad decoding, the CAN
//! payload, publish timing) lives in
//! [`cancomponents_core::ds18b20`] and is unit-tested on the host; this
//! module is the bit-banged bus driver plus the task orchestration.
//!
//! A sensor that fails [`LOST_AFTER_FAILED_CYCLES`] measurement cycles in a
//! row is *lost*: its reading is dropped from the shared bank (so a consumer
//! never works with a stale value) and a `DeviceError` report
//! (`Component::Sensor`, `ErrorCode::SensorLost`) goes out on CAN, repeated
//! every minute while it stays lost, followed by `ErrorCode::SensorRecovered`
//! when it answers again. `local_code` is the sensor's slot and `details` its
//! first four serial bytes — the same id the gateway puts into the MQTT
//! topic. The list of sensors seen is kept in flash, so a sensor that's
//! already dead after a reboot (the echo watchdog restarts the device
//! whenever the bus goes quiet) is still reported. It stays on the list until
//! [`forget_lost_sensors`] — use that when removing one on purpose.
//!
//! Assumptions about the wiring: the sensors are externally powered (3-wire,
//! not parasite power) and the data line has an external pull-up.
//!
//! # Bus timing
//!
//! There's no 1-Wire peripheral on the ESP32, so slots are bit-banged with
//! microsecond delays. Only the part of a slot that is timing-critical on
//! the *low* side (or the sample point of a read) runs inside a critical
//! section; recovery times may stretch freely if an interrupt comes in. That
//! keeps interrupts masked for at most ~60 µs at a time, which the CAN
//! controller's receive FIFO easily absorbs.

use crate::can::send_can_message;
use crate::config::{self, config};
use crate::console_log;
use crate::error::{report_error, Component, ErrorCode, Severity};
use cancomponents_core::can_message_type::CanMessageType;
pub use cancomponents_core::ds18b20::LOST_AFTER_FAILED_CYCLES;
use cancomponents_core::ds18b20::{
    can_payload, command, parse_scratchpad, HealthEvent, HealthEventKind, OneWireBus, PublishTimer,
    Reading, RomCode, RomSearch, SensorBank, SensorHealth, Temperature,
};
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use embassy_executor::Spawner;
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::watch::{Receiver, Watch};
use embassy_time::{Duration, Instant, Ticker, Timer};
use esp_hal::delay::Delay;
use esp_hal::gpio::{DriveMode, Flex, OutputConfig, Pin, Pull};
use heapless::Vec;

/// Sensors handled on one bus. More are ignored (and logged).
pub const MAX_SENSORS: usize = 32;
/// How often every sensor is measured.
pub const SAMPLE_PERIOD: Duration = Duration::from_secs(10);
/// Default for [`send_interval_secs`].
pub const DEFAULT_SEND_INTERVAL_SECS: u32 = 30;
/// Shortest allowed CAN send interval — there are no fresher values than
/// one per [`SAMPLE_PERIOD`] to send.
pub const MIN_SEND_INTERVAL_SECS: u32 = SAMPLE_PERIOD.as_secs() as u32;
/// Longest allowed CAN send interval (one day).
pub const MAX_SEND_INTERVAL_SECS: u32 = 24 * 60 * 60;

/// Flash key of sensor slot 0; slot `n` lives at `SENSOR_KEY_BASE + n`
/// (0x40..=0x5F, clear of the `config::Key` values). The value is the 8 ROM
/// bytes; all zeros marks an unused slot.
const SENSOR_KEY_BASE: u8 = 0x40;

/// Re-search the bus (find new / lost sensors) every this many cycles.
const RESCAN_EVERY_CYCLES: u32 = 6;
/// `Convert T` takes up to 750 ms at 12 bit resolution.
const CONVERSION_TIMEOUT: Duration = Duration::from_millis(1_000);
/// After this many cycles in a row without a single good reading, the bus is
/// searched again right away instead of waiting for the next periodic scan.
const FAILED_CYCLES_BEFORE_RESCAN: u32 = 3;

static BANK: BlockingMutex<CriticalSectionRawMutex, RefCell<SensorBank<MAX_SENSORS>>> =
    BlockingMutex::new(RefCell::new(SensorBank::new()));
/// Every sensor seen so far and whether it's answering; its order is the
/// slot numbering used in error reports and by the CLI.
static HEALTH: BlockingMutex<CriticalSectionRawMutex, RefCell<SensorHealth<MAX_SENSORS>>> =
    BlockingMutex::new(RefCell::new(SensorHealth::new()));
static FORGET_LOST: AtomicBool = AtomicBool::new(false);
/// Counts completed measurement cycles; see [`subscribe`].
static CYCLES: Watch<CriticalSectionRawMutex, u32, 2> = Watch::new();
static SEND_INTERVAL_SECS: AtomicU32 = AtomicU32::new(DEFAULT_SEND_INTERVAL_SECS);

/// Receiver for the "new measurements are available" notification.
pub type UpdateReceiver = Receiver<'static, CriticalSectionRawMutex, u32, 2>;

/// A copy of the latest reading of every sensor currently on the bus.
pub fn snapshot() -> Vec<Reading, MAX_SENSORS> {
    BANK.lock(|bank| {
        Vec::from_slice(bank.borrow().as_slice()).expect("bank is never larger than MAX_SENSORS")
    })
}

/// The latest reading of one sensor, if it's known.
pub fn reading(rom: &RomCode) -> Option<Reading> {
    BANK.lock(|bank| bank.borrow().get(rom).copied())
}

/// What's known about one sensor, see [`sensors`].
#[derive(Debug, Clone, Copy)]
pub struct SensorStatus {
    /// Position in the discovery order; also the `local_code` of its error
    /// reports.
    pub slot: usize,
    pub rom: RomCode,
    /// Its latest reading — `None` while the sensor is lost, or hasn't been
    /// read successfully yet.
    pub reading: Option<Reading>,
    pub lost: bool,
}

/// Every sensor seen since boot, lost ones included, in slot order.
pub fn sensors() -> Vec<SensorStatus, MAX_SENSORS> {
    HEALTH.lock(|health| {
        let health = health.borrow();
        BANK.lock(|bank| {
            let bank = bank.borrow();
            health
                .roms()
                .enumerate()
                .map(|(slot, rom)| SensorStatus {
                    slot,
                    rom,
                    reading: bank.get(&rom).copied(),
                    lost: health.is_lost(&rom),
                })
                .collect()
        })
    })
}

/// Stops tracking sensors that are currently lost (for ones that were
/// removed on purpose), so they stop being reported. Applied at the start of
/// the next measurement cycle; a sensor that's still connected is simply
/// found again by the next bus scan.
pub fn forget_lost_sensors() {
    FORGET_LOST.store(true, Ordering::Relaxed);
}

/// Subscribes to measurement-cycle completions: `changed().await` resolves
/// each time [`snapshot`] holds fresh data. At most two subscribers exist at
/// once; returns `None` when both slots are taken.
pub fn subscribe() -> Option<UpdateReceiver> {
    CYCLES.receiver()
}

/// Seconds between `TemperatureSensor` CAN frames.
pub fn send_interval_secs() -> u32 {
    SEND_INTERVAL_SECS.load(Ordering::Relaxed)
}

/// Sets and persists the CAN send interval (clamped to
/// [`MIN_SEND_INTERVAL_SECS`]..=[`MAX_SEND_INTERVAL_SECS`]). Takes effect
/// within one measurement cycle. Returns the value actually applied, or
/// `Err` if it couldn't be persisted (it is then not applied either).
pub async fn set_send_interval_secs(secs: u32) -> Result<u32, ()> {
    let secs = secs.clamp(MIN_SEND_INTERVAL_SECS, MAX_SEND_INTERVAL_SECS);
    config()
        .await
        .set_u32(config::Key::SensorSendInterval, secs)
        .await?;
    SEND_INTERVAL_SECS.store(secs, Ordering::Relaxed);
    Ok(secs)
}

/// Starts measuring the DS18B20 sensors on `pin`.
pub async fn init(pin: impl Pin + 'static, spawner: &Spawner) {
    if let Some(secs) = config()
        .await
        .get_u32(config::Key::SensorSendInterval)
        .await
    {
        SEND_INTERVAL_SECS.store(
            secs.clamp(MIN_SEND_INTERVAL_SECS, MAX_SEND_INTERVAL_SECS),
            Ordering::Relaxed,
        );
    }
    load_known_sensors().await;
    spawner.spawn(temperature_task(BitBangBus::new(pin)).unwrap());
}

/// Re-registers the sensors stored in flash, so they're tracked (and
/// reported if they don't answer) from the first cycle on.
async fn load_known_sensors() {
    for slot in 0..MAX_SENSORS as u8 {
        let stored = config().await.get_array::<8>(SENSOR_KEY_BASE + slot).await;
        let Some(rom) = stored.and_then(|bytes| RomCode::new(bytes).ok()) else {
            continue;
        };
        if rom.is_ds18b20_compatible() {
            let _ = HEALTH.lock(|health| health.borrow_mut().register(rom));
        }
    }
    let known = HEALTH.lock(|health| health.borrow().len());
    if known > 0 {
        console_log!("temperature: {known} known sensor(s) from flash");
    }
}

/// Writes the current sensor list to flash (call after it changes). Slots
/// past the end are blanked.
async fn persist_known_sensors() {
    let roms: Vec<RomCode, MAX_SENSORS> = HEALTH.lock(|health| health.borrow().roms().collect());
    for slot in 0..MAX_SENSORS {
        let bytes = roms.get(slot).map_or([0u8; 8], |rom| *rom.as_bytes());
        let key = SENSOR_KEY_BASE + slot as u8;
        let mut config = config().await;
        // Skip the write when the slot already holds this value, so
        // unchanged slots cost no flash wear.
        if config.get_array::<8>(key).await.unwrap_or([0u8; 8]) != bytes
            && config.set_array(key, bytes).await.is_err()
        {
            console_log!("temperature: failed to persist sensor slot {slot}");
        }
    }
}

/// Open-drain bit-banged 1-Wire master on one GPIO.
struct BitBangBus {
    pin: Flex<'static>,
    delay: Delay,
}

impl BitBangBus {
    fn new(pin: impl Pin + 'static) -> Self {
        let mut pin = Flex::new(pin);
        // The board has an external pull-up; the internal one just keeps
        // the line defined if a sensor cable is pulled.
        pin.apply_output_config(
            &OutputConfig::default()
                .with_drive_mode(DriveMode::OpenDrain)
                .with_pull(Pull::Up),
        );
        pin.set_high();
        pin.set_input_enable(true);
        pin.set_output_enable(true);
        Self {
            pin,
            delay: Delay::new(),
        }
    }

    fn wait_us(&self, us: u32) {
        self.delay.delay_micros(us);
    }
}

impl OneWireBus for BitBangBus {
    fn reset(&mut self) -> bool {
        self.pin.set_low();
        self.wait_us(480);
        // Release and look for the presence pulse 15-60 µs later.
        let presence = critical_section::with(|_| {
            self.pin.set_high();
            self.wait_us(70);
            self.pin.is_low()
        });
        self.wait_us(410);
        presence
    }

    fn write_bit(&mut self, bit: bool) {
        // A "1" is a short low pulse, a "0" a long one; the slot (>= 60 µs)
        // is padded out after the line is released again.
        let low_us = if bit { 6 } else { 60 };
        critical_section::with(|_| {
            self.pin.set_low();
            self.wait_us(low_us);
            self.pin.set_high();
        });
        self.wait_us(if bit { 64 } else { 10 });
    }

    fn read_bit(&mut self) -> bool {
        // The sensor holds the line low for a "0" until 15 µs after the
        // falling edge, so sample just before that.
        let bit = critical_section::with(|_| {
            self.pin.set_low();
            self.wait_us(2);
            self.pin.set_high();
            self.wait_us(8);
            self.pin.is_high()
        });
        self.wait_us(55);
        bit
    }
}

#[embassy_executor::task]
async fn temperature_task(mut bus: BitBangBus) {
    let mut publish = PublishTimer::new(
        Duration::from_secs(u64::from(send_interval_secs())),
        SAMPLE_PERIOD / 2,
    );
    let mut cycle = 0u32;
    let mut failed_cycles = 0u32;
    let mut ticker = Ticker::every(SAMPLE_PERIOD);

    loop {
        let cycle_start = Instant::now();

        if FORGET_LOST.swap(false, Ordering::Relaxed) {
            forget_lost().await;
        }

        let known = HEALTH.lock(|health| health.borrow().len());
        if known == 0
            || cycle.is_multiple_of(RESCAN_EVERY_CYCLES)
            || failed_cycles >= FAILED_CYCLES_BEFORE_RESCAN
        {
            rescan(&mut bus).await;
            failed_cycles = 0;
        }
        cycle = cycle.wrapping_add(1);

        let devices: Vec<RomCode, MAX_SENSORS> =
            HEALTH.lock(|health| health.borrow().roms().collect());
        if !devices.is_empty() {
            let good = measure(&mut bus, &devices).await;
            failed_cycles = if good == 0 { failed_cycles + 1 } else { 0 };

            CYCLES.sender().send(cycle);

            publish.set_interval(Duration::from_secs(u64::from(send_interval_secs())));
            let now = Instant::now();
            if publish.due(now) {
                publish.mark_published(now);
                publish_fresh(cycle_start).await;
            }
        }

        ticker.next().await;
    }
}

async fn forget_lost() {
    let forgotten = HEALTH.lock(|health| health.borrow_mut().forget_lost());
    if forgotten > 0 {
        console_log!("temperature: forgot {forgotten} lost sensor(s)");
        persist_known_sensors().await;
    }
}

/// Searches the bus and starts tracking every sensor not seen before.
/// Sensors that were seen earlier but aren't found now are deliberately kept:
/// their failing measurements are what reports them lost.
async fn rescan(bus: &mut BitBangBus) {
    let mut search = RomSearch::new();
    let mut found_new = false;
    loop {
        match search.next(bus) {
            Ok(Some(rom)) if rom.is_ds18b20_compatible() => {
                match HEALTH.lock(|health| health.borrow_mut().register(rom)) {
                    Ok(true) => {
                        console_log!("temperature: new sensor {:02X?}", rom.as_bytes());
                        found_new = true;
                    }
                    Ok(false) => {}
                    Err(_) => {
                        console_log!(
                            "temperature: more than {MAX_SENSORS} sensors, ignoring the rest"
                        );
                        break;
                    }
                }
            }
            Ok(Some(rom)) => {
                console_log!(
                    "temperature: ignoring 1-wire device {:02X?}",
                    rom.as_bytes()
                );
            }
            Ok(None) => break,
            Err(e) => {
                console_log!("temperature: bus search failed: {e:?}");
                break;
            }
        }
        // One search step blocks for ~15 ms; let the other tasks run.
        yield_now().await;
    }
    if found_new {
        persist_known_sensors().await;
    }
}

/// Runs one conversion on all sensors, stores the results and updates every
/// sensor's health (announcing changes on CAN). Returns how many sensors
/// delivered a valid reading.
async fn measure(bus: &mut BitBangBus, devices: &[RomCode]) -> usize {
    let mut good = 0;

    if convert(bus).await {
        for rom in devices {
            let temperature = read_scratchpad(bus, rom);
            if let Some(temperature) = temperature {
                // A full bank can't happen: the bank never holds more
                // sensors than the health tracker, which has the same limit.
                let _ = BANK.lock(|bank| {
                    bank.borrow_mut().update(Reading {
                        rom: *rom,
                        temperature,
                        at: Instant::now(),
                    })
                });
                good += 1;
            } else {
                console_log!("temperature: bad reading from {:02X?}", rom.as_bytes());
            }
            record(rom, temperature.is_some()).await;
            yield_now().await;
        }
    } else {
        for rom in devices {
            record(rom, false).await;
        }
    }
    good
}

/// Starts a conversion on every sensor and waits for it to finish. Returns
/// false if the bus doesn't answer or the sensors never report done.
async fn convert(bus: &mut BitBangBus) -> bool {
    if !bus.reset() {
        console_log!("temperature: no presence pulse, is the bus connected?");
        return false;
    }
    bus.write_byte(command::SKIP_ROM);
    bus.write_byte(command::CONVERT_T);

    // A sensor answers read slots with 0 while it converts and 1 when done.
    let started = Instant::now();
    Timer::after_millis(100).await;
    while !bus.read_bit() {
        if started.elapsed() > CONVERSION_TIMEOUT {
            console_log!("temperature: conversion timed out");
            return false;
        }
        Timer::after_millis(25).await;
    }
    true
}

fn read_scratchpad(bus: &mut BitBangBus, rom: &RomCode) -> Option<Temperature> {
    if !bus.reset() {
        return None;
    }
    bus.write_byte(command::MATCH_ROM);
    for byte in rom.as_bytes() {
        bus.write_byte(*byte);
    }
    bus.write_byte(command::READ_SCRATCHPAD);
    let mut scratchpad = [0u8; 9];
    for byte in &mut scratchpad {
        *byte = bus.read_byte();
    }
    parse_scratchpad(&scratchpad).ok()
}

/// Feeds one sensor's cycle outcome into the health tracker and announces
/// what that changed.
async fn record(rom: &RomCode, ok: bool) {
    let event = HEALTH.lock(|health| health.borrow_mut().record(rom, ok));
    let Some(event) = event else { return };

    if event.kind == HealthEventKind::Lost {
        // Never leave a dead sensor's last value around looking current.
        BANK.lock(|bank| bank.borrow_mut().remove(rom));
    }
    announce(event).await;
}

/// Logs a health change and reports it on CAN as a `DeviceError`.
async fn announce(event: HealthEvent) {
    let (code, severity, what) = match event.kind {
        HealthEventKind::Lost => (ErrorCode::SensorLost, Severity::Error, "LOST"),
        HealthEventKind::StillLost => (
            ErrorCode::SensorLost,
            Severity::RepeatingError,
            "still lost",
        ),
        HealthEventKind::Recovered => (ErrorCode::SensorRecovered, Severity::Warning, "recovered"),
    };
    console_log!(
        "temperature: sensor #{} {:02X?} {what}",
        event.slot,
        event.rom.as_bytes()
    );

    let serial = event.rom.serial();
    report_error(
        Component::Sensor,
        code,
        severity,
        event.slot as u8,
        &serial[..4],
    )
    .await;
}

/// Sends the readings measured since `cycle_start`, one frame per sensor.
/// Sensors that failed this cycle are skipped rather than re-sent with an
/// old value.
async fn publish_fresh(cycle_start: Instant) {
    let fresh = snapshot();
    for reading in fresh.iter().filter(|r| r.at >= cycle_start) {
        let payload = can_payload(&reading.rom, reading.temperature);
        send_can_message(CanMessageType::TemperatureSensor, &payload, false).await;
    }
}
