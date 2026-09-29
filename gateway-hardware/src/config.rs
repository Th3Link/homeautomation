//! Persistent configuration storage, backed by `sequential-storage` over
//! raw flash (the `nvs` partition from `partitions.csv`) — the flash-backed
//! counterpart to [`gateway_core::config`]'s schema. Mirrors
//! `cancomponent-rs/cc-hardware`'s `config.rs` structurally (same
//! `Mutex<Option<Config>>` singleton, same `sequential-storage`-over-
//! `SharedFlash` setup), but stores one entry per
//! [`gateway_core::config::Key`] rather than the smaller set
//! `cancomponent-rs` needs, and provides [`Config::load_or_init`] to
//! materialize/persist the full [`GatewayConfig`] at once — the equivalent
//! of every original module's own "read each field, or seed and persist
//! its hardcoded default" boilerplate, now done in one place.

use crate::flash::SharedFlash;
use core::ops::Range;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
pub use gateway_core::config::Key;

use gateway_core::config::{Bitrate, GatewayConfig, WifiMode};
use heapless::String;
use sequential_storage::cache::{Cache, Uncached};
use sequential_storage::map::{MapConfig, MapStorage};

/// Matches the `nvs` entry in `partitions.csv`.
pub const CONFIG_PARTITION: Range<u32> = 0x9000..0xFC000;

pub static CONFIG: Mutex<CriticalSectionRawMutex, Option<Config>> = Mutex::new(None);

/// Initializes the `CONFIG` singleton. Idempotent; safe to call more than
/// once.
pub async fn init() {
    let mut guard = CONFIG.lock().await;
    if guard.is_none() {
        *guard = Some(Config::new());
    }
}

/// Locks and returns the `CONFIG` singleton. Panics if [`init`] hasn't run
/// yet.
pub async fn config(
) -> embassy_sync::mutex::MappedMutexGuard<'static, CriticalSectionRawMutex, Config> {
    let guard = CONFIG.lock().await;
    embassy_sync::mutex::MutexGuard::map(guard, |opt| opt.as_mut().expect("config not initialized"))
}

type ConfigCache = Cache<Uncached, Uncached, Uncached, u8>;

pub struct Config {
    map: MapStorage<u8, BlockingAsync<SharedFlash>, ConfigCache>,
    // Sized for the largest single value (mqtt_uri/mqtt_user/mqtt_password,
    // each up to 60 bytes) plus sequential-storage's own per-item overhead.
    buffer: [u8; 128],
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl Config {
    pub fn new() -> Self {
        Self {
            map: MapStorage::new(
                BlockingAsync::new(SharedFlash),
                MapConfig::new(CONFIG_PARTITION),
                Cache::new_uncached(),
            ),
            buffer: [0; 128],
        }
    }

    pub async fn get_str<const N: usize>(&mut self, key: Key) -> Option<String<N>> {
        let raw: &[u8] = self
            .map
            .fetch_item(&mut self.buffer, &(key as u8))
            .await
            .ok()
            .flatten()?;
        let s = core::str::from_utf8(raw).ok()?;
        let mut out = String::new();
        out.push_str(s).ok()?;
        Some(out)
    }

    pub async fn set_str(&mut self, key: Key, value: &str) -> Result<(), ()> {
        self.map
            .store_item(&mut self.buffer, &(key as u8), &value.as_bytes())
            .await
            .map_err(|_| ())
    }

    pub async fn get_u8(&mut self, key: Key) -> Option<u8> {
        self.map
            .fetch_item(&mut self.buffer, &(key as u8))
            .await
            .ok()
            .flatten()
    }

    pub async fn set_u8(&mut self, key: Key, value: u8) -> Result<(), ()> {
        self.map
            .store_item(&mut self.buffer, &(key as u8), &value)
            .await
            .map_err(|_| ())
    }

    pub async fn get_u32(&mut self, key: Key) -> Option<u32> {
        self.map
            .fetch_item(&mut self.buffer, &(key as u8))
            .await
            .ok()
            .flatten()
    }

    pub async fn set_u32(&mut self, key: Key, value: u32) -> Result<(), ()> {
        self.map
            .store_item(&mut self.buffer, &(key as u8), &value)
            .await
            .map_err(|_| ())
    }

    pub async fn get_bool(&mut self, key: Key) -> Option<bool> {
        self.get_u8(key).await.map(|b| b != 0)
    }

    pub async fn set_bool(&mut self, key: Key, value: bool) -> Result<(), ()> {
        self.set_u8(key, value as u8).await
    }

    /// Reads every field, seeding (and persisting) [`GatewayConfig`]'s
    /// default for any key that isn't set yet — the one-shot equivalent of
    /// every original module's own `read_nvs`/`init`.
    pub async fn load_or_init(&mut self) -> GatewayConfig {
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

        let wifi_mode = match self.get_str::<20>(Key::WifiMode).await {
            Some(v) => WifiMode::from_name(v.as_str()),
            None => {
                let _ = self
                    .set_str(Key::WifiMode, defaults.wifi_mode.as_str())
                    .await;
                defaults.wifi_mode
            }
        };

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
            wifi_mode,
            wifi_ssid: str_field!(Key::WifiSsid, defaults.wifi_ssid),
            wifi_password: str_field!(Key::WifiPassword, defaults.wifi_password),
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
