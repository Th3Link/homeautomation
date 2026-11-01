//! Bosch's BSEC 2 (`bsec2/libalgobsec.a`, closed source): turns the BME680's
//! readings and its gas sensor's resistance into an indoor air quality index
//! (IAQ), a CO₂ equivalent and a breath VOC equivalent.
//!
//! The library decides when and how the sensor measures
//! ([`Bsec::sensor_control`]): in low power mode every 3 s, with the heater
//! temperature and duration it asks for. Its outputs are estimates from the gas sensor: the CO₂
//! equivalent follows the VOCs people breathe out, it isn't a CO₂
//! measurement. Each output comes with an accuracy (0-3); it calibrates
//! itself over days, which [`Bsec::state`] keeps across restarts.
//!
//! The library has one global instance: [`Bsec::new`] hands it out once.

use core::sync::atomic::{AtomicBool, Ordering};
use heapless::Vec;
use static_cell::ConstStaticCell;

/// `bsec_sensor_configuration_t`.
#[repr(C)]
#[derive(Clone, Copy)]
struct SensorConfiguration {
    sample_rate: f32,
    sensor_id: u8,
}

/// `bsec_input_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Input {
    time_stamp: i64,
    signal: f32,
    signal_dimensions: u8,
    sensor_id: u8,
}

impl Input {
    pub fn new(time_stamp: i64, sensor_id: u8, signal: f32) -> Self {
        Self {
            time_stamp,
            signal,
            signal_dimensions: 1,
            sensor_id,
        }
    }
}

/// `bsec_output_t`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Output {
    pub time_stamp: i64,
    pub signal: f32,
    signal_dimensions: u8,
    pub sensor_id: u8,
    pub accuracy: u8,
}

/// `bsec_bme_settings_t`: what the sensor has to do next.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BmeSettings {
    /// When to call [`Bsec::sensor_control`] again [ns].
    pub next_call: i64,
    /// Which [`input`] ids [`Bsec::do_steps`] wants, as bits `1 << (id - 1)`.
    pub process_data: u32,
    pub heater_temperature: u16,
    pub heater_duration: u16,
    heater_temperature_profile: [u16; 10],
    heater_duration_profile: [u16; 10],
    heater_profile_len: u8,
    pub run_gas: u8,
    pub pressure_oversampling: u8,
    pub temperature_oversampling: u8,
    pub humidity_oversampling: u8,
    pub trigger_measurement: u8,
    op_mode: u8,
}

impl BmeSettings {
    /// Whether [`Bsec::do_steps`] wants input `id`.
    pub fn wants(&self, id: u8) -> bool {
        self.process_data & (1 << (id - 1)) != 0
    }
}

/// `bsec_version_t`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Version {
    major: u8,
    minor: u8,
    major_bugfix: u8,
    minor_bugfix: u8,
}

/// `bsec_physical_sensor_t`: the inputs.
pub mod input {
    pub const PRESSURE: u8 = 1;
    pub const HUMIDITY: u8 = 2;
    pub const TEMPERATURE: u8 = 3;
    pub const GAS_RESISTOR: u8 = 4;
    pub const PROFILE_PART: u8 = 24;
}

/// `bsec_virtual_sensor_t`: the outputs the gateway gets.
pub mod output {
    pub const IAQ: u8 = 1;
    pub const CO2_EQUIVALENT: u8 = 3;
    pub const BREATH_VOC_EQUIVALENT: u8 = 4;
}

/// `BSEC_SAMPLE_RATE_LP`: every 3 s.
const SAMPLE_RATE_LP: f32 = 0.33333;
/// `BSEC_MAX_PHYSICAL_SENSOR`.
const MAX_PHYSICAL_SENSOR: usize = 8;
/// `BSEC_MAX_STATE_BLOB_SIZE`.
pub const STATE_BYTES: usize = 221;
/// `BSEC_MAX_WORKBUFFER_SIZE`.
const WORK_BYTES: usize = 4096;
/// The outputs subscribed to.
const OUTPUTS: [u8; 3] = [
    output::IAQ,
    output::CO2_EQUIVALENT,
    output::BREATH_VOC_EQUIVALENT,
];

extern "C" {
    fn bsec_init() -> i32;
    fn bsec_get_version(version: *mut Version) -> i32;
    fn bsec_update_subscription(
        requested_virtual_sensors: *const SensorConfiguration,
        n_requested_virtual_sensors: u8,
        required_sensor_settings: *mut SensorConfiguration,
        n_required_sensor_settings: *mut u8,
    ) -> i32;
    fn bsec_sensor_control(time_stamp: i64, sensor_settings: *mut BmeSettings) -> i32;
    fn bsec_do_steps(
        inputs: *const Input,
        n_inputs: u8,
        outputs: *mut Output,
        n_outputs: *mut u8,
    ) -> i32;
    fn bsec_set_state(
        serialized_state: *const u8,
        n_serialized_state: u32,
        work_buffer: *mut u8,
        n_work_buffer_size: u32,
    ) -> i32;
    fn bsec_get_state(
        state_set_id: u8,
        serialized_state: *mut u8,
        n_serialized_state_max: u32,
        work_buffer: *mut u8,
        n_work_buffer: u32,
        n_serialized_state: *mut u32,
    ) -> i32;
}

/// A `bsec_library_return_t` other than `BSEC_OK`: below zero an error,
/// above a warning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Status(pub i32);

fn check(status: i32) -> Result<(), Status> {
    if status == 0 {
        Ok(())
    } else {
        Err(Status(status))
    }
}

static TAKEN: AtomicBool = AtomicBool::new(false);
/// The library's scratch space for loading and saving its state.
static WORK: ConstStaticCell<[u8; WORK_BYTES]> = ConstStaticCell::new([0; WORK_BYTES]);

/// The library, initialised and subscribed to [`OUTPUTS`] in low power mode.
pub struct Bsec {
    work: &'static mut [u8; WORK_BYTES],
}

impl Bsec {
    /// Initialises the library. `None` after the first call.
    pub fn new() -> Option<Result<Self, Status>> {
        if TAKEN.swap(true, Ordering::Relaxed) {
            return None;
        }
        Some(Self::init())
    }

    fn init() -> Result<Self, Status> {
        // SAFETY: the library's single instance, taken once (`TAKEN`).
        check(unsafe { bsec_init() })?;
        let requested = OUTPUTS.map(|sensor_id| SensorConfiguration {
            sample_rate: SAMPLE_RATE_LP,
            sensor_id,
        });
        let mut required = [SensorConfiguration {
            sample_rate: 0.0,
            sensor_id: 0,
        }; MAX_PHYSICAL_SENSOR];
        let mut n_required = MAX_PHYSICAL_SENSOR as u8;
        // SAFETY: both arrays are as long as the counts passed.
        check(unsafe {
            bsec_update_subscription(
                requested.as_ptr(),
                requested.len() as u8,
                required.as_mut_ptr(),
                &mut n_required,
            )
        })?;
        Ok(Self { work: WORK.take() })
    }

    /// The library's version, `major.minor.major_bugfix.minor_bugfix`.
    pub fn version(&self) -> [u8; 4] {
        let mut v = Version::default();
        // SAFETY: `v` is a valid `bsec_version_t`.
        unsafe { bsec_get_version(&mut v) };
        [v.major, v.minor, v.major_bugfix, v.minor_bugfix]
    }

    /// What the sensor has to do at `now` [ns]. A warning still comes with
    /// settings.
    pub fn sensor_control(&mut self, now: i64) -> (BmeSettings, Result<(), Status>) {
        // SAFETY: all zeroes is a valid `bsec_bme_settings_t`.
        let mut settings: BmeSettings = unsafe { core::mem::zeroed() };
        // SAFETY: `settings` is a valid `bsec_bme_settings_t`.
        let status = check(unsafe { bsec_sensor_control(now, &mut settings) });
        (settings, status)
    }

    /// Feeds one measurement; returns the outputs it produced.
    pub fn do_steps(&mut self, inputs: &[Input]) -> Result<Vec<Output, 3>, Status> {
        let empty = Output {
            time_stamp: 0,
            signal: 0.0,
            signal_dimensions: 0,
            sensor_id: 0,
            accuracy: 0,
        };
        let mut outputs = [empty; OUTPUTS.len()];
        let mut n_outputs = outputs.len() as u8;
        // SAFETY: both arrays are as long as the counts passed.
        let status = unsafe {
            bsec_do_steps(
                inputs.as_ptr(),
                inputs.len() as u8,
                outputs.as_mut_ptr(),
                &mut n_outputs,
            )
        };
        // Warnings (timing, say) still come with outputs.
        if status < 0 {
            return Err(Status(status));
        }
        Ok(outputs[..usize::from(n_outputs).min(OUTPUTS.len())]
            .iter()
            .copied()
            .collect())
    }

    /// The calibration so far, for [`Self::set_state`] after a restart.
    pub fn state(&mut self, out: &mut [u8; STATE_BYTES]) -> Result<usize, Status> {
        let mut n = 0u32;
        // SAFETY: the buffers are as long as the counts passed.
        check(unsafe {
            bsec_get_state(
                0,
                out.as_mut_ptr(),
                out.len() as u32,
                self.work.as_mut_ptr(),
                self.work.len() as u32,
                &mut n,
            )
        })?;
        Ok(n as usize)
    }

    /// Takes up a calibration saved by [`Self::state`].
    pub fn set_state(&mut self, state: &[u8]) -> Result<(), Status> {
        // SAFETY: the buffers are as long as the counts passed.
        check(unsafe {
            bsec_set_state(
                state.as_ptr(),
                state.len() as u32,
                self.work.as_mut_ptr(),
                self.work.len() as u32,
            )
        })
    }
}
