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
}

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
