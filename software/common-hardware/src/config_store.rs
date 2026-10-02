//! `sequential-storage`-backed key/value flash config storage — the
//! boilerplate shared by `gateway-hardware::config` and
//! `cancomponents-hardware::config`: a [`ConfigStore`] wrapping
//! `MapStorage` over [`crate::flash::SharedFlash`] with typed get/set
//! accessors, and a [`ConfigCell`] singleton (a `Mutex<Option<ConfigStore>>`
//! with idempotent `init`/`get`) so each firmware crate doesn't have to
//! reimplement that plumbing. The actual `Key` enum and any whole-struct
//! load-or-default logic are crate-specific and stay in each firmware
//! crate — only construct a [`ConfigStore`] with *a* key type that
//! implements `Into<u8>` (any `#[derive(IntoPrimitive)]` `#[repr(u8)]` enum
//! qualifies).

use crate::flash::SharedFlash;
use core::ops::Range;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::{MappedMutexGuard, Mutex, MutexGuard};
use heapless::String;
use sequential_storage::cache::{Cache, Uncached};
use sequential_storage::map::{MapConfig, MapStorage};

type ConfigCache = Cache<Uncached, Uncached, Uncached, u8>;

/// A key/value store over one flash partition. `BUF` is the scratch buffer
/// size, sized by the caller for its largest single value plus
/// `sequential-storage`'s own per-item overhead.
pub struct ConfigStore<const BUF: usize> {
    map: MapStorage<u8, BlockingAsync<SharedFlash>, ConfigCache>,
    buffer: [u8; BUF],
}

impl<const BUF: usize> ConfigStore<BUF> {
    pub fn new(partition: Range<u32>) -> Self {
        Self {
            map: MapStorage::new(
                BlockingAsync::new(SharedFlash),
                MapConfig::new(partition),
                Cache::new_uncached(),
            ),
            buffer: [0; BUF],
        }
    }

    pub async fn get_str<const N: usize>(&mut self, key: impl Into<u8>) -> Option<String<N>> {
        let raw: &[u8] = self
            .map
            .fetch_item(&mut self.buffer, &key.into())
            .await
            .ok()
            .flatten()?;
        let s = core::str::from_utf8(raw).ok()?;
        let mut out = String::new();
        out.push_str(s).ok()?;
        Some(out)
    }

    pub async fn set_str(&mut self, key: impl Into<u8>, value: &str) -> Result<(), ()> {
        self.map
            .store_item(&mut self.buffer, &key.into(), &value.as_bytes())
            .await
            .map_err(|_| ())
    }

    pub async fn get_u8(&mut self, key: impl Into<u8>) -> Option<u8> {
        self.map
            .fetch_item(&mut self.buffer, &key.into())
            .await
            .ok()
            .flatten()
    }

    pub async fn set_u8(&mut self, key: impl Into<u8>, value: u8) -> Result<(), ()> {
        self.map
            .store_item(&mut self.buffer, &key.into(), &value)
            .await
            .map_err(|_| ())
    }

    pub async fn get_u32(&mut self, key: impl Into<u8>) -> Option<u32> {
        self.map
            .fetch_item(&mut self.buffer, &key.into())
            .await
            .ok()
            .flatten()
    }

    pub async fn set_u32(&mut self, key: impl Into<u8>, value: u32) -> Result<(), ()> {
        self.map
            .store_item(&mut self.buffer, &key.into(), &value)
            .await
            .map_err(|_| ())
    }

    pub async fn get_bool(&mut self, key: impl Into<u8>) -> Option<bool> {
        self.get_u8(key).await.map(|b| b != 0)
    }

    pub async fn set_bool(&mut self, key: impl Into<u8>, value: bool) -> Result<(), ()> {
        self.set_u8(key, value as u8).await
    }
}

/// A lazily-initialized, lockable [`ConfigStore`] singleton — the
/// `static CONFIG: Mutex<Option<Config>>` + `init()`/`config()` pattern
/// every firmware crate needs, generalized once.
pub struct ConfigCell<const BUF: usize>(Mutex<CriticalSectionRawMutex, Option<ConfigStore<BUF>>>);

impl<const BUF: usize> ConfigCell<BUF> {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// Initializes the store over `partition`. Idempotent; safe to call
    /// more than once.
    pub async fn init(&self, partition: Range<u32>) {
        let mut guard = self.0.lock().await;
        if guard.is_none() {
            *guard = Some(ConfigStore::new(partition));
        }
    }

    /// Locks and returns the store. Panics if [`Self::init`] hasn't run yet.
    pub async fn get(&self) -> MappedMutexGuard<'_, CriticalSectionRawMutex, ConfigStore<BUF>> {
        let guard = self.0.lock().await;
        MutexGuard::map(guard, |opt| opt.as_mut().expect("config not initialized"))
    }
}

impl<const BUF: usize> Default for ConfigCell<BUF> {
    fn default() -> Self {
        Self::new()
    }
}
