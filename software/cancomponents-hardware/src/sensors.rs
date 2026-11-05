//! `ExtensionMode::Sensors`: the light switch's sensor extension board.
//!
//! - **PIR** (active high): `PirSensor` once a second while there is motion,
//!   then one release ([`cancomponents_core::presence`]).
//! - **VEML7700** ambient light (I2C 0x10): `AmbientLightSensor` in lux, on a
//!   change of 10 % or once per send interval
//!   ([`cancomponents_core::ambient_light`]).
//! - **BME680** (I2C 0x76 or 0x77): temperature, humidity, pressure and the
//!   air quality once per send interval ([`crate::environment`]).
//! - **SCD30** (I2C 0x61, optional): measured CO2 once per send interval
//!   ([`crate::co2`]).
//! - **Night light** (PCF8574A at 0x38): the `Nightlight` message, 0-3 LEDs.
//!
//! Each part is looked for once at start; one that doesn't answer is
//! skipped (and logged), so a board with fewer parts works too. The I2C
//! bus is shared between them.

use crate::can::send_can_message;
use crate::console_log;
use crate::temperature::send_interval_secs;
use cancomponents_core::ambient_light::{self, Range, Reporter};
use cancomponents_core::can_message_type::CanMessageType;
use cancomponents_core::presence::PresenceFsm;
use cancomponents_core::sensor_board;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Instant, Ticker, Timer};
use embedded_hal_async::i2c::I2c as _;
use esp_hal::gpio::{AnyPin, Input, InputConfig, Pull};

pub(crate) use crate::extension::{Bus, Device};

/// Whether the night light answered at start.
static NIGHTLIGHT: AtomicBool = AtomicBool::new(false);
/// Night light levels from CAN, for [`nightlight_task`].
static NIGHTLIGHT_LEVEL: Channel<CriticalSectionRawMutex, u8, 2> = Channel::new();

pub struct Sensors;

impl Sensors {
    pub async fn init(
        bus: &'static Bus,
        _onewire: AnyPin<'static>,
        pir: AnyPin<'static>,
        spawner: &Spawner,
    ) {
        crate::temperature::load_send_interval().await;
        scan(bus).await;

        let pir = Input::new(pir, InputConfig::default().with_pull(Pull::Down));
        spawner.spawn(pir_task(pir).unwrap());
        spawner.spawn(ambient_light_task(I2cDevice::new(bus)).unwrap());
        spawner.spawn(crate::environment::environment_task(bus).unwrap());
        spawner.spawn(crate::co2::co2_task(bus).unwrap());

        let mut nightlight = I2cDevice::new(bus);
        let found = nightlight
            .write(
                sensor_board::NIGHTLIGHT_ADDRESS,
                &[sensor_board::nightlight_port(0)],
            )
            .await
            .is_ok();
        NIGHTLIGHT.store(found, Ordering::Relaxed);
        console_log!(
            "sensors: night light {}",
            if found { "found" } else { "not found" }
        );
        if found {
            spawner.spawn(nightlight_task(nightlight).unwrap());
        }
    }
}

/// Logs every address on the bus that answers, for bring-up.
async fn scan(bus: &'static Bus) {
    let mut dev = I2cDevice::new(bus);
    let mut found: heapless::String<128> = heapless::String::new();
    for address in 0x08..0x78 {
        if dev.write(address, &[]).await.is_ok() {
            let _ = core::fmt::Write::write_fmt(&mut found, format_args!(" 0x{address:02x}"));
        }
    }
    console_log!("sensors: I2C devices:{found}");
}

/// Handles `Nightlight`: `[level]`, 0 = off, 1-3 = that many LEDs.
pub async fn nightlight(data: &[u8], remote: bool) {
    let (Some(&level), false) = (data.first(), remote) else {
        return;
    };
    if NIGHTLIGHT.load(Ordering::Relaxed) {
        // A newer level replaces one still queued.
        if NIGHTLIGHT_LEVEL.try_send(level).is_err() {
            NIGHTLIGHT_LEVEL.clear();
            let _ = NIGHTLIGHT_LEVEL.try_send(level);
        }
    }
}

#[embassy_executor::task]
async fn nightlight_task(mut dev: Device) {
    loop {
        let port = sensor_board::nightlight_port(NIGHTLIGHT_LEVEL.receive().await);
        if dev
            .write(sensor_board::NIGHTLIGHT_ADDRESS, &[port])
            .await
            .is_err()
        {
            console_log!("sensors: night light didn't answer");
        }
    }
}

/// How often the PIR's output is looked at.
const PIR_POLL: Duration = Duration::from_millis(50);

#[embassy_executor::task]
async fn pir_task(pir: Input<'static>) {
    let mut fsm = PresenceFsm::new();
    let polls = (Duration::from_secs(1).as_millis() / PIR_POLL.as_millis()) as u32;
    let mut ticker = Ticker::every(PIR_POLL);
    loop {
        // Motion at any time during the second counts.
        let mut motion = false;
        for _ in 0..polls {
            ticker.next().await;
            motion |= pir.is_high();
        }
        if let Some(msg) = fsm.tick(motion) {
            send_can_message(CanMessageType::PirSensor, &msg.to_bytes(), false).await;
        }
    }
}

/// How often the light is measured.
const LIGHT_PERIOD: Duration = Duration::from_secs(2);

#[embassy_executor::task]
async fn ambient_light_task(mut dev: Device) {
    let mut range = Range::Dim;
    if set_range(&mut dev, range).await.is_err() {
        console_log!("sensors: no VEML7700 ambient light sensor");
        return;
    }
    console_log!("sensors: VEML7700 ambient light sensor found");
    let mut reporter = Reporter::new();
    let mut ticker = Ticker::every(LIGHT_PERIOD);
    loop {
        ticker.next().await;
        let mut counts = [0u8; 2];
        if dev
            .write_read(
                ambient_light::ADDRESS,
                &[ambient_light::REG_ALS],
                &mut counts,
            )
            .await
            .is_err()
        {
            continue;
        }
        let counts = u16::from_le_bytes(counts);
        let next = range.next(counts);
        if next != range {
            // The reading is clipped (or too coarse): measure again in the
            // other range.
            range = next;
            let _ = set_range(&mut dev, range).await;
            continue;
        }
        let lux = range.lux(counts);
        let now = Instant::now().as_secs();
        if reporter.report(lux, now, u64::from(send_interval_secs())) {
            send_can_message(
                CanMessageType::AmbientLightSensor,
                &lux.to_le_bytes(),
                false,
            )
            .await;
        }
    }
}

/// Switches the VEML7700 to `range` (power saving off) and waits until a
/// measurement in it is ready.
async fn set_range(dev: &mut Device, range: Range) -> Result<(), ()> {
    let config = range.config().to_le_bytes();
    dev.write(
        ambient_light::ADDRESS,
        &[ambient_light::REG_CONF, config[0], config[1]],
    )
    .await
    .map_err(|_| ())?;
    dev.write(
        ambient_light::ADDRESS,
        &[ambient_light::REG_POWER_SAVING, 0, 0],
    )
    .await
    .map_err(|_| ())?;
    Timer::after_millis(u64::from(2 * range.integration_ms())).await;
    Ok(())
}
