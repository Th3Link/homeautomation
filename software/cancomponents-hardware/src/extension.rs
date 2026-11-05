//! Selects and starts whatever is wired to the second I2C header / GPIO
//! extension connector (buttons, sensors, PWM, relay boards, or nothing),
//! based on the persisted `ExtensionMode`.
//!
//! In every I2C mode the bus is shared, and relay boards without a µC
//! (banks 1-4, see [`crate::relais`]) are looked for on it as well, so they
//! can hang on the same bus as the sensors or the PWM boards.

use crate::button::Button;
use crate::config::{self, config};
use crate::pwm::Pwm;
use crate::sensors::Sensors;
use cancomponents_core::extension::Mode;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal::gpio::{AnyPin, Level, Output, OutputConfig};
use esp_hal::i2c::master::{Config, I2c};
use esp_hal::Async;
use static_cell::StaticCell;

/// An I2C bus shared between tasks.
pub(crate) type Bus = Mutex<CriticalSectionRawMutex, I2c<'static, Async>>;
/// One user of a [`Bus`].
pub(crate) type Device = I2cDevice<'static, CriticalSectionRawMutex, I2c<'static, Async>>;

static BUS: StaticCell<Bus> = StaticCell::new();

/// The extension bus on `sda`/`scl`.
fn bus(
    i2c1: esp_hal::peripherals::I2C1<'static>,
    sda: AnyPin<'static>,
    scl: AnyPin<'static>,
) -> &'static Bus {
    let i2c = I2c::new(i2c1, Config::default())
        .unwrap()
        .with_sda(sda)
        .with_scl(scl)
        .into_async();
    BUS.init(Mutex::new(i2c))
}

pub struct Extension {}

impl Extension {
    /// Reads `ExtensionMode` from flash (defaulting to `Off`) and wires up
    /// the corresponding peripheral. `pin0`/`pin1` are the I2C SDA/SCL pins
    /// in I2C-based modes; all four pins are individual GPIOs in `Button`
    /// mode. `vcc_gnd`, where present, powers the extension board itself.
    /// Returns the I2C bus in the I2C modes, for the relay boards.
    pub async fn init(
        i2c1: esp_hal::peripherals::I2C1<'static>,
        pin0: AnyPin<'static>,
        pin1: AnyPin<'static>,
        pin2: AnyPin<'static>,
        pin3: AnyPin<'static>,
        vcc_gnd: Option<(AnyPin<'static>, AnyPin<'static>)>,
        spawner: &Spawner,
    ) -> Option<&'static Bus> {
        let extension_mode = config()
            .await
            .get_u8(config::Key::ExtensionMode)
            .await
            .map(Mode::from)
            .unwrap_or(Mode::Off);

        match extension_mode {
            Mode::Button => {
                if let Some((vcc, gnd)) = vcc_gnd {
                    Output::new(vcc, Level::High, OutputConfig::default());
                    Output::new(gnd, Level::Low, OutputConfig::default());
                }
                // Reported as buttons 4-7, after the board's 0-3.
                Button::init(pin0, pin1, pin2, pin3, 4, spawner);
                None
            }
            Mode::Sensors => {
                if let Some((vcc, gnd)) = vcc_gnd {
                    Output::new(vcc, Level::High, OutputConfig::default());
                    Output::new(gnd, Level::Low, OutputConfig::default());
                }
                let bus = bus(i2c1, pin0, pin1);
                Sensors::init(bus, pin2, pin3, spawner).await;
                Some(bus)
            }
            Mode::LegacySensors => {
                // The original firmware's `leg_sen` wiring of hwrev 1
                // boards: the board is powered from GPIO2 (VCC) and GPIO18
                // (GND), and the PIR moves to GPIO5 (the `vcc_gnd` pair's
                // first pin). Not verified on such a board.
                let gnd = pin3;
                if let Some((pin3, vcc)) = vcc_gnd {
                    Output::new(vcc, Level::High, OutputConfig::default());
                    Output::new(gnd, Level::Low, OutputConfig::default());
                    let bus = bus(i2c1, pin0, pin1);
                    Sensors::init(bus, pin2, pin3, spawner).await;
                    Some(bus)
                } else {
                    // Legacy Sensors are only compatible with hwrev 1
                    // From hwrev 2 on vcc and gnd are hard wired
                    None
                }
            }
            Mode::Pwm => {
                let bus = bus(i2c1, pin0, pin1);
                Pwm::init(I2cDevice::new(bus), spawner);
                Some(bus)
            }
            Mode::Relais => Some(bus(i2c1, pin0, pin1)),
            _ => None,
        }
    }
}
