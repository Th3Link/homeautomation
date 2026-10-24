//! `/api/v1/devices`: the CAN nodes seen on the bus.

use super::error::ApiError;
use super::json::acquire;
use crate::device_list::{now_minutes, DEVICE_LIST};
use core::fmt::Write;
use core::str::FromStr;
use gateway_core::device_list::DeviceListEntry;
use gateway_core::device_type::DeviceType;
use heapless::{String, Vec};
use picoserve::extract::Query;
use picoserve::response::IntoResponse;
use picoserve::routing::{get, parse_path_segment, PathRouter};
use picoserve::Router;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

/// Entries per list page: the page is copied out of the device table so
/// that the table isn't locked while the response is written. Kept small
/// because every web task has room for one page in its (static) future.
const PAGE: usize = 8;

/// The routes under `/devices`, to be added to the API router.
pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    router.route("/devices", get(list)).route(
        ("/devices", parse_path_segment::<DeviceName>()),
        get(one).patch(super::commands::patch_device),
    )
}

/// The `:device` path segment: a custom name or a `0x…` key, as in the
/// MQTT topics.
pub struct DeviceName(String<12>);

impl DeviceName {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl FromStr for DeviceName {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        String::try_from(s).map(Self).map_err(|_| ())
    }
}

/// A device table entry as the API shows it. It is serialized straight
/// from the entry (the hex strings are formatted on the fly), so nothing
/// but the entry itself has to be kept.
struct DeviceView<'a> {
    entry: &'a DeviceListEntry,
    now_minutes: u32,
}

fn hex<const N: usize>(args: core::fmt::Arguments) -> String<N> {
    let mut s = String::new();
    let _ = s.write_fmt(args);
    s
}

impl Serialize for DeviceView<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let e = self.entry;
        let device_type = (e.id >> 16) as u8;
        let mut s = serializer.serialize_struct("Device", 16)?;
        s.serialize_field("key", &hex::<10>(format_args!("0x{:08x}", e.id)))?;
        s.serialize_field("name", e.custom_string.as_str())?;
        s.serialize_field("device_type", &device_type)?;
        s.serialize_field("type_name", DeviceType::from(device_type).name())?;
        s.serialize_field("device_id", &((e.id >> 8) as u8))?;
        s.serialize_field("version", e.version.as_str())?;
        s.serialize_field(
            "last_seen_minutes_ago",
            &self.now_minutes.saturating_sub(e.last_seen_minutes),
        )?;
        s.serialize_field("uptime", &e.uptime)?;
        s.serialize_field("hwrev", &e.hwrev)?;
        s.serialize_field("baudrate", &e.baudrate)?;
        s.serialize_field("extension_mode", &e.extension_mode)?;
        s.serialize_field("relais_mode", &e.relais_mode)?;
        s.serialize_field("state", &e.state)?;
        s.serialize_field("error", &e.error)?;
        s.serialize_field(
            "uid0",
            &hex::<18>(format_args!("0x{:016x}", u64::from_be_bytes(e.uid0))),
        )?;
        s.serialize_field(
            "uid1",
            &hex::<18>(format_args!("0x{:016x}", u64::from_be_bytes(e.uid1))),
        )?;
        s.end()
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// Only devices of this type.
    #[serde(default)]
    device_type: Option<u8>,
    #[serde(default)]
    offset: usize,
}

struct DevicePage {
    /// Devices matching the filter, in all pages.
    total: usize,
    offset: usize,
    now_minutes: u32,
    devices: Vec<DeviceListEntry, PAGE>,
}

impl Serialize for DevicePage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Devices<'a>(&'a DevicePage);
        impl Serialize for Devices<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_seq(self.0.devices.iter().map(|entry| DeviceView {
                    entry,
                    now_minutes: self.0.now_minutes,
                }))
            }
        }
        let mut s = serializer.serialize_struct("DevicePage", 3)?;
        s.serialize_field("total", &self.total)?;
        s.serialize_field("offset", &self.offset)?;
        s.serialize_field("devices", &Devices(self))?;
        s.end()
    }
}

async fn list(Query(query): Query<ListQuery>) -> Result<impl IntoResponse, ApiError> {
    // The buffer first: waiting for it must not hold up the table.
    let buffer = acquire().await;
    let page = {
        let table = DEVICE_LIST.lock().await;
        let matching = || {
            table
                .iter()
                .filter(|e| query.device_type.is_none_or(|t| (e.id >> 16) as u8 == t))
        };
        DevicePage {
            total: matching().count(),
            offset: query.offset,
            now_minutes: now_minutes(),
            devices: matching().skip(query.offset).take(PAGE).cloned().collect(),
        }
    };
    buffer.render(&page)
}

struct One {
    entry: DeviceListEntry,
    now_minutes: u32,
}

impl Serialize for One {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        DeviceView {
            entry: &self.entry,
            now_minutes: self.now_minutes,
        }
        .serialize(serializer)
    }
}

async fn one(DeviceName(name): DeviceName) -> Result<impl IntoResponse, ApiError> {
    let buffer = acquire().await;
    let entry = {
        let table = DEVICE_LIST.lock().await;
        table
            .resolve(name.as_str())
            .and_then(|key| table.iter().find(|e| e.id == key))
            .cloned()
            .ok_or(ApiError::not_found("no such device"))?
    };
    buffer.render(&One {
        entry,
        now_minutes: now_minutes(),
    })
}
