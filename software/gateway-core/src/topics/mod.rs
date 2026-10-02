//! MQTT topic construction/parsing — the actual translation logic of the
//! original C++ `Bridge*` classes, with all CAN/MQTT I/O stripped out.
//! Each submodule mirrors one `Bridge*.cpp`. `gateway-hardware` calls
//! these to build outgoing topics/bodies from CAN frames and to parse
//! incoming MQTT messages into the CAN payload types from
//! [`crate::relais`]/[`crate::lamp`]/[`crate::button`].

pub mod button;
pub mod debug;
pub mod device;
pub mod lamp;
pub mod relais;

use crate::helper::{mqtt_split, to_hex_string};
use heapless::String;

/// Big enough for every topic this crate builds (longest is a
/// rollershutter-state topic with bank/number suffix).
pub type Topic = String<80>;

/// Writes the "device or custom name" segment shared by every state topic:
/// the device's custom string if it has one, otherwise `0x<hex device
/// key>`. Matches every `Bridge*::dispatch`'s `if (custom_string.length() >
/// 0) ... else ...` branch.
pub(crate) fn write_device_segment(out: &mut Topic, device_key: u32, custom_string: &str) {
    use core::fmt::Write;
    if custom_string.is_empty() {
        let _ = write!(out, "0x{}", to_hex_string(device_key));
    } else {
        let _ = write!(out, "{custom_string}");
    }
}

/// The target device string from a `"canbus/<x>_command/<target>"` topic —
/// the segment at position 2 after `/`-splitting (matching
/// `mqtt_split(topic, topic_len, 2)`), to be resolved with
/// [`crate::device_list::DeviceTable::resolve`].
pub fn command_target(topic: &str) -> &str {
    mqtt_split(topic, 2)
}

/// `canbus/gateway/restart` — exact match, any payload
/// (`BridgeDebug::dispatch`'s `restartcommand` branch).
pub const GATEWAY_RESTART_TOPIC: &str = "canbus/gateway/restart";

pub fn is_gateway_restart_topic(topic: &str) -> bool {
    topic == GATEWAY_RESTART_TOPIC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_segment_prefers_custom_string() {
        let mut t = Topic::new();
        write_device_segment(&mut t, 0x1005_0100, "kitchen");
        assert_eq!(t.as_str(), "kitchen");
    }

    #[test]
    fn device_segment_falls_back_to_hex_id() {
        let mut t = Topic::new();
        write_device_segment(&mut t, 0x1005_0100, "");
        assert_eq!(t.as_str(), "0x10050100");
    }

    #[test]
    fn command_target_reads_third_segment() {
        assert_eq!(command_target("canbus/relais_command/kitchen"), "kitchen");
    }

    #[test]
    fn restart_topic_is_exact_match_only() {
        assert!(is_gateway_restart_topic("canbus/gateway/restart"));
        assert!(!is_gateway_restart_topic("canbus/gateway/restart/extra"));
        assert!(!is_gateway_restart_topic("canbus/gateway/restar"));
    }
}
