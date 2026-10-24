//! Persistent key/value config storage, backed by `sequential-storage` over
//! raw flash (the `CONFIG_PARTITION` range, separate from the OTA app
//! partitions). The underlying store/singleton plumbing (`Config`/
//! `CONFIG`) is shared with `gateway-hardware` via
//! `common_hardware::config_store`.

use common_hardware::config_store::{ConfigCell, ConfigStore};
use core::ops::Range;
use num_enum::{IntoPrimitive, TryFromPrimitive};

pub const CONFIG_PARTITION: Range<u32> = 0x9000..0xFC000;

const BUF: usize = 256;

/// The flash-backed config store, keyed by [`Key`].
pub type Config = ConfigStore<BUF>;

pub static CONFIG: ConfigCell<BUF> = ConfigCell::new();

/// Keys for the single-byte/short-string values persisted in flash. The
/// discriminant is the on-flash key, not just a Rust implementation detail
/// — do not renumber existing entries.
#[derive(Copy, Clone, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum Key {
    RelaisMode = 1,
    ExtensionMode = 2,
    DeviceId = 3,
    DeviceType = 4,
    CustomString = 5,
    /// Stored but currently unused — see the note in `can::init`.
    Baudrate = 6,
    HardwareRevision = 7,
    /// Seconds between `TemperatureSensor` CAN frames (`u32`), see
    /// `temperature::set_send_interval_secs`.
    SensorSendInterval = 8,
    /// DS18B20 topic id (`u32`) of the heating circuit's flow sensor; 0 =
    /// not set.
    HeatingFlowSensor = 9,
    /// Tank charge target in whole °C (`u8`).
    HeatingTankTarget = 10,
    /// Index (0 = top) of the tank sensor the charge target is compared
    /// with (`u8`).
    HeatingTankRef = 11,
    /// Whether missing `Echo` frames restart the device (`u8`, 0 = off;
    /// unset means on). See `echo_guard`.
    EchoRestart = 12,
}

/// Flash key of the tank sensor at position 0 (top); position `n` lives at
/// `TANK_SENSOR_KEY_BASE + n` (`u32` topic id, 0 = not set). Raw keys, clear
/// of the [`Key`] values.
pub const TANK_SENSOR_KEY_BASE: u8 = 0x60;

/// Initializes the `CONFIG` singleton. Idempotent; safe to call more than
/// once (e.g. from multiple init paths).
pub async fn init() {
    CONFIG.init(CONFIG_PARTITION).await;
}

/// Locks and returns the `CONFIG` singleton. Panics if [`init`] hasn't run
/// yet.
pub async fn config() -> embassy_sync::mutex::MappedMutexGuard<
    'static,
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    Config,
> {
    CONFIG.get().await
}
