//! `canbus/button/#`, `canbus/presence/#` — mirrors `BridgeButton.cpp`.
//! CAN->MQTT only, no command topics (there is no `dispatch(topic, ...)`
//! handling on the original `BridgeButton`).

use super::{write_device_segment, Topic};
use crate::button::{ButtonEvent, ButtonEventKind};
use core::fmt::Write;

/// `canbus/button/0x<id>` or `canbus/button/<custom_string>`.
pub fn button_topic(device_key: u32, custom_string: &str) -> Topic {
    let mut t = Topic::new();
    let _ = t.push_str("canbus/button/");
    write_device_segment(&mut t, device_key, custom_string);
    t
}

/// `canbus/presence/0x<id>` or `canbus/presence/<custom_string>`.
pub fn presence_topic(device_key: u32, custom_string: &str) -> Topic {
    let mut t = Topic::new();
    let _ = t.push_str("canbus/presence/");
    write_device_segment(&mut t, device_key, custom_string);
    t
}

/// Renders a `ButtonEvent` payload into its `canbus/button/#` body,
/// `"<button_id>/<released|hold|single|double|tripple>/<count>"` —
/// `None` for [`ButtonEventKind::Pressed`], which the original never
/// publishes (only settled/terminal states are).
pub fn button_body(event: ButtonEvent) -> Option<Topic> {
    let word = match event.event {
        ButtonEventKind::Released => "released",
        ButtonEventKind::Hold => "hold",
        ButtonEventKind::Pressed => return None,
        ButtonEventKind::Single => "single",
        ButtonEventKind::Double => "double",
        ButtonEventKind::Tripple => "tripple",
    };
    let mut body = Topic::new();
    let _ = write!(body, "{}/{}/{}", event.button_id, word, event.count);
    Some(body)
}

/// Renders a `PirSensor` payload into its `canbus/presence/#` body,
/// `"8/<release|hold>/<count>"`. The leading `8` is a literal constant
/// (the original's hardcoded "external PIR" id), **not** derived from
/// `event.button_id` — reproduced faithfully rather than "fixed" to use
/// the decoded id, since that's what the deployed firmware actually
/// sends. Every event kind other than `Released` collapses to `"hold"`.
pub fn presence_body(event: ButtonEvent) -> Topic {
    let word = match event.event {
        ButtonEventKind::Released => "release",
        _ => "hold",
    };
    let mut body = Topic::new();
    let _ = write!(body, "8/{}/{}", word, event.count);
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: ButtonEventKind, count: u16) -> ButtonEvent {
        ButtonEvent {
            button_id: 3,
            event: kind,
            count,
        }
    }

    #[test]
    fn button_topic_prefers_custom_string() {
        assert_eq!(
            button_topic(0x1004_0100, "").as_str(),
            "canbus/button/0x10040100"
        );
        assert_eq!(
            button_topic(0x1004_0100, "hallway").as_str(),
            "canbus/button/hallway"
        );
    }

    #[test]
    fn pressed_is_not_published() {
        assert_eq!(button_body(ev(ButtonEventKind::Pressed, 1)), None);
    }

    #[test]
    fn settled_states_render_their_word() {
        assert_eq!(
            button_body(ev(ButtonEventKind::Released, 1))
                .unwrap()
                .as_str(),
            "3/released/1"
        );
        assert_eq!(
            button_body(ev(ButtonEventKind::Hold, 2)).unwrap().as_str(),
            "3/hold/2"
        );
        assert_eq!(
            button_body(ev(ButtonEventKind::Single, 3))
                .unwrap()
                .as_str(),
            "3/single/3"
        );
        assert_eq!(
            button_body(ev(ButtonEventKind::Double, 4))
                .unwrap()
                .as_str(),
            "3/double/4"
        );
        assert_eq!(
            button_body(ev(ButtonEventKind::Tripple, 5))
                .unwrap()
                .as_str(),
            "3/tripple/5"
        );
    }

    #[test]
    fn presence_body_ignores_decoded_button_id() {
        // Regression test for the hardcoded-"8" quirk: even though this
        // event carries button_id 3, the presence body always says "8".
        assert_eq!(
            presence_body(ev(ButtonEventKind::Released, 7)).as_str(),
            "8/release/7"
        );
    }

    #[test]
    fn presence_body_collapses_every_non_released_kind_to_hold() {
        for kind in [
            ButtonEventKind::Pressed,
            ButtonEventKind::Hold,
            ButtonEventKind::Single,
            ButtonEventKind::Double,
            ButtonEventKind::Tripple,
        ] {
            assert_eq!(presence_body(ev(kind, 0)).as_str(), "8/hold/0");
        }
    }
}
