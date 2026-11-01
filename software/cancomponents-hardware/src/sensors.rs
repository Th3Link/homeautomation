//! `ExtensionMode::Sensors`: the light switch's sensor extension board.
//!
//! - **PIR** (active high): `PirSensor` once a second while there is motion,
//!   then one release ([`cancomponents_core::presence`]).
//! - **VEML7700** ambient light (I2C 0x10): `AmbientLightSensor` in lux, on a
//!   change of 10 % or once per send interval
//!   ([`cancomponents_core::ambient_light`]).
//! - **BME680** (I2C 0x76 or 0x77): temperature, humidity and pressure, and
//!   from its gas sensor through Bosch's BSEC ([`crate::bsec`]) a CO₂
//!   equivalent, a breath VOC equivalent and an air quality index, once per
//!   send interval ([`cancomponents_core::sensor_board`]). BSEC has the
//!   sensor measure every 3 s; its outputs go out once they have an accuracy
//!   of at least 1, and its calibration is saved every few hours. Without
//!   BSEC (it doesn't start) the gas sensor stays off.
//! - **Night light** (PCF8574A at 0x38): the `Nightlight` message, 0-3 LEDs.
//!
//! Each part is looked for once at start; one that doesn't answer is
//! skipped (and logged), so a board with fewer parts works too. The I2C
//! bus is shared between them.

use crate::bsec::{self, Bsec};
use crate::can::send_can_message;
use crate::config::{self, config};
use crate::console_log;
use crate::temperature::send_interval_secs;
use bosch_bme680::{
    AsyncBme680, Configuration, DeviceAddress, GasConfig, IIRFilter, MeasurmentData, Oversampling,
};
use cancomponents_core::ambient_light::{self, Range, Reporter};
use cancomponents_core::can_message_type::CanMessageType;
use cancomponents_core::presence::PresenceFsm;
use cancomponents_core::sensor_board::{self, bme680_sub_id, frame, HeaterCalibration};
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Delay, Duration, Instant, Ticker, Timer};
use embedded_hal_async::i2c::I2c as _;
use esp_hal::gpio::{AnyPin, Input, InputConfig, Pull};
use esp_hal::i2c::master::I2c;
use esp_hal::Async;
use static_cell::StaticCell;

type Bus = Mutex<CriticalSectionRawMutex, I2c<'static, Async>>;
type Device = I2cDevice<'static, CriticalSectionRawMutex, I2c<'static, Async>>;
type Bme680 = AsyncBme680<Device, Delay>;

static BUS: StaticCell<Bus> = StaticCell::new();
/// Whether the night light answered at start.
static NIGHTLIGHT: AtomicBool = AtomicBool::new(false);
/// Night light levels from CAN, for [`nightlight_task`].
static NIGHTLIGHT_LEVEL: Channel<CriticalSectionRawMutex, u8, 2> = Channel::new();

pub struct Sensors;

impl Sensors {
    pub async fn init(
        i2c: I2c<'static, Async>,
        _onewire: AnyPin<'static>,
        pir: AnyPin<'static>,
        spawner: &Spawner,
    ) {
        crate::temperature::load_send_interval().await;
        let bus: &'static Bus = BUS.init(Mutex::new(i2c));
        scan(bus).await;

        let pir = Input::new(pir, InputConfig::default().with_pull(Pull::Down));
        spawner.spawn(pir_task(pir).unwrap());
        spawner.spawn(ambient_light_task(I2cDevice::new(bus)).unwrap());
        spawner.spawn(environment_task(I2cDevice::new(bus), I2cDevice::new(bus)).unwrap());

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

/// The BME680's settings: the oversampling of temperature, pressure and
/// humidity (BSEC's encoding, 0-5) and whether the gas sensor runs. Its
/// heater's set point is written separately before each measurement.
fn bme680_config(oversampling: [u8; 3], gas: bool) -> Configuration {
    let [t, p, h] = oversampling;
    Configuration::builder()
        .temperature_oversampling(Oversampling::from(t))
        .pressure_oversampling(Oversampling::from(p))
        .humidity_oversampling(Oversampling::from(h))
        // BSEC takes the readings unfiltered.
        .filter(if gas {
            IIRFilter::Coeff0
        } else {
            IIRFilter::Coeff3
        })
        .gas_config(gas.then(GasConfig::default))
        .build()
}

/// The oversampling without BSEC: temperature ×2, pressure ×16, humidity ×1.
const PLAIN_OVERSAMPLING: [u8; 3] = [2, 5, 1];

/// `registers` is a second handle on the bus, for the heater's registers,
/// which the driver only sets to its fixed 300 °C.
#[embassy_executor::task]
async fn environment_task(dev: Device, registers: Device) {
    let config = bme680_config(PLAIN_OVERSAMPLING, false);
    let mut dev = Some(dev);
    let mut found = None;
    for address in [0x77, 0x76] {
        let Some(i2c) = dev.take() else { break };
        let device_address = if address == 0x77 {
            DeviceAddress::Secondary
        } else {
            DeviceAddress::Primary
        };
        let mut sensor = AsyncBme680::new(i2c, device_address, Delay, 20);
        if sensor.initialize(&config).await.is_ok() {
            found = Some((sensor, address));
            break;
        }
        dev = Some(sensor.into_inner());
    }
    let Some((sensor, address)) = found else {
        console_log!("sensors: no BME680");
        return;
    };
    console_log!("sensors: BME680 found at 0x{address:02x}");

    match Bsec::new() {
        Some(Ok(bsec)) => {
            let [major, minor, major_bugfix, minor_bugfix] = bsec.version();
            console_log!("bsec: {major}.{minor}.{major_bugfix}.{minor_bugfix}");
            air_quality(sensor, registers, address, bsec).await
        }
        Some(Err(status)) => {
            console_log!("bsec: doesn't start ({}), no gas sensor", status.0);
            environment(sensor, address).await
        }
        None => {}
    }
}

/// Temperature, humidity and pressure without BSEC, once per send interval.
async fn environment(mut sensor: Bme680, address: u8) {
    let sub_id = bme680_sub_id(address);
    Timer::after_secs(5).await;
    loop {
        match sensor.measure().await {
            Ok(m) => send_environment(sub_id, &m).await,
            Err(_) => console_log!("sensors: BME680 measurement failed"),
        }
        Timer::after(Duration::from_secs(u64::from(send_interval_secs()))).await;
    }
}

async fn send_environment(sub_id: u64, m: &MeasurmentData) {
    let readings = [
        (
            CanMessageType::TemperatureSensor,
            sensor_board::temperature_raw(m.temperature),
        ),
        (
            CanMessageType::HumiditySensor,
            sensor_board::unsigned_raw(m.humidity),
        ),
        (
            CanMessageType::PressureSensor,
            // In Pa, though the driver's docs say hPa.
            sensor_board::unsigned_raw(m.pressure / 100.0),
        ),
    ];
    for (kind, raw) in readings {
        send_can_message(kind, &frame(sub_id, raw), false).await;
    }
}

/// How often BSEC's calibration is saved: it takes days to build, and
/// after a restart without it BSEC starts over.
const STATE_SAVE: Duration = Duration::from_secs(4 * 3600);

/// BSEC's latest outputs (IAQ, CO₂ and breath VOC equivalent) and their
/// accuracy.
#[derive(Default)]
struct AirQuality {
    iaq: Option<(f32, u8)>,
    co2: Option<(f32, u8)>,
    voc: Option<(f32, u8)>,
}

/// The BME680 measuring as BSEC asks, its readings into BSEC, and both
/// sent once per send interval.
async fn air_quality(mut sensor: Bme680, mut registers: Device, address: u8, mut bsec: Bsec) {
    let sub_id = bme680_sub_id(address);
    let heater = match sensor.get_calibration_data() {
        Ok(c) => HeaterCalibration {
            par_gh1: c.par_gh1,
            par_gh2: c.par_gh2,
            par_gh3: c.par_gh3,
            res_heat_range: c.res_heat_range,
            res_heat_val: c.res_heat_val,
        },
        Err(_) => return,
    };
    load_state(&mut bsec).await;

    let mut oversampling = None;
    let mut ambient = 20;
    let mut environment: Option<MeasurmentData> = None;
    let mut air = AirQuality::default();
    let mut last_status = 0;
    let mut logged_settings = false;
    let mut next_send = Instant::now() + Duration::from_secs(5);
    let mut next_save = Instant::now() + STATE_SAVE;
    loop {
        let now = Instant::now();
        let time_stamp = now.as_micros() as i64 * 1000;
        let (settings, status) = bsec.sensor_control(time_stamp);
        log_status(&mut last_status, "sensor_control", status);

        if settings.trigger_measurement != 0 {
            if !logged_settings {
                logged_settings = true;
                console_log!(
                    "bsec: heater {} °C for {} ms, every {} ms",
                    settings.heater_temperature,
                    settings.heater_duration,
                    (settings.next_call - time_stamp) / 1_000_000
                );
            }
            // At least the oversampling without BSEC: it skips what it
            // doesn't need (pressure), and the gateway gets that too.
            let wanted = [
                settings.temperature_oversampling,
                settings.pressure_oversampling,
                settings.humidity_oversampling,
            ];
            let wanted: [u8; 3] = core::array::from_fn(|i| wanted[i].max(PLAIN_OVERSAMPLING[i]));
            if oversampling != Some(wanted)
                && sensor
                    .set_configuration(&bme680_config(wanted, true))
                    .await
                    .is_ok()
            {
                oversampling = Some(wanted);
            }
            let heater_ok = registers
                .write(
                    address,
                    &[
                        sensor_board::RES_HEAT_0,
                        heater.res_heat(settings.heater_temperature, ambient),
                    ],
                )
                .await
                .is_ok()
                && registers
                    .write(
                        address,
                        &[
                            sensor_board::GAS_WAIT_0,
                            sensor_board::gas_wait(settings.heater_duration),
                        ],
                    )
                    .await
                    .is_ok();
            match sensor.measure().await {
                Ok(m) if heater_ok => {
                    ambient = m.temperature as i32;
                    let inputs = inputs(&settings, time_stamp, &m);
                    match bsec.do_steps(&inputs) {
                        Ok(outputs) => {
                            for o in outputs {
                                air.take(o, &mut next_save);
                            }
                        }
                        Err(status) => log_status(&mut last_status, "do_steps", Err(status)),
                    }
                    environment = Some(m);
                }
                _ => console_log!("sensors: BME680 measurement failed"),
            }
        }

        if Instant::now() >= next_send {
            next_send = Instant::now() + Duration::from_secs(u64::from(send_interval_secs()));
            if let Some(m) = &environment {
                send_environment(sub_id, m).await;
            }
            air.send(sub_id).await;
        }
        if Instant::now() >= next_save {
            next_save = Instant::now() + STATE_SAVE;
            save_state(&mut bsec).await;
        }

        let next = Instant::from_micros((settings.next_call / 1000).max(0) as u64);
        // A broken schedule mustn't spin.
        Timer::at(next.max(Instant::now() + Duration::from_millis(100))).await;
    }
}

/// BSEC's inputs from one measurement, those it asked for.
fn inputs(
    settings: &bsec::BmeSettings,
    time_stamp: i64,
    m: &MeasurmentData,
) -> heapless::Vec<bsec::Input, 5> {
    let mut inputs = heapless::Vec::new();
    let mut push = |id, value| {
        if settings.wants(id) {
            let _ = inputs.push(bsec::Input::new(time_stamp, id, value));
        }
    };
    push(bsec::input::TEMPERATURE, m.temperature);
    push(bsec::input::HUMIDITY, m.humidity);
    // In Pa, as BSEC wants it.
    push(bsec::input::PRESSURE, m.pressure);
    if let Some(gas) = m.gas_resistance {
        push(bsec::input::GAS_RESISTOR, gas);
        // Forced mode: always the profile's first (and only) step.
        push(bsec::input::PROFILE_PART, 0.0);
    }
    inputs
}

impl AirQuality {
    /// Keeps one of BSEC's outputs. The calibration is saved when the IAQ
    /// first becomes fully accurate.
    fn take(&mut self, o: bsec::Output, next_save: &mut Instant) {
        let value = Some((o.signal, o.accuracy));
        match o.sensor_id {
            bsec::output::IAQ => {
                let before = self.iaq.map(|(_, accuracy)| accuracy);
                if before != Some(o.accuracy) {
                    console_log!("bsec: IAQ accuracy {}", o.accuracy);
                    if o.accuracy == 3 {
                        // Saved on the next pass of the loop.
                        *next_save = Instant::now();
                    }
                }
                self.iaq = value;
            }
            bsec::output::CO2_EQUIVALENT => self.co2 = value,
            bsec::output::BREATH_VOC_EQUIVALENT => self.voc = value,
            _ => {}
        }
    }

    /// Sends the outputs with an accuracy of at least 1 (before that, BSEC
    /// reports its fixed start values).
    async fn send(&self, sub_id: u64) {
        let outputs = [
            (CanMessageType::Co2Equivalent, self.co2),
            (CanMessageType::VocBreath, self.voc),
            (CanMessageType::AirQuality, self.iaq),
        ];
        for (kind, output) in outputs {
            if let Some((value, accuracy)) = output {
                if accuracy >= 1 {
                    let raw = sensor_board::unsigned_raw(value);
                    send_can_message(kind, &frame(sub_id, raw), false).await;
                }
            }
        }
    }
}

/// Logs a status of BSEC's other than the last one logged.
fn log_status(last: &mut i32, what: &str, status: Result<(), bsec::Status>) {
    let code = status.err().map_or(0, |s| s.0);
    if code != *last {
        *last = code;
        if code != 0 {
            console_log!("bsec: {what}: {code}");
        }
    }
}

async fn load_state(bsec: &mut Bsec) {
    let mut state = [0u8; bsec::STATE_BYTES];
    let n = config()
        .await
        .read_bytes(config::Key::BsecState, &mut state)
        .await;
    match n {
        Some(n) => match bsec.set_state(&state[..n]) {
            Ok(()) => console_log!("bsec: calibration restored"),
            Err(status) => console_log!("bsec: saved calibration refused ({})", status.0),
        },
        None => console_log!("bsec: no saved calibration, starting over"),
    }
}

async fn save_state(bsec: &mut Bsec) {
    let mut state = [0u8; bsec::STATE_BYTES];
    match bsec.state(&mut state) {
        Ok(n) => {
            let stored = config()
                .await
                .set_bytes(config::Key::BsecState, &state[..n])
                .await;
            if stored.is_err() {
                console_log!("bsec: couldn't save the calibration");
            }
        }
        Err(status) => console_log!("bsec: no state ({})", status.0),
    }
}
