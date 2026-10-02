//! Wire format for the `DeviceIdType` CAN message, which assigns a device
//! its `device_id`/`device_type` (sent as `data[0]=device_id,
//! data[1]=device_type`; `Command::save_device` in the original C++
//! gateway is the one caller that sends it).

/// Parses a `DeviceIdType` payload into `(device_id, device_type)`.
///
/// Returns `None` if `data` isn't exactly 2 bytes.
pub fn parse_id_type(data: &[u8]) -> Option<(u8, u8)> {
    if data.len() != 2 {
        return None;
    }
    Some((data[0], data[1]))
}

/// Encodes a `DeviceIdType` payload from `(device_id, device_type)`.
pub fn encode_id_type(device_id: u8, device_type: u8) -> [u8; 2] {
    [device_id, device_type]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_byte_payload() {
        assert_eq!(parse_id_type(&[5, 7]), Some((5, 7)));
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(parse_id_type(&[]), None);
        assert_eq!(parse_id_type(&[5]), None);
        assert_eq!(parse_id_type(&[5, 7, 9]), None);
    }

    #[test]
    fn encode_decode_roundtrip() {
        let bytes = encode_id_type(5, 7);
        assert_eq!(parse_id_type(&bytes), Some((5, 7)));
    }
}
