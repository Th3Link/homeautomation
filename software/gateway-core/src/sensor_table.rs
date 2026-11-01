//! The latest reading of every sensor seen on the bus (for the web UI's
//! sensor list): one entry per device, kind of reading and sensor id, kept
//! up to date by the fixed-point sensor messages and the ambient light
//! ([`crate::sensor`]).

use crate::can_message_type::CanMessageType;
use heapless::Vec;

/// What a sensor message measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorKind {
    Temperature,
    Humidity,
    Pressure,
    Co2Equivalent,
    VocBreath,
    AirQuality,
    /// Ambient light (`AmbientLightSensor`), lux; one per device.
    Brightness,
}

impl SensorKind {
    /// The kind a message type carries, if it is a sensor one.
    pub fn from_message(msg_type: CanMessageType) -> Option<Self> {
        Some(match msg_type {
            CanMessageType::TemperatureSensor => Self::Temperature,
            CanMessageType::HumiditySensor => Self::Humidity,
            CanMessageType::PressureSensor => Self::Pressure,
            CanMessageType::Co2Equivalent => Self::Co2Equivalent,
            CanMessageType::VocBreath => Self::VocBreath,
            CanMessageType::AirQuality => Self::AirQuality,
            CanMessageType::AmbientLightSensor => Self::Brightness,
            _ => return None,
        })
    }

    /// The message type that carries this kind.
    pub fn message(self) -> CanMessageType {
        match self {
            Self::Temperature => CanMessageType::TemperatureSensor,
            Self::Humidity => CanMessageType::HumiditySensor,
            Self::Pressure => CanMessageType::PressureSensor,
            Self::Co2Equivalent => CanMessageType::Co2Equivalent,
            Self::VocBreath => CanMessageType::VocBreath,
            Self::AirQuality => CanMessageType::AirQuality,
            Self::Brightness => CanMessageType::AmbientLightSensor,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Temperature => "temperature",
            Self::Humidity => "humidity",
            Self::Pressure => "pressure",
            Self::Co2Equivalent => "co2",
            Self::VocBreath => "voc",
            Self::AirQuality => "air_quality",
            Self::Brightness => "brightness",
        }
    }

    /// The unit, where the protocol defines one.
    pub fn unit(self) -> &'static str {
        match self {
            Self::Temperature => "°C",
            Self::Humidity => "%",
            Self::Pressure => "hPa",
            Self::Brightness => "lx",
            // BSEC's estimates from a BME680's gas sensor.
            Self::Co2Equivalent | Self::VocBreath => "ppm",
            // An index, 0-500.
            Self::AirQuality => "",
        }
    }
}

/// Whether the node can tell the gateway a slot and name for this sensor
/// (`SensorConfig`): only for its DS18B20s. The others (a light switch's
/// BME680 and light sensor) are known by their device.
pub fn named_by_node(kind: SensorKind, sub_id: u64) -> bool {
    kind == SensorKind::Temperature && !common_core::sensor_id::is_bme680(sub_id)
}

/// One sensor and what it reported last.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SensorEntry {
    /// [`crate::can_id::CanId::device_key`] of the device the sensor is on.
    pub device_key: u32,
    pub kind: SensorKind,
    /// The sensor's own id (48 bits; the 1-Wire address of a DS18B20).
    pub sub_id: u64,
    pub value: f32,
    /// Uptime-clock seconds of the last reading.
    pub last_seen_s: u32,
}

/// Registry of every sensor seen, capacity `N`.
pub struct SensorTable<const N: usize> {
    entries: Vec<SensorEntry, N>,
}

impl<const N: usize> Default for SensorTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> SensorTable<N> {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Records a reading. A sensor that is already known is updated; a new
    /// one is added, and when the table is full it replaces the sensor that
    /// reported longest ago (it is the likeliest to be gone).
    pub fn observe(
        &mut self,
        device_key: u32,
        kind: SensorKind,
        sub_id: u64,
        value: f32,
        now_s: u32,
    ) {
        let entry = SensorEntry {
            device_key,
            kind,
            sub_id,
            value,
            last_seen_s: now_s,
        };
        if let Some(known) = self
            .entries
            .iter_mut()
            .find(|e| e.device_key == device_key && e.kind == kind && e.sub_id == sub_id)
        {
            *known = entry;
        } else if self.entries.push(entry).is_err() {
            if let Some(oldest) = self.entries.iter_mut().min_by_key(|e| e.last_seen_s) {
                *oldest = entry;
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &SensorEntry> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn a_known_sensor_is_updated_not_duplicated() {
        let mut table: SensorTable<4> = SensorTable::new();
        table.observe(0x1009_0900, SensorKind::Temperature, 7, 20.5, 10);
        table.observe(0x1009_0900, SensorKind::Temperature, 7, 21.0, 20);
        assert_eq!(table.len(), 1);
        let entry = table.iter().next().unwrap();
        assert_eq!((entry.value, entry.last_seen_s), (21.0, 20));
    }

    #[test]
    fn sensors_differ_by_device_kind_and_id() {
        let mut table: SensorTable<8> = SensorTable::new();
        table.observe(1, SensorKind::Temperature, 7, 1.0, 0);
        table.observe(2, SensorKind::Temperature, 7, 1.0, 0);
        table.observe(1, SensorKind::Humidity, 7, 1.0, 0);
        table.observe(1, SensorKind::Temperature, 8, 1.0, 0);
        assert_eq!(table.len(), 4);
    }

    #[test]
    fn a_full_table_replaces_the_stalest_sensor() {
        let mut table: SensorTable<2> = SensorTable::new();
        table.observe(1, SensorKind::Temperature, 1, 1.0, 100);
        table.observe(1, SensorKind::Temperature, 2, 2.0, 50);
        table.observe(1, SensorKind::Temperature, 3, 3.0, 200);
        assert_eq!(table.len(), 2);
        let mut ids: std::vec::Vec<u64> = table.iter().map(|e| e.sub_id).collect();
        ids.sort();
        assert_eq!(ids, [1, 3]);
    }

    #[test]
    fn only_sensor_messages_have_a_kind() {
        assert_eq!(
            SensorKind::from_message(CanMessageType::TemperatureSensor),
            Some(SensorKind::Temperature)
        );
        assert_eq!(SensorKind::from_message(CanMessageType::Relais), None);
        assert_eq!(SensorKind::Temperature.unit(), "°C");
        assert_eq!(SensorKind::Co2Equivalent.unit(), "ppm");
        assert_eq!(SensorKind::AirQuality.unit(), "");
    }
}
