//! Home Assistant MQTT discovery: the retained configuration messages that
//! make the CAN devices show up in Home Assistant by themselves
//! (`homeassistant/<component>/<node>/<object>/config`, see Home
//! Assistant's "MQTT Discovery"). They point Home Assistant at the state
//! topics the gateway publishes anyway ([`crate::topics`]); nothing about
//! those changes.
//!
//! Per CAN device (a Home Assistant device, `canbus_<key>`):
//!
//! - every sensor reading seen: a `sensor` (temperature, humidity,
//!   pressure, brightness, CO₂, VOC, air quality);
//! - a presence sensor seen: a `binary_sensor` (motion);
//! - every button seen: device triggers for single, double and triple
//!   clicks, hold (repeated while held) and the release after a hold.
//!
//! Relays, rollershutters, lamps and the heating aren't described yet.
//!
//! Pure: the caller supplies the device's facts and the state topics and
//! publishes what comes back, retained.

use crate::sensor_table::SensorKind;
use core::fmt::Write;
use heapless::String;

/// The discovery prefix Home Assistant listens on (its default).
pub const PREFIX: &str = "homeassistant";
/// Home Assistant's birth/last-will topic: `online` after it started, the
/// signal to publish the configurations again.
pub const STATUS_TOPIC: &str = "homeassistant/status";

/// A configuration topic.
pub type Topic = String<96>;
/// A configuration message.
pub type Payload = String<768>;

/// What Home Assistant is told about a CAN device.
#[derive(Debug, Clone, Copy)]
pub struct Device<'a> {
    /// [`crate::can_id::CanId::device_key`].
    pub key: u32,
    /// Its custom string, or empty.
    pub name: &'a str,
    /// The device type's name (`Relais`, `Button`, …).
    pub model: &'a str,
    /// Its firmware version, or empty.
    pub version: &'a str,
}

/// A button event a device trigger fires on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonTrigger {
    Single,
    Double,
    Triple,
    Hold,
    ReleaseAfterHold,
}

impl ButtonTrigger {
    pub const ALL: [ButtonTrigger; 5] = [
        Self::Single,
        Self::Double,
        Self::Triple,
        Self::Hold,
        Self::ReleaseAfterHold,
    ];

    /// The event as the button topic's body names it.
    fn event(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Double => "double",
            Self::Triple => "tripple",
            Self::Hold => "hold",
            Self::ReleaseAfterHold => "released",
        }
    }

    /// Home Assistant's trigger type.
    fn kind(self) -> &'static str {
        match self {
            Self::Single => "button_short_press",
            Self::Double => "button_double_press",
            Self::Triple => "button_triple_press",
            Self::Hold => "button_long_press",
            Self::ReleaseAfterHold => "button_long_release",
        }
    }
}

/// One thing Home Assistant is told about.
#[derive(Debug, Clone, Copy)]
pub enum Entity<'a> {
    /// A sensor reading, published on `state_topic`. `label` names it
    /// among the device's sensors of the same kind (the sensor's name, or
    /// its id), empty if it is the only one.
    Sensor {
        kind: SensorKind,
        sub_id: u64,
        label: &'a str,
        state_topic: &'a str,
    },
    /// The presence sensor (`canbus/presence/…`).
    Presence { state_topic: &'a str },
    /// One event of one button (`canbus/button/…`).
    Button {
        button: u8,
        trigger: ButtonTrigger,
        state_topic: &'a str,
    },
}

/// The configuration topic of `entity` on device `key`.
pub fn topic(key: u32, entity: &Entity<'_>) -> Topic {
    let mut t = Topic::new();
    let _ = match entity {
        Entity::Sensor { kind, sub_id, .. } => write!(
            t,
            "{PREFIX}/sensor/canbus_{key:08x}/{}_{sub_id:x}/config",
            kind.name()
        ),
        Entity::Presence { .. } => {
            write!(t, "{PREFIX}/binary_sensor/canbus_{key:08x}/presence/config")
        }
        Entity::Button {
            button, trigger, ..
        } => write!(
            t,
            "{PREFIX}/device_automation/canbus_{key:08x}/button{button}_{}/config",
            trigger.event()
        ),
    };
    t
}

/// The configuration message of `entity` on `device`; `None` if it
/// doesn't fit.
pub fn payload(device: &Device<'_>, entity: &Entity<'_>) -> Option<Payload> {
    let mut p = Payload::new();
    write_payload(&mut p, device, entity).ok()?;
    Some(p)
}

fn write_payload(p: &mut Payload, device: &Device<'_>, entity: &Entity<'_>) -> core::fmt::Result {
    let key = device.key;
    match entity {
        Entity::Sensor {
            kind,
            sub_id,
            label,
            state_topic,
        } => {
            p.write_str("{\"name\":")?;
            let mut name: String<40> = String::new();
            let _ = name.push_str(kind_label(*kind));
            if !label.is_empty() {
                let _ = name.push(' ');
                let _ = name.push_str(label);
            }
            json_string(p, &name)?;
            write!(
                p,
                ",\"uniq_id\":\"canbus_{key:08x}_{}_{sub_id:x}\"",
                kind.name()
            )?;
            p.write_str(",\"stat_t\":")?;
            json_string(p, state_topic)?;
            if let Some(class) = device_class(*kind) {
                write!(p, ",\"dev_cla\":\"{class}\"")?;
            }
            if !kind.unit().is_empty() {
                p.write_str(",\"unit_of_meas\":")?;
                json_string(p, kind.unit())?;
            }
            p.write_str(",\"stat_cla\":\"measurement\"")?;
        }
        Entity::Presence { state_topic } => {
            write!(
                p,
                "{{\"name\":\"Presence\",\"uniq_id\":\"canbus_{key:08x}_presence\",\"stat_t\":"
            )?;
            json_string(p, state_topic)?;
            // Body `8/<release|hold>/<count>`.
            p.write_str(
                ",\"dev_cla\":\"motion\",\"val_tpl\":\"{{ 'OFF' if value.split('/')[1] == 'release' else 'ON' }}\"",
            )?;
        }
        Entity::Button {
            button,
            trigger,
            state_topic,
        } => {
            p.write_str("{\"atype\":\"trigger\",\"t\":")?;
            json_string(p, state_topic)?;
            write!(
                p,
                ",\"type\":\"{}\",\"stype\":\"button_{}\",\"pl\":\"{button}/{}\"",
                trigger.kind(),
                u16::from(*button) + 1,
                trigger.event()
            )?;
            // Body `<button>/<event>/<count>`: the count doesn't matter.
            p.write_str(",\"val_tpl\":\"{{ value.split('/')[0] ~ '/' ~ value.split('/')[1] }}\"")?;
        }
    }
    write_device(p, device)?;
    p.write_str("}")
}

fn write_device(p: &mut Payload, device: &Device<'_>) -> core::fmt::Result {
    write!(
        p,
        ",\"dev\":{{\"ids\":[\"canbus_{:08x}\"],\"name\":",
        device.key
    )?;
    if device.name.is_empty() {
        let mut name: String<24> = String::new();
        let _ = write!(name, "{} 0x{:08x}", device.model, device.key);
        json_string(p, &name)?;
    } else {
        json_string(p, device.name)?;
    }
    p.write_str(",\"mdl\":")?;
    json_string(p, device.model)?;
    if !device.version.is_empty() {
        p.write_str(",\"sw\":")?;
        json_string(p, device.version)?;
    }
    p.write_str("}")
}

fn kind_label(kind: SensorKind) -> &'static str {
    match kind {
        SensorKind::Temperature => "Temperature",
        SensorKind::Humidity => "Humidity",
        SensorKind::Pressure => "Pressure",
        SensorKind::Co2Equivalent => "CO2",
        SensorKind::VocBreath => "VOC",
        SensorKind::AirQuality => "Air quality",
        SensorKind::Brightness => "Brightness",
    }
}

fn device_class(kind: SensorKind) -> Option<&'static str> {
    match kind {
        SensorKind::Temperature => Some("temperature"),
        SensorKind::Humidity => Some("humidity"),
        SensorKind::Pressure => Some("atmospheric_pressure"),
        SensorKind::Brightness => Some("illuminance"),
        SensorKind::Co2Equivalent => Some("carbon_dioxide"),
        SensorKind::AirQuality => Some("aqi"),
        SensorKind::VocBreath => None,
    }
}

/// `s` as a JSON string.
fn json_string<W: Write>(out: &mut W, s: &str) -> core::fmt::Result {
    out.write_char('"')?;
    for c in s.chars() {
        match c {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32)?,
            c => out.write_char(c)?,
        }
    }
    out.write_char('"')
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;

    const SWITCH: Device<'static> = Device {
        key: 0x1004_4d00,
        name: "testsw",
        model: "Button",
        version: "v1.2",
    };

    #[test]
    fn sensor_config() {
        let entity = Entity::Sensor {
            kind: SensorKind::Temperature,
            sub_id: 0x6800_0077,
            label: "",
            state_topic: "canbus/temperature/testsw/0x68000077",
        };
        assert_eq!(
            topic(SWITCH.key, &entity).as_str(),
            "homeassistant/sensor/canbus_10044d00/temperature_68000077/config"
        );
        assert_eq!(
            payload(&SWITCH, &entity).unwrap().as_str(),
            "{\"name\":\"Temperature\",\"uniq_id\":\"canbus_10044d00_temperature_68000077\",\
             \"stat_t\":\"canbus/temperature/testsw/0x68000077\",\"dev_cla\":\"temperature\",\
             \"unit_of_meas\":\"°C\",\"stat_cla\":\"measurement\",\
             \"dev\":{\"ids\":[\"canbus_10044d00\"],\"name\":\"testsw\",\"mdl\":\"Button\",\"sw\":\"v1.2\"}}"
        );
    }

    #[test]
    fn presence_and_button_configs() {
        let presence = Entity::Presence {
            state_topic: "canbus/presence/testsw",
        };
        let p = payload(&SWITCH, &presence).unwrap();
        assert!(p.contains("\"dev_cla\":\"motion\""));
        assert!(p.contains("'release'"));

        let button = Entity::Button {
            button: 2,
            trigger: ButtonTrigger::Double,
            state_topic: "canbus/button/testsw",
        };
        assert_eq!(
            topic(SWITCH.key, &button).as_str(),
            "homeassistant/device_automation/canbus_10044d00/button2_double/config"
        );
        let p = payload(&SWITCH, &button).unwrap();
        assert!(p.contains(
            "\"type\":\"button_double_press\",\"stype\":\"button_3\",\"pl\":\"2/double\""
        ));
    }

    #[test]
    fn unnamed_devices_and_escaping() {
        let device = Device {
            key: 0x1005_0100,
            name: "",
            model: "Relais",
            version: "",
        };
        let entity = Entity::Sensor {
            kind: SensorKind::Brightness,
            sub_id: 0,
            label: "a\"b",
            state_topic: "canbus/brightness/0x10050100",
        };
        let p = payload(&device, &entity).unwrap();
        assert!(p.contains("\"name\":\"Brightness a\\\"b\""));
        assert!(p.contains("\"name\":\"Relais 0x10050100\""));
        assert!(!p.contains("\"sw\""));
        // Every config fits, with the longest names and topics.
        let long = Device {
            key: u32::MAX,
            name: "abcdefgh",
            model: "HeatingControl",
            version: "v0.3.0-123-gabcdef",
        };
        for trigger in ButtonTrigger::ALL {
            let e = Entity::Button {
                button: 7,
                trigger,
                state_topic: "canbus/button/abcdefgh",
            };
            assert!(payload(&long, &e).is_some());
            assert!(topic(long.key, &e).len() < 96);
        }
    }
}
