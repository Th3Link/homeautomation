//! Discovery: how nodes make themselves known to the gateway without all
//! of them answering at once (ADR 0021).
//!
//! - A node sends `Discovery` with its name (its custom string, 0-8 bytes
//!   UTF-8) when it starts, when its name changes, and when the gateway
//!   asks: a `Discovery` R frame to device 0 (everybody) or to the node.
//! - The gateway answers every `Discovery` with a `DiscoveryAck` to the
//!   node's id (no payload). It learns the node and its name from the
//!   announcement itself.
//! - Until the ack comes, the node repeats. The first announcement waits
//!   [`first_delay_ms`] (0-2 s at random), so that nodes started or asked
//!   together don't all send in the same moment; after it, the node waits
//!   [`retry_delay_ms`] for the ack: 300 ms-2 s for the first
//!   [`FAST_TRIES`] tries, then 2-5 s, for as long as it takes.

use crate::Payload;

/// The longest name: one frame.
pub const MAX_NAME: usize = 8;

/// Tries with a short wait for the ack, before the long one.
pub const FAST_TRIES: u32 = 5;

/// The `Discovery` payload for `name`, cut to [`MAX_NAME`] bytes at a
/// character boundary.
pub fn encode(name: &str) -> Payload {
    let mut end = name.len().min(MAX_NAME);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    let mut bytes = [0; MAX_NAME];
    bytes[..end].copy_from_slice(&name.as_bytes()[..end]);
    Payload::new(bytes, end)
}

/// The name in a `Discovery` payload; trailing NULs (a padded name) are
/// dropped. `None` if it isn't UTF-8.
pub fn decode(data: &[u8]) -> Option<&str> {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    core::str::from_utf8(&data[..end]).ok()
}

/// How long to wait before the first announcement, from any `random`
/// number: 0-2000 ms.
pub fn first_delay_ms(random: u32) -> u32 {
    between(0, 2000, random)
}

/// How long to wait for the ack after `sent` announcements (1 or more),
/// from any `random` number: 300-2000 ms for the first [`FAST_TRIES`],
/// then 2000-5000 ms.
pub fn retry_delay_ms(sent: u32, random: u32) -> u32 {
    if sent < FAST_TRIES {
        between(300, 2000, random)
    } else {
        between(2000, 5000, random)
    }
}

fn between(low: u32, high: u32, random: u32) -> u32 {
    low + random % (high - low + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_fit_one_frame() {
        assert_eq!(encode("kitchen").as_slice(), b"kitchen");
        assert_eq!(encode("livingroom").as_slice(), b"livingro");
        // Not cut inside a character: "ü" is two bytes.
        assert_eq!(encode("abcdefgü").as_slice(), b"abcdefg");
        assert_eq!(encode("").as_slice().len(), 0);
    }

    #[test]
    fn decode_drops_padding() {
        assert_eq!(decode(b"hall\0\0\0\0"), Some("hall"));
        assert_eq!(decode(b"hall"), Some("hall"));
        assert_eq!(decode(b""), Some(""));
        assert_eq!(decode(&[0xFF]), None);
    }

    #[test]
    fn delays_stay_in_their_ranges() {
        for random in [0, 1, 299, 1701, 2999, u32::MAX] {
            assert!(first_delay_ms(random) <= 2000);
            for sent in 1..FAST_TRIES {
                assert!((300..=2000).contains(&retry_delay_ms(sent, random)));
            }
            for sent in FAST_TRIES..FAST_TRIES + 3 {
                assert!((2000..=5000).contains(&retry_delay_ms(sent, random)));
            }
        }
        assert_eq!(retry_delay_ms(1, 0), 300);
        assert_eq!(retry_delay_ms(1, 1700), 2000);
        assert_eq!(retry_delay_ms(FAST_TRIES, 0), 2000);
    }
}
