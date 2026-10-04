//! Persistent configuration storage, backed by `sequential-storage` over
//! raw flash (the `nvs` partition from `partitions.csv`) — the flash-backed
//! counterpart to [`gateway_core::config`]'s schema. Stores one entry per
//! [`gateway_core::config::Key`] and provides [`LoadOrInit::load_or_init`]
//! to materialize/persist the full [`GatewayConfig`] at once — the
//! equivalent of every original module's own "read each field, or seed and
//! persist its hardcoded default" boilerplate, now done in one place. The
//! underlying store/singleton plumbing (`Config`/`CONFIG`) is shared with
//! `cancomponents-hardware` via `common_hardware::config_store`.

use common_hardware::config_store::{ConfigCell, ConfigStore};
use core::ops::Range;
pub use gateway_core::config::Key;
use gateway_core::config::{Bitrate, GatewayConfig};

/// Matches the `nvs` entry in `partitions.csv`.
pub const CONFIG_PARTITION: Range<u32> = 0x9000..0x4C000;

// Sized for the largest single value (mqtt_uri/mqtt_user/mqtt_password,
// each up to 60 bytes) plus sequential-storage's own per-item overhead.
const BUF: usize = 128;

/// The flash-backed config store, keyed by [`Key`].
pub type Config = ConfigStore<BUF>;

static CONFIG: ConfigCell<BUF> = ConfigCell::new();

/// Initializes the `CONFIG` singleton. Idempotent; safe to call more than
/// once.
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

/// Adds the whole-[`GatewayConfig`] load/default logic on top of the
/// generic [`Config`] store — a local trait rather than an inherent method
/// since `Config` is a type alias for a `common_hardware` type this crate
/// doesn't own. Needs to stay `pub` since `bin/main.rs` — a separate crate
/// from this library, despite sharing a package — calls it too.
#[allow(async_fn_in_trait)] // single-threaded embassy executor; Send doesn't matter here
pub trait LoadOrInit {
    /// Reads every field, seeding (and persisting) [`GatewayConfig`]'s
    /// default for any key that isn't set yet — the one-shot equivalent of
    /// every original module's own `read_nvs`/`init`.
    async fn load_or_init(&mut self) -> GatewayConfig;
}

impl LoadOrInit for Config {
    async fn load_or_init(&mut self) -> GatewayConfig {
        let defaults = GatewayConfig::default();

        macro_rules! str_field {
            ($key:expr, $default:expr) => {{
                match self.get_str($key).await {
                    Some(v) => v,
                    None => {
                        let _ = self.set_str($key, $default.as_str()).await;
                        $default
                    }
                }
            }};
        }
        macro_rules! u8_field {
            ($key:expr, $default:expr) => {{
                match self.get_u8($key).await {
                    Some(v) => v,
                    None => {
                        let _ = self.set_u8($key, $default).await;
                        $default
                    }
                }
            }};
        }
        macro_rules! u32_field {
            ($key:expr, $default:expr) => {{
                match self.get_u32($key).await {
                    Some(v) => v,
                    None => {
                        let _ = self.set_u32($key, $default).await;
                        $default
                    }
                }
            }};
        }

        let can_bitrate = match self.get_u8(Key::CanBitrate).await {
            Some(v) => Bitrate::from(v),
            None => {
                let _ = self
                    .set_u8(Key::CanBitrate, defaults.can_bitrate.into())
                    .await;
                defaults.can_bitrate
            }
        };

        let mqtt_enabled = match self.get_bool(Key::MqttEnabled).await {
            Some(v) => v,
            None => {
                let _ = self.set_bool(Key::MqttEnabled, defaults.mqtt_enabled).await;
                defaults.mqtt_enabled
            }
        };

        GatewayConfig {
            hostname: str_field!(Key::Hostname, defaults.hostname),
            mqtt_enabled,
            mqtt_uri: str_field!(Key::MqttUri, defaults.mqtt_uri),
            mqtt_user: str_field!(Key::MqttUser, defaults.mqtt_user),
            mqtt_password: str_field!(Key::MqttPassword, defaults.mqtt_password),
            web_username: str_field!(Key::WebUsername, defaults.web_username),
            web_password: str_field!(Key::WebPassword, defaults.web_password),
            can_id: u8_field!(Key::CanId, defaults.can_id),
            can_type: u8_field!(Key::CanType, defaults.can_type),
            can_bitrate,
            hw_rev: u8_field!(Key::HwRev, defaults.hw_rev),
            custom_string: str_field!(Key::CustomString, defaults.custom_string),
            update_delay_ms: u32_field!(Key::UpdateDelay, defaults.update_delay_ms),
        }
    }
}
