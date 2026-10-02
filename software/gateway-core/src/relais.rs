//! Wire formats for the `Relais`/`Rollershutter`/`RelaisState`/
//! `RollershutterState` CAN messages.
//!
//! The original C++ gateway encodes a `Relais`/`Rollershutter` *command*
//! two different ways depending on where the command came from, and this
//! is preserved here rather than "fixed" into one shape, since real nodes
//! on the bus are only ever going to see whatever the deployed gateway
//! actually sends:
//! - MQTT (`BridgeRelais::dispatch`, `canbus/relais_command/#` /
//!   `canbus/rollershutter_command/#`): [`MqttCommand`], 8 bytes,
//!   `[num, state, stop_time(32-bit LE), reserved(16-bit)]` — no `bank`.
//! - The `/control.json` RPC (`Command::relais_rollershutter`):
//!   [`RelaisMsg`], 8 bytes, `[number, state, time(24-bit LE), bank,
//!   reserved(16-bit)]` — matches `ICAN::RELAIS_MSG_t`'s bitfield layout.
//!   This is *also* the shape `RollershutterState` reports are decoded
//!   with ([`RelaisMsg::from_bytes`]).
//!
//! `RelaisState` (as opposed to `RollershutterState`) reports are simpler
//! still: the gateway only ever reads byte 0 ([`decode_relais_state`]).

/// A `Relais`/`Rollershutter` command as sent by an MQTT
/// `canbus/relais_command/#` / `canbus/rollershutter_command/#` topic.
/// Wire layout (8 bytes): `[num, state, stop_time_ms(LE 32-bit), reserved(16-bit)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MqttCommand {
    pub num: u8,
    pub state: u8,
    pub stop_time_ms: u32,
}

impl MqttCommand {
    pub fn to_bytes(self) -> [u8; 8] {
        let t = self.stop_time_ms.to_le_bytes();
        [self.num, self.state, t[0], t[1], t[2], t[3], 0, 0]
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::DecodeError> {
        if data.len() != 8 {
            return Err(crate::DecodeError);
        }
        Ok(Self {
            num: data[0],
            state: data[1],
            stop_time_ms: u32::from_le_bytes([data[2], data[3], data[4], data[5]]),
        })
    }
}

/// A `Relais`/`Rollershutter` command as sent by the `/control.json` RPC,
/// or a `RollershutterState` report as received off the bus. Matches
/// `ICAN::RELAIS_MSG_t`'s packed-bitfield layout exactly.
///
/// Wire layout (8 bytes): `[number, state, time_ms(LE 24-bit), bank,
/// reserved(16-bit)]`. `time_ms` truncates to 24 bits (~4.6h max) — this
/// mirrors the C++ bitfield, which physically cannot hold more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelaisMsg {
    pub number: u8,
    pub state: u8,
    pub time_ms: u32,
    pub bank: u8,
}

impl RelaisMsg {
    pub fn to_bytes(self) -> [u8; 8] {
        let t = (self.time_ms & 0x00FF_FFFF).to_le_bytes();
        [self.number, self.state, t[0], t[1], t[2], self.bank, 0, 0]
    }

    /// Decodes a `RelaisMsg`. Accepts any payload of 6 bytes or more
    /// (reading only the first 6) so it tolerates both the 8-byte frame
    /// the gateway itself sends/receives and a shorter 6-byte frame from a
    /// node that only sends `number..bank` without the trailing reserved
    /// bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::DecodeError> {
        if data.len() < 6 {
            return Err(crate::DecodeError);
        }
        Ok(Self {
            number: data[0],
            state: data[1],
            time_ms: u32::from_le_bytes([data[2], data[3], data[4], 0]),
            bank: data[5],
        })
    }
}

/// Decodes a `RelaisState` report: the gateway only ever reads the first
/// byte (`BridgeRelais::dispatch`'s `RELAIS_STATE` branch), unlike
/// `RollershutterState` which uses the full [`RelaisMsg`] layout.
pub fn decode_relais_state(data: &[u8]) -> Option<u8> {
    data.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mqtt_command_roundtrip() {
        let cmd = MqttCommand {
            num: 3,
            state: 1,
            stop_time_ms: 12_345,
        };
        assert_eq!(MqttCommand::from_bytes(&cmd.to_bytes()).unwrap(), cmd);
    }

    #[test]
    fn mqtt_command_has_no_bank_field_unlike_relais_msg() {
        // Regression test for the two-different-encodings quirk: MqttCommand's
        // wire layout puts a full 32-bit stop_time where RelaisMsg would put a
        // 24-bit time + a bank byte, so the two are NOT interchangeable even
        // though both address the same `Relais`/`Rollershutter` message type.
        let cmd = MqttCommand {
            num: 1,
            state: 3,
            stop_time_ms: 0x00AA_BBCC,
        };
        let bytes = cmd.to_bytes();
        // byte 5 would be `bank` in RelaisMsg but is the top byte of
        // stop_time_ms here.
        assert_eq!(bytes[5], 0x00);
        assert_eq!(bytes, [1, 3, 0xCC, 0xBB, 0xAA, 0x00, 0, 0]);
    }

    #[test]
    fn mqtt_command_rejects_wrong_length() {
        assert!(MqttCommand::from_bytes(&[0u8; 7]).is_err());
        assert!(MqttCommand::from_bytes(&[0u8; 9]).is_err());
    }

    #[test]
    fn relais_msg_roundtrip() {
        let msg = RelaisMsg {
            number: 2,
            state: 3,
            time_ms: 500,
            bank: 1,
        };
        assert_eq!(RelaisMsg::from_bytes(&msg.to_bytes()).unwrap(), msg);
    }

    #[test]
    fn relais_msg_accepts_six_byte_frame() {
        let msg = RelaisMsg {
            number: 2,
            state: 3,
            time_ms: 500,
            bank: 1,
        };
        let full = msg.to_bytes();
        assert_eq!(RelaisMsg::from_bytes(&full[..6]).unwrap(), msg);
    }

    #[test]
    fn relais_msg_time_truncates_to_24_bits() {
        let max_24bit_ms = (1u32 << 24) - 1;
        let msg = RelaisMsg {
            number: 0,
            state: 0,
            time_ms: max_24bit_ms,
            bank: 0,
        };
        assert_eq!(
            RelaisMsg::from_bytes(&msg.to_bytes()).unwrap().time_ms,
            max_24bit_ms
        );
    }

    #[test]
    fn relais_msg_rejects_short_input() {
        for len in 0..6 {
            assert!(RelaisMsg::from_bytes(&[0u8; 8][..len]).is_err());
        }
    }

    #[test]
    fn relais_state_reads_first_byte_only() {
        assert_eq!(decode_relais_state(&[1, 2, 3, 4]), Some(1));
        assert_eq!(decode_relais_state(&[]), None);
    }
}
