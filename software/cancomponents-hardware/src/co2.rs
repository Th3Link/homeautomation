//! An optional Sensirion SCD30 on the sensor board (I2C 0x61): CO2 measured
//! by NDIR, sent as `Co2Sensor` (ppm) once per send interval. It is the
//! reference to calibrate the BME680's CO2 equivalent against (ADR 0024).
//!
//! The SCD30 measures on its own every 2 s; its CO2 is compensated with the
//! BME680's pressure ([`crate::environment::pressure_hpa`]), updated when
//! that moves by 10 hPa. Its automatic self-calibration is left as the
//! sensor has it stored (off from the factory).
//!
//! The SCD30 stretches the I2C clock; the ESP32's I2C allows up to about
//! 13 ms. Failed reads are logged once and retried.

use crate::can::send_can_message;
use crate::console_log;
use crate::environment::pressure_hpa;
use crate::sensors::Bus;
use crate::temperature::send_interval_secs;
use cancomponents_core::can_message_type::CanMessageType;
use cancomponents_core::sensor_board::{self, frame, scd30_sub_id};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_time::{Delay, Duration, Instant, Timer};
use libscd::asynchronous::scd30::{Scd30, I2C_ADDRESS};

/// The SCD30's own measurement interval.
const POLL: Duration = Duration::from_secs(2);
/// The pressure compensation is renewed when the pressure moved this far.
const PRESSURE_STEP_HPA: f32 = 10.0;
/// How long to wait for the BME680's first pressure at start.
const PRESSURE_WAIT: Duration = Duration::from_secs(30);

#[embassy_executor::task]
pub async fn co2_task(bus: &'static Bus) {
    let mut scd = Scd30::new(I2cDevice::new(bus), Delay);
    let Ok((major, minor)) = scd.read_firmware_version().await else {
        console_log!("sensors: no SCD30");
        return;
    };
    console_log!("sensors: SCD30 found (firmware {major}.{minor})");

    let start = Instant::now();
    while pressure_hpa().is_none() && start.elapsed() < PRESSURE_WAIT {
        Timer::after_secs(1).await;
    }
    let mut compensated = pressure_hpa();
    if start_measuring(&mut scd, compensated).await.is_err() {
        console_log!("sensors: SCD30 didn't start measuring");
        return;
    }

    let sub_id = scd30_sub_id(I2C_ADDRESS);
    let mut next_send = Instant::now();
    let mut failing = false;
    loop {
        Timer::after(POLL).await;
        let measurement = match scd.data_ready().await {
            Ok(true) => scd.read_measurement().await.ok(),
            Ok(false) => continue,
            Err(_) => None,
        };
        let Some(m) = measurement else {
            if !failing {
                console_log!("sensors: SCD30 read failed");
            }
            failing = true;
            continue;
        };
        failing = false;

        let now = Instant::now();
        if now >= next_send {
            next_send = now + Duration::from_secs(u64::from(send_interval_secs()));
            let raw = sensor_board::unsigned_raw(f32::from(m.co2));
            send_can_message(CanMessageType::Co2Sensor, &frame(sub_id, raw), false).await;
        }

        let pressure = pressure_hpa();
        let moved = match (pressure, compensated) {
            (Some(p), Some(c)) => (p - c).abs() >= PRESSURE_STEP_HPA,
            (Some(_), None) => true,
            _ => false,
        };
        if moved && start_measuring(&mut scd, pressure).await.is_ok() {
            compensated = pressure;
        }
    }
}

/// Starts (or renews) continuous measuring, compensated for `pressure`
/// (hPa) if known.
async fn start_measuring<I: embedded_hal_async::i2c::I2c>(
    scd: &mut Scd30<I, Delay>,
    pressure: Option<f32>,
) -> Result<(), ()> {
    // 0 switches the compensation off; the SCD30 takes 700-1400 hPa.
    let hpa = pressure.map_or(0, |p| (p + 0.5).clamp(700.0, 1400.0) as u16);
    scd.start_continuous_measurement(hpa).await.map_err(|_| ())
}
