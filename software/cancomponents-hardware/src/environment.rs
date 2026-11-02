//! The sensor board's BME680: temperature, humidity and pressure, and from
//! its gas sensor the air quality, with our own open calculation
//! ([`bme68x_iaq_co2`], ADR 0024).
//!
//! Every 3 s the sensor measures once (forced mode), the heater at 320 °C for
//! 150 ms: the gas resistance depends on how long the heater rested, so the
//! cycle stays constant. Each measurement goes into the air quality
//! calculation; once per send interval go out `TemperatureSensor`,
//! `HumiditySensor`, `PressureSensor` and, once the calculation is past its
//! warm-up, `AirQuality` (index 0–500), `Co2Equivalent` and `VocBreath`
//! (ppm, estimates from VOCs, not measurements), and the last heat-stable
//! raw gas resistance as `GasResistance` (kΩ), to calibrate the calculation
//! against a real CO2 sensor ([`crate::co2`]).
//!
//! The calibration (the gas ceiling) is kept in flash every 4 hours and when
//! it first becomes highly accurate, and restored at start. If the heater
//! can't be set up, the sensor measures without gas as before.

use crate::can::send_can_message;
use crate::config::{self, config};
use crate::console_log;
use crate::sensors::{Bus, Device};
use crate::temperature::send_interval_secs;
use bme68x::asynch::{Bme68x, I2cAddress, I2cInterface};
use bme68x::{
    Configuration, Filter, HeaterConfiguration, Measurement, OperationMode, Oversampling,
    StandbyTime,
};
use bme68x_iaq_co2::{
    Accuracy, Air, AirQuality, Config as IaqConfig, Sample, Stage, CYCLE_MS, HEATER_DURATION_MS,
    HEATER_TEMPERATURE_C,
};
use cancomponents_core::can_message_type::CanMessageType;
use cancomponents_core::sensor_board::{self, bme680_sub_id, frame};
use core::sync::atomic::{AtomicU32, Ordering};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_time::{Delay, Duration, Instant, Ticker, Timer};
use embedded_hal_async::i2c::I2c as _;

type Sensor = Bme68x<I2cInterface<Device>, Delay>;
/// 100 values in the gas ceiling, the crate's default.
type Iaq = AirQuality<100>;

/// BME68x chip id register and value.
const CHIP_ID_REGISTER: u8 = 0xD0;
const CHIP_ID: u8 = 0x61;

const MEASUREMENT: Configuration = Configuration {
    humidity_oversampling: Oversampling::X1,
    temperature_oversampling: Oversampling::X2,
    pressure_oversampling: Oversampling::X16,
    filter: Filter::Size3,
    // Unused in forced mode; `None` would set bit 7 of ctrl_gas_1, which is
    // reserved on the BME680 (ODR20 on the BME688).
    standby_time: StandbyTime::Millis0_59,
};

/// The last pressure, hPa × 100 (0 = none yet), for the SCD30's pressure
/// compensation.
static PRESSURE: AtomicU32 = AtomicU32::new(0);

/// The last pressure the BME680 measured, in hPa.
pub fn pressure_hpa() -> Option<f32> {
    let pa = PRESSURE.load(Ordering::Relaxed);
    (pa != 0).then(|| pa as f32 / 100.0)
}

/// How often the calibration is saved (besides when it becomes accurate).
const SAVE_EVERY: Duration = Duration::from_secs(4 * 3600);
/// Measurements without a heat-stable gas reading after which the gas
/// sensor is reported as not working (5 minutes).
const GAS_SILENCE: u32 = 100;
/// The heater's set point is recomputed when the sensor's temperature moved
/// this far (K) from the one it was computed for.
const AMBIENT_STEP: i32 = 3;

#[embassy_executor::task]
pub async fn environment_task(bus: &'static Bus) {
    let Some((mut sensor, address)) = find(bus).await else {
        console_log!("sensors: no BME680");
        return;
    };
    console_log!("sensors: BME680 found at 0x{address:02x}");
    let sub_id = bme680_sub_id(address);
    let gas = match set_up(&mut sensor, true).await {
        Ok(()) => true,
        Err(()) => {
            console_log!("sensors: BME680 gas heater not set up, measuring without gas");
            set_up(&mut sensor, false).await.is_ok()
        }
    };

    let mut iaq = Iaq::new(IaqConfig::default());
    if gas {
        load_state(&mut iaq).await;
    }
    let mut ambient = 25;
    let mut shown = (Stage::Init, Accuracy::Unreliable);
    let mut failing = false;
    let mut without_gas = 0u32;
    let mut last_gas_ohm = None;
    let mut next_send = Instant::now();
    let mut next_save = Instant::now() + SAVE_EVERY;
    let mut ticker = Ticker::every(Duration::from_millis(u64::from(CYCLE_MS)));
    loop {
        ticker.next().await;
        let Some(m) = measure(&mut sensor).await else {
            if !failing {
                console_log!("sensors: BME680 measurement failed");
            }
            failing = true;
            continue;
        };
        failing = false;
        PRESSURE.store(m.values.pressure, Ordering::Relaxed);
        if gas && m.status.gas_valid() && m.status.heater_stable() {
            last_gas_ohm = Some(m.values.gas_resistance);
        }
        if gas && !(m.status.gas_valid() && m.status.heater_stable()) {
            without_gas += 1;
            if without_gas == GAS_SILENCE {
                // Seen on a sensor failing Bosch's self-test with
                // `GasResponseTooSmall`: the gas reading never changes.
                console_log!(
                    "sensors: BME680 gas sensor: no heat-stable reading in 5 minutes, \
                     no air quality (defective?)"
                );
            }
        } else {
            without_gas = 0;
        }
        let out = iaq.update(&Sample::from_bme68x(&m, Instant::now().as_millis()));
        let now = Instant::now();

        if (out.air.stage, out.air.accuracy) != shown {
            shown = (out.air.stage, out.air.accuracy);
            log_air(&out.air);
            if out.air.accuracy == Accuracy::High {
                next_save = now;
            }
        }
        if now >= next_save && out.air.ceiling_ohm > 0.0 {
            save_state(&iaq).await;
            next_save = now + SAVE_EVERY;
        }
        // The heater's resistance for 320 °C depends on the temperature
        // around it.
        let temperature = i32::from(m.values.temperature) / 100;
        if gas && (temperature - ambient).abs() >= AMBIENT_STEP {
            ambient = temperature;
            sensor.set_ambient_temperature(ambient.clamp(-40, 85) as i8);
            if set_up(&mut sensor, true).await.is_err() {
                console_log!("sensors: BME680 heater not updated");
            }
        }

        if now >= next_send {
            next_send = now + Duration::from_secs(u64::from(send_interval_secs()));
            let pressure_hpa = m.values.pressure as f32 / 100.0;
            let mut readings: heapless::Vec<(CanMessageType, u16), 7> = heapless::Vec::new();
            readings.extend([
                (
                    CanMessageType::TemperatureSensor,
                    sensor_board::temperature_raw(out.climate.temperature),
                ),
                (
                    CanMessageType::HumiditySensor,
                    sensor_board::unsigned_raw(out.climate.humidity),
                ),
                (
                    CanMessageType::PressureSensor,
                    sensor_board::unsigned_raw(pressure_hpa),
                ),
            ]);
            // During the burn-in the ceiling is still rising with the
            // sensor, so every reading looks like clean air.
            if out.air.stage == Stage::Operation {
                let e = out.air.equivalents;
                for (kind, value) in [
                    (CanMessageType::AirQuality, e.index),
                    (CanMessageType::Co2Equivalent, e.eco2_ppm),
                    (CanMessageType::VocBreath, e.bvoc_ppm),
                ] {
                    let _ = readings.push((kind, sensor_board::unsigned_raw(value)));
                }
            }
            if let Some(ohm) = last_gas_ohm.take() {
                let _ = readings.push((
                    CanMessageType::GasResistance,
                    sensor_board::unsigned_raw(ohm as f32 / 1000.0),
                ));
            }
            for (kind, raw) in readings {
                send_can_message(kind, &frame(sub_id, raw), false).await;
            }
        }
    }
}

/// The BME680 at 0x77 or 0x76.
async fn find(bus: &'static Bus) -> Option<(Sensor, u8)> {
    for (address, select) in [(0x77, I2cAddress::High), (0x76, I2cAddress::Low)] {
        let mut dev = I2cDevice::new(bus);
        let mut id = [0];
        let answered = dev
            .write_read(address, &[CHIP_ID_REGISTER], &mut id)
            .await
            .is_ok();
        if answered && id[0] == CHIP_ID {
            if let Ok(sensor) = Bme68x::new(I2cInterface::new(dev, select), Delay).await {
                return Some((sensor, address));
            }
        }
    }
    None
}

/// Oversampling and filter, and the heater on (`gas`) or off.
async fn set_up(sensor: &mut Sensor, gas: bool) -> Result<(), ()> {
    sensor
        .set_configuration(&MEASUREMENT)
        .await
        .map_err(|_| ())?;
    sensor
        .set_heater_configuration(&HeaterConfiguration::Forced {
            enabled: gas,
            temperature_celsius: HEATER_TEMPERATURE_C,
            duration_ms: HEATER_DURATION_MS,
        })
        .await
        .map_err(|_| ())
}

/// One forced measurement.
async fn measure(sensor: &mut Sensor) -> Option<Measurement> {
    sensor
        .set_operation_mode(OperationMode::Forced)
        .await
        .ok()?;
    let conversion = Sensor::measurement_duration(OperationMode::Forced, &MEASUREMENT);
    Timer::after_micros(u64::from(conversion) + u64::from(HEATER_DURATION_MS) * 1000).await;
    let measurements = sensor.measurements(OperationMode::Forced).await.ok()?;
    measurements.as_slice().first().copied()
}

fn log_air(air: &Air) {
    let stage = match air.stage {
        Stage::Init => "warming up",
        Stage::BurnIn => "burn-in",
        Stage::Operation => "calibrated",
    };
    console_log!(
        "air quality: {stage}, accuracy {}, ceiling {} kOhm",
        air.accuracy as u8,
        (air.ceiling_ohm / 1000.0) as u32
    );
}

async fn load_state(iaq: &mut Iaq) {
    let mut state = [0u8; Iaq::STATE_LEN];
    let n = config()
        .await
        .read_bytes(config::Key::AirQualityState, &mut state)
        .await;
    match n.map(|n| iaq.restore(&state[..n])) {
        Some(Ok(())) => console_log!("air quality: calibration restored"),
        Some(Err(e)) => console_log!("air quality: saved calibration refused ({e})"),
        None => console_log!("air quality: no saved calibration, starting over"),
    }
}

async fn save_state(iaq: &Iaq) {
    let mut state = [0u8; Iaq::STATE_LEN];
    let Ok(n) = iaq.save(&mut state) else {
        return;
    };
    let stored = config()
        .await
        .set_bytes(config::Key::AirQualityState, &state[..n])
        .await;
    if stored.is_err() {
        console_log!("air quality: couldn't save the calibration");
    }
}
