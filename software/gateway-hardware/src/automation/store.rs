//! The automation's configuration on flash: its own `sequential-storage`
//! map on the `storage` partition, which the Rust firmware didn't use
//! before, so the settings store isn't worn by it. The layout is
//! [`gateway_core::automation::store`]'s.

use crate::console_log;
use common_hardware::flash::SharedFlash;
use core::ops::Range;
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::{MappedMutexGuard, Mutex, MutexGuard};
use gateway_core::automation::store::{Named, Section, Slots};
use gateway_core::automation::{decode, encode};
use sequential_storage::cache::key_pointers::ArrayKeyPointers;
use sequential_storage::cache::page_pointers::ArrayPagePointers;
use sequential_storage::cache::page_states::ArrayPageStates;
use sequential_storage::cache::Cache;
use sequential_storage::map::{MapConfig, MapStorage};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Matches the `storage` entry in `partitions.csv`.
pub const PARTITION: Range<u32> = 0x4D000..0x66000;
const PAGES: usize = (PARTITION.end - PARTITION.start) as usize / 4096;
/// Where recently used keys are, so most reads don't scan the pages.
const CACHED_KEYS: usize = 32;
/// The largest stored item (a light with 8 lamp groups and 15 levels is
/// about 250 bytes).
const ITEM: usize = 384;
/// An item plus `sequential-storage`'s overhead.
const DATA: usize = ITEM + 64;

type StoreCache = Cache<
    ArrayPageStates<PAGES>,
    ArrayPagePointers<PAGES>,
    ArrayKeyPointers<u16, CACHED_KEYS>,
    u16,
>;

/// Why an item couldn't be stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    /// It doesn't fit [`ITEM`] bytes.
    TooLarge,
    /// Its section is full.
    Full,
    Flash,
}

pub struct Store {
    map: MapStorage<u16, BlockingAsync<SharedFlash>, StoreCache>,
    data: [u8; DATA],
    item: [u8; ITEM],
    /// Which slots of each section ([`Section`] as index) are taken.
    slots: [Slots; 5],
}

static STORE: Mutex<CriticalSectionRawMutex, Option<Store>> = Mutex::new(None);

/// Opens the store and finds out which slots are taken. A partition that
/// doesn't hold a map (the C++ gateway's SPIFFS) is erased.
pub async fn init() {
    let mut store = Store {
        map: MapStorage::new(
            BlockingAsync::new(SharedFlash),
            MapConfig::new(PARTITION),
            Cache::new(
                ArrayPageStates::new(),
                ArrayPagePointers::new(),
                ArrayKeyPointers::new(),
            ),
        ),
        data: [0; DATA],
        item: [0; ITEM],
        slots: [Slots::new(); 5],
    };
    if let Err(e) = store.scan().await {
        console_log!("automation: store unreadable ({e:?}), erasing it");
        if store.map.erase_all().await.is_err() {
            console_log!("automation: erasing the store failed");
        }
        store.slots = [Slots::new(); 5];
    }
    *STORE.lock().await = Some(store);
}

/// Locks the store. Panics if [`init`] hasn't run.
pub async fn store() -> MappedMutexGuard<'static, CriticalSectionRawMutex, Store> {
    MutexGuard::map(STORE.lock().await, |s| {
        s.as_mut().expect("automation store not initialized")
    })
}

impl Store {
    /// Notes the keys of every stored item.
    async fn scan(
        &mut self,
    ) -> Result<(), sequential_storage::Error<esp_storage::FlashStorageError>> {
        let mut keys: heapless::Vec<u16, 256> = heapless::Vec::new();
        {
            let mut items = self.map.fetch_all_items(&mut self.data).await?;
            while let Some((key, _)) = items.next::<&[u8]>(&mut self.data).await? {
                if !keys.contains(&key) {
                    let _ = keys.push(key);
                }
            }
        }
        // The iteration also returns old values and removed items; a fetch
        // tells what is there now.
        for key in keys {
            let Some((section, slot)) = Section::split(key) else {
                continue;
            };
            let present = self
                .map
                .fetch_item::<&[u8]>(&mut self.data, &key)
                .await?
                .is_some_and(|v| !v.is_empty());
            if present {
                self.slots[section as usize].take(slot);
            }
        }
        Ok(())
    }

    /// The taken slots of `section`.
    pub fn slots(&self, section: Section) -> Slots {
        self.slots[section as usize]
    }

    /// Reads the item in `slot`; `None` if there is none or it doesn't
    /// decode (logged).
    pub async fn get<T: DeserializeOwned>(&mut self, section: Section, slot: u8) -> Option<T> {
        let key = section.key(slot);
        let bytes: &[u8] = match self.map.fetch_item(&mut self.data, &key).await {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return None,
            Err(e) => {
                console_log!("automation: reading {key:#06x} failed: {e:?}");
                return None;
            }
        };
        let len = bytes.len().min(ITEM);
        self.item[..len].copy_from_slice(&bytes[..len]);
        let value = decode(&self.item[..len]);
        if value.is_none() && len > 0 {
            console_log!("automation: item {key:#06x} doesn't decode, skipped");
        }
        value
    }

    /// Stores `value` in `slot`.
    pub async fn put<T: Serialize>(
        &mut self,
        section: Section,
        slot: u8,
        value: &T,
    ) -> Result<(), StoreError> {
        let len = encode(value, &mut self.item)
            .ok_or(StoreError::TooLarge)?
            .len();
        let key = section.key(slot);
        self.map
            .store_item(&mut self.data, &key, &&self.item[..len])
            .await
            .map_err(|e| {
                console_log!("automation: storing {key:#06x} failed: {e:?}");
                StoreError::Flash
            })?;
        self.slots[section as usize].take(slot);
        Ok(())
    }

    /// Stores `value` in the first free slot of `section`; returns it.
    pub async fn add<T: Serialize>(
        &mut self,
        section: Section,
        value: &T,
    ) -> Result<u8, StoreError> {
        let slot = self
            .slots(section)
            .first_free(section.capacity())
            .ok_or(StoreError::Full)?;
        self.put(section, slot, value).await?;
        Ok(slot)
    }

    pub async fn remove(&mut self, section: Section, slot: u8) -> Result<(), StoreError> {
        let key = section.key(slot);
        self.map
            .remove_item(&mut self.data, &key)
            .await
            .map_err(|_| StoreError::Flash)?;
        self.slots[section as usize].free(slot);
        Ok(())
    }

    /// The slot of the named item `name` in `section` (outputs, motion
    /// lights, schedules).
    pub async fn find<T: DeserializeOwned>(&mut self, section: Section, name: &str) -> Option<u8> {
        for slot in self.slots(section).iter() {
            if let Some(named) = self.get::<Named<T>>(section, slot).await {
                if named.name == name {
                    return Some(slot);
                }
            }
        }
        None
    }
}
