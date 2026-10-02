use num_enum::{FromPrimitive, IntoPrimitive};

/// The `msg_type` byte of a [`crate::can_id::CanId`] — identifies what a
/// CAN frame's payload means.
///
/// This is a fixed wire-protocol enum: the discriminants are the actual
/// bytes sent on the bus (`ICAN::MSG_ID_t` in the original C++) and must
/// never be renumbered. New message types may be appended with unused
/// values; existing ones must not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, FromPrimitive)]
#[repr(u8)]
pub enum CanMessageType {
    Available = 0,
    DeviceError = 1,
    Restart = 2,
    DeviceUid0 = 3,
    DeviceUid1 = 4,
    DeviceIdType = 5,
    DeviceGroup = 6,
    ApplicationVersion = 7,
    Baudrate = 8,
    Uptime = 9,
    CustomString = 10,
    PwmFrequency = 11,
    RequestParameter = 12,
    ApplicationVersionString = 13,
    UpdateSilence = 14,
    FlashStart = 15,
    FlashSelect = 16,
    FlashErase = 17,
    FlashRead = 18,
    FlashWrite = 19,
    FlashVerify = 20,
    FlashProgress = 21,
    FlashComplete = 22,
    ButtonEvent = 30,
    TemperatureSensor = 31,
    HwRev = 41,
    ExtensionMode = 42,
    LampGroup = 90,
    PirSensor = 128,
    HumiditySensor = 129,
    Relais = 130,
    RelaisState = 131,
    Rollershutter = 132,
    RollershutterState = 133,
    RelaisMode = 134,
    AmbientLightSensor = 140,
    AmbientLightSensorWhite = 141,
    Nightlight = 150,
    PressureSensor = 151,
    Co2Equivalent = 152,
    VocBreath = 153,
    AirQuality = 154,
    LogDownload = 155,
    Ping = 156,
    PingDisable = 157,
    Echo = 158,
    /// Not part of the wire protocol. The original C++'s `switch` on
    /// `MSG_ID_t` silently drops unrecognized bytes; representing that as a
    /// typed fallback (rather than refusing to build a [`CanMessageType`]
    /// at all) keeps decoding infallible while still making "unknown
    /// message" an explicit, matchable case instead of undefined behavior.
    #[num_enum(default)]
    InvalidMessage = 255,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_every_named_discriminant() {
        let known: &[(CanMessageType, u8)] = &[
            (CanMessageType::Available, 0),
            (CanMessageType::DeviceError, 1),
            (CanMessageType::Restart, 2),
            (CanMessageType::DeviceUid0, 3),
            (CanMessageType::DeviceUid1, 4),
            (CanMessageType::DeviceIdType, 5),
            (CanMessageType::DeviceGroup, 6),
            (CanMessageType::ApplicationVersion, 7),
            (CanMessageType::Baudrate, 8),
            (CanMessageType::Uptime, 9),
            (CanMessageType::CustomString, 10),
            (CanMessageType::PwmFrequency, 11),
            (CanMessageType::RequestParameter, 12),
            (CanMessageType::ApplicationVersionString, 13),
            (CanMessageType::UpdateSilence, 14),
            (CanMessageType::FlashStart, 15),
            (CanMessageType::FlashSelect, 16),
            (CanMessageType::FlashErase, 17),
            (CanMessageType::FlashRead, 18),
            (CanMessageType::FlashWrite, 19),
            (CanMessageType::FlashVerify, 20),
            (CanMessageType::FlashProgress, 21),
            (CanMessageType::FlashComplete, 22),
            (CanMessageType::ButtonEvent, 30),
            (CanMessageType::TemperatureSensor, 31),
            (CanMessageType::HwRev, 41),
            (CanMessageType::ExtensionMode, 42),
            (CanMessageType::LampGroup, 90),
            (CanMessageType::PirSensor, 128),
            (CanMessageType::HumiditySensor, 129),
            (CanMessageType::Relais, 130),
            (CanMessageType::RelaisState, 131),
            (CanMessageType::Rollershutter, 132),
            (CanMessageType::RollershutterState, 133),
            (CanMessageType::RelaisMode, 134),
            (CanMessageType::AmbientLightSensor, 140),
            (CanMessageType::AmbientLightSensorWhite, 141),
            (CanMessageType::Nightlight, 150),
            (CanMessageType::PressureSensor, 151),
            (CanMessageType::Co2Equivalent, 152),
            (CanMessageType::VocBreath, 153),
            (CanMessageType::AirQuality, 154),
            (CanMessageType::LogDownload, 155),
            (CanMessageType::Ping, 156),
            (CanMessageType::PingDisable, 157),
            (CanMessageType::Echo, 158),
        ];

        for &(variant, byte) in known {
            assert_eq!(u8::from(variant), byte);
            assert_eq!(CanMessageType::from(byte), variant);
        }
    }

    #[test]
    fn unknown_byte_decodes_to_invalid_message_instead_of_panicking() {
        // Gaps in the discriminant list (e.g. 23-29, 43-89, 91-127, 142-149)
        // are intentional — the wire protocol just doesn't use them.
        for byte in [23u8, 89, 127, 200, 254] {
            assert_eq!(CanMessageType::from(byte), CanMessageType::InvalidMessage);
        }
    }
}
