//! CAN-frame generation for the `/control.json` RPC (`Command` in the
//! original C++).
//!
//! Turning the JSON request body itself into typed field values (strings,
//! numbers, the odd `commandId`-is-a-string-or-an-array shape for
//! `can_selected`) is left to `gateway-hardware`, which owns the actual
//! JSON parsing/HTTP layer. What lives here is the part worth testing in
//! isolation: given already-parsed fields, which CAN frames does each verb
//! actually put on the bus, in what order, with what quirks. Every
//! function here is organized around one RPC verb, matching
//! `Command::relais_rollershutter`/`Command::lamps`/`Command::save_device`/
//! etc. one-to-one rather than folding them into a single enum — the
//! verbs don't share enough shape for that to simplify anything.

use crate::can_message_type::CanMessageType;
use crate::device_message::encode_id_type;
use crate::lamp::LampMsg;
use crate::relais::RelaisMsg;
use heapless::Vec;

/// Max CAN frames one RPC call can produce — bounds every function below.
/// `save` is the largest emitter (up to 7 independent sends).
pub const MAX_SENDS: usize = 16;

/// Which device(s) a command addresses (`unit`/`commandId` in the
/// original). `CanSelected`'s ids are already-resolved, full
/// `is_ng|group|device_type|device_id` values with `msg_type` zeroed
/// (e.g. from [`crate::device_list::DeviceTable::resolve`] or a raw
/// `"0x..."` string) — this module only ever adds a `msg_type` to them, it
/// never parses hex itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Broadcast to every device (`"can_all"`).
    CanAll,
    /// Broadcast to every device of one type (`"can_by_type"`).
    CanByType(u8),
    /// A fixed list of individually-addressed devices (`"can_selected"`).
    CanSelected(Vec<u32, 8>),
    /// A single device, already resolved (`"can_by_uid"`).
    CanByUid(u32),
}

impl Target {
    fn base_ids(&self, out: &mut Vec<u32, MAX_SENDS>) {
        match self {
            Target::CanAll => {
                let _ = out.push(0x1000_0000);
            }
            Target::CanByType(t) => {
                let _ = out.push(0x1000_0000 | ((*t as u32) << 16));
            }
            Target::CanSelected(ids) => {
                for &id in ids {
                    let _ = out.push(id);
                }
            }
            Target::CanByUid(id) => {
                let _ = out.push(*id);
            }
        }
    }
}

/// One frame to send: a full CAN identifier (already including
/// `msg_type`), the payload, and whether it's a remote-frame request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanSend {
    pub can_id: u32,
    pub data: Vec<u8, 8>,
    pub request: bool,
}

fn make_send(can_id: u32, data: &[u8], request: bool) -> CanSend {
    let mut v = Vec::new();
    let _ = v.extend_from_slice(data);
    CanSend {
        can_id,
        data: v,
        request,
    }
}

/// Expands `target` into one [`CanSend`] per addressed device for
/// `msg_type`/`data`/`request` (`Command::send_can_command`).
pub fn targeted_sends(
    target: &Target,
    msg_type: CanMessageType,
    data: &[u8],
    request: bool,
) -> Vec<CanSend, MAX_SENDS> {
    let mut bases = Vec::new();
    target.base_ids(&mut bases);
    let mut out = Vec::new();
    for base in bases {
        let _ = out.push(make_send(base + u8::from(msg_type) as u32, data, request));
    }
    out
}

/// `"relais"`/`"rollershutter"` (`Command::relais_rollershutter`) — encodes
/// `msg` with [`RelaisMsg::to_bytes`] (the RPC's shape, distinct from the
/// MQTT command's [`crate::relais::MqttCommand`]).
pub fn relais_rollershutter_sends(
    target: &Target,
    rollershutter: bool,
    msg: RelaisMsg,
) -> Vec<CanSend, MAX_SENDS> {
    let msg_type = if rollershutter {
        CanMessageType::Rollershutter
    } else {
        CanMessageType::Relais
    };
    targeted_sends(target, msg_type, &msg.to_bytes(), false)
}

/// `"lamp"` (`Command::lamps`) — **always** sends the 4-byte short form,
/// even though `msg.bank` is accepted from the request; this matches the
/// original, which parses a `bank` field into `LAMP_MSG_t` but then only
/// ever transmits the first 4 bytes.
pub fn lamp_sends(target: &Target, msg: LampMsg) -> Vec<CanSend, MAX_SENDS> {
    targeted_sends(
        target,
        CanMessageType::LampGroup,
        &msg.to_bytes_short(),
        false,
    )
}

/// `"legacy_mode"` (`Command::legacy_mode`) — restarts the target into
/// update mode, for manually bringing up legacy STM32-based nodes.
pub fn legacy_mode_sends(target: &Target) -> Vec<CanSend, MAX_SENDS> {
    targeted_sends(
        target,
        CanMessageType::Restart,
        &[2 /* UPDATE_MODE */],
        false,
    )
}

/// `"silence_on"`/`"silence_off"` (`Command::silence_on`/`silence_off`).
pub fn silence_sends(target: &Target, on: bool) -> Vec<CanSend, MAX_SENDS> {
    targeted_sends(target, CanMessageType::UpdateSilence, &[on as u8], false)
}

/// `"refresh"` (`Command::refresh_device`) — zero-length `RequestParameter`
/// remote-frame request.
pub fn refresh_sends(target: &Target) -> Vec<CanSend, MAX_SENDS> {
    targeted_sends(target, CanMessageType::RequestParameter, &[], true)
}

/// `"ping"` (`Command::ping_device`) — zero-length `Available`
/// remote-frame request.
pub fn ping_sends(target: &Target) -> Vec<CanSend, MAX_SENDS> {
    targeted_sends(target, CanMessageType::Available, &[], true)
}

/// `"restart"` when `unit != "self"` (`Command::restart_device`) —
/// `unit == "self"` restarts the gateway itself instead and never reaches
/// here (that's a local, non-CAN action for `gateway-hardware` to handle
/// directly).
pub fn restart_device_sends(target: &Target) -> Vec<CanSend, MAX_SENDS> {
    targeted_sends(
        target,
        CanMessageType::Restart,
        &[1 /* APPLICATION */],
        false,
    )
}

/// Every independently-optional field of `"save"` (`Command::save_device`).
/// Each `Some` field produces its own CAN send(s), all using the same
/// `target`; absent fields produce nothing. `uid0`/`uid1` only ever get
/// sent when `device_id_type` is also present, matching the original
/// nesting the UID sends inside the `type && id` check — if a UID wasn't
/// actually supplied by the request, it's sent as all-zero (the "bypass"
/// the original's comment describes), not skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveDeviceFields {
    pub uid0: Option<u64>,
    pub uid1: Option<u64>,
    pub device_id_type: Option<(u8, u8)>,
    pub baudrate: Option<u8>,
    pub hwrev: Option<u8>,
    pub extension_mode: Option<ExtensionMode>,
    /// Truncated to 8 bytes on send, matching `custom_string[8]`.
    pub custom_string: Option<heapless::String<8>>,
    pub relais_mode: Option<RelaisMode>,
}

/// `Command::save_device`'s `extension_mode` name table. Unrecognized
/// names fall back to `Off` — replicated via [`ExtensionMode::from_name`]
/// rather than rejecting the request, matching the original silently
/// leaving its locally-initialized `extension_mode = 0` untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionMode {
    Off = 0,
    Buttons = 1,
    Sensors = 2,
    Pwm = 3,
    Relais = 4,
    LegacySensors = 5,
    SwRollershutter = 6,
    HwRollershutter = 7,
}

impl ExtensionMode {
    pub fn from_name(s: &str) -> Self {
        match s {
            "BUTTONS" => Self::Buttons,
            "RELAIS" => Self::Relais,
            "SWROLLERSHUTTER" => Self::SwRollershutter,
            "HWROLLERSHUTTER" => Self::HwRollershutter,
            "PWM" => Self::Pwm,
            "SENSORS" => Self::Sensors,
            "LEGACY_SENSORS" => Self::LegacySensors,
            _ => Self::Off,
        }
    }
}

/// `Command::save_device`'s `relais_mode` name table. Unrecognized names
/// fall back to `Off`, same rationale as [`ExtensionMode::from_name`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelaisMode {
    Off = 0,
    Relais = 1,
    SwRollershutter = 2,
    HwRollershutter = 3,
}

impl RelaisMode {
    pub fn from_name(s: &str) -> Self {
        match s {
            "RELAIS" => Self::Relais,
            "SWROLLERSHUTTER" => Self::SwRollershutter,
            "HWROLLERSHUTTER" => Self::HwRollershutter,
            _ => Self::Off,
        }
    }
}

pub fn save_device_sends(target: &Target, fields: &SaveDeviceFields) -> Vec<CanSend, MAX_SENDS> {
    let mut out: Vec<CanSend, MAX_SENDS> = Vec::new();
    let mut extend = |more: Vec<CanSend, MAX_SENDS>| {
        for s in more {
            let _ = out.push(s);
        }
    };

    if let Some((id, device_type)) = fields.device_id_type {
        // Little-endian byte order: the original aliases a `uint64_t`
        // directly over an 8-byte array on a little-endian target.
        let uid0 = fields.uid0.unwrap_or(0).to_le_bytes();
        let uid1 = fields.uid1.unwrap_or(0).to_le_bytes();
        extend(targeted_sends(
            target,
            CanMessageType::DeviceUid0,
            &uid0,
            false,
        ));
        extend(targeted_sends(
            target,
            CanMessageType::DeviceUid1,
            &uid1,
            false,
        ));
        extend(targeted_sends(
            target,
            CanMessageType::DeviceIdType,
            &encode_id_type(id, device_type),
            false,
        ));
    }

    if let Some(baudrate) = fields.baudrate {
        extend(targeted_sends(
            target,
            CanMessageType::Baudrate,
            &[baudrate],
            false,
        ));
    }

    if let Some(hwrev) = fields.hwrev {
        extend(targeted_sends(
            target,
            CanMessageType::HwRev,
            &[hwrev],
            false,
        ));
    }

    if let Some(mode) = fields.extension_mode {
        extend(targeted_sends(
            target,
            CanMessageType::ExtensionMode,
            &[mode as u8],
            false,
        ));
    }

    if let Some(custom_string) = &fields.custom_string {
        let bytes = custom_string.as_bytes();
        let n = bytes.len().min(8);
        extend(targeted_sends(
            target,
            CanMessageType::CustomString,
            &bytes[..n],
            false,
        ));
    }

    if let Some(mode) = fields.relais_mode {
        extend(targeted_sends(
            target,
            CanMessageType::RelaisMode,
            &[mode as u8],
            false,
        ));
    }

    out
}

/// `Command::save_config`'s one nontrivial validation rule: an MQTT
/// password shorter than 8 characters is silently dropped rather than
/// applied (`if (password_string_len >= 8) { m_mqtt.password(...); }`).
/// Everything else `save_config` does is plain field assignment with no
/// logic worth a pure function.
pub fn should_apply_mqtt_password(password: &str) -> bool {
    password.len() >= 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_all_targets_broadcast_address() {
        let sends = targeted_sends(&Target::CanAll, CanMessageType::Ping, &[], true);
        assert_eq!(sends.len(), 1);
        assert_eq!(sends[0].can_id, 0x1000_0000 + 156);
        assert!(sends[0].request);
    }

    #[test]
    fn can_by_type_broadcasts_within_type() {
        let sends = targeted_sends(
            &Target::CanByType(0x05),
            CanMessageType::Available,
            &[],
            true,
        );
        assert_eq!(sends[0].can_id, 0x1005_0000);
    }

    #[test]
    fn can_selected_sends_to_every_listed_id() {
        let mut ids = Vec::new();
        ids.push(0x1005_0100).unwrap();
        ids.push(0x1005_0200).unwrap();
        let sends = targeted_sends(
            &Target::CanSelected(ids),
            CanMessageType::Restart,
            &[1],
            false,
        );
        assert_eq!(sends.len(), 2);
        assert_eq!(sends[0].can_id, 0x1005_0100 + 2);
        assert_eq!(sends[1].can_id, 0x1005_0200 + 2);
    }

    #[test]
    fn lamp_always_sends_short_form_even_with_bank() {
        let sends = lamp_sends(
            &Target::CanAll,
            LampMsg {
                value: 1,
                bitmask: 2,
                bank: 5,
            },
        );
        assert_eq!(sends[0].data.len(), 4);
    }

    #[test]
    fn save_device_with_no_fields_sends_nothing() {
        let sends = save_device_sends(&Target::CanAll, &SaveDeviceFields::default());
        assert!(sends.is_empty());
    }

    #[test]
    fn save_device_id_type_bypasses_missing_uids_with_zeros() {
        let fields = SaveDeviceFields {
            device_id_type: Some((5, 7)),
            ..Default::default()
        };
        let sends = save_device_sends(&Target::CanAll, &fields);
        assert_eq!(sends.len(), 3);
        assert_eq!(sends[0].data.as_slice(), &[0u8; 8]); // uid0, zero bypass
        assert_eq!(sends[1].data.as_slice(), &[0u8; 8]); // uid1, zero bypass
        assert_eq!(sends[2].data.as_slice(), &[5, 7]); // [device_id, device_type]
    }

    #[test]
    fn save_device_uid_bytes_are_little_endian() {
        let fields = SaveDeviceFields {
            device_id_type: Some((0, 0)),
            uid0: Some(0x0102_0304_0506_0708),
            ..Default::default()
        };
        let sends = save_device_sends(&Target::CanAll, &fields);
        assert_eq!(sends[0].data.as_slice(), &[8, 7, 6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn save_device_each_field_is_independent() {
        let fields = SaveDeviceFields {
            hwrev: Some(3),
            relais_mode: Some(RelaisMode::Relais),
            ..Default::default()
        };
        let sends = save_device_sends(&Target::CanAll, &fields);
        assert_eq!(sends.len(), 2);
        assert_eq!(sends[0].data.as_slice(), &[3]);
        assert_eq!(sends[1].data.as_slice(), &[1]);
    }

    #[test]
    fn extension_mode_and_relais_mode_name_tables() {
        assert_eq!(ExtensionMode::from_name("RELAIS"), ExtensionMode::Relais);
        assert_eq!(ExtensionMode::from_name("nonsense"), ExtensionMode::Off);
        assert_eq!(
            RelaisMode::from_name("HWROLLERSHUTTER"),
            RelaisMode::HwRollershutter
        );
        assert_eq!(RelaisMode::from_name("nonsense"), RelaisMode::Off);
    }

    #[test]
    fn mqtt_password_shorter_than_8_is_rejected() {
        assert!(!should_apply_mqtt_password("short"));
        assert!(should_apply_mqtt_password("longenough"));
    }
}
