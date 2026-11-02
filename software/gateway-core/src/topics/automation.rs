//! `canbus/output_*/<name>` and `canbus/automation_*/<name>`: the outputs,
//! motion lights and schedules of the gateway's own automation
//! ([`crate::automation`]), by name.
//!
//! - `canbus/output_state/<name>`: a light's level (`0` = off), a
//!   dimmer's value, `ON`/`OFF` for a relay, `up`/`down`/`stopped` for a
//!   shutter.
//! - `canbus/output_command/<name>`: `ON`, `OFF`, `TOGGLE`, `MAX`, `INC`,
//!   `DEC`, `UP`, `DOWN`, `STOP`, or a number (a light's level, a dimmer's
//!   value).
//! - `canbus/automation_state/<name>`, `canbus/automation_command/<name>`:
//!   `ON`/`OFF` (enabled or not); commands also `TOGGLE`.

use super::Topic;
use crate::automation::binding::{Op, Switch};
use crate::automation::engine::{Movement, OutputKind, OutputState};
use crate::DecodeError;
use core::fmt::Write;
use heapless::String;

pub const OUTPUT_COMMAND_TOPIC: &str = "canbus/output_command/#";
pub const AUTOMATION_COMMAND_TOPIC: &str = "canbus/automation_command/#";

const OUTPUT_COMMAND: &str = "canbus/output_command/";
const AUTOMATION_COMMAND: &str = "canbus/automation_command/";

pub fn output_state_topic(name: &str) -> Topic {
    let mut t = Topic::new();
    let _ = write!(t, "canbus/output_state/{name}");
    t
}

pub fn automation_state_topic(name: &str) -> Topic {
    let mut t = Topic::new();
    let _ = write!(t, "canbus/automation_state/{name}");
    t
}

/// The output named by a `canbus/output_command/<name>` topic.
pub fn output_command_name(topic: &str) -> Option<&str> {
    topic.strip_prefix(OUTPUT_COMMAND)
}

/// The motion light or schedule named by a
/// `canbus/automation_command/<name>` topic.
pub fn automation_command_name(topic: &str) -> Option<&str> {
    topic.strip_prefix(AUTOMATION_COMMAND)
}

/// Parses an output command body.
pub fn parse_output_command(body: &str) -> Result<Op, DecodeError> {
    let body = body.trim();
    Ok(match body.to_ascii_uppercase_short().as_str() {
        "ON" => Op::On,
        "OFF" => Op::Off,
        "TOGGLE" => Op::Toggle,
        "MAX" => Op::Max,
        "INC" => Op::Inc,
        "DEC" => Op::Dec,
        "UP" => Op::Up,
        "DOWN" => Op::Down,
        "STOP" => Op::Stop,
        _ => Op::Set(body.parse().map_err(|_| DecodeError)?),
    })
}

/// Parses an automation command body.
pub fn parse_automation_command(body: &str) -> Result<Switch, DecodeError> {
    match body.trim().to_ascii_uppercase_short().as_str() {
        "ON" => Ok(Switch::Enable),
        "OFF" => Ok(Switch::Disable),
        "TOGGLE" => Ok(Switch::Toggle),
        _ => Err(DecodeError),
    }
}

/// The `output_state` body of an output; `None` for a group, which has no
/// state of its own.
pub fn output_state_body(kind: OutputKind, state: OutputState) -> Option<String<8>> {
    let mut body = String::new();
    match kind {
        OutputKind::Light | OutputKind::Dimmer => {
            let _ = write!(body, "{}", state.level);
        }
        OutputKind::Relay => {
            let _ = body.push_str(if state.level > 0 { "ON" } else { "OFF" });
        }
        OutputKind::Shutter => {
            let _ = body.push_str(match state.movement {
                Movement::Up => "up",
                Movement::Down => "down",
                Movement::Stopped => "stopped",
            });
        }
        OutputKind::Group => return None,
    }
    Some(body)
}

pub fn automation_state_body(enabled: bool) -> &'static str {
    if enabled {
        "ON"
    } else {
        "OFF"
    }
}

/// Upper case of a short ASCII word (longer ones come back empty, which
/// matches no keyword).
trait UpperShort {
    fn to_ascii_uppercase_short(&self) -> String<8>;
}

impl UpperShort for str {
    fn to_ascii_uppercase_short(&self) -> String<8> {
        let mut out = String::new();
        for c in self.chars() {
            if out.push(c.to_ascii_uppercase()).is_err() {
                return String::new();
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics() {
        assert_eq!(
            output_state_topic("bad").as_str(),
            "canbus/output_state/bad"
        );
        assert_eq!(
            automation_state_topic("bad-pir").as_str(),
            "canbus/automation_state/bad-pir"
        );
        assert_eq!(
            output_command_name("canbus/output_command/bad"),
            Some("bad")
        );
        assert_eq!(output_command_name("canbus/output_state/bad"), None);
        assert_eq!(
            automation_command_name("canbus/automation_command/bad-pir"),
            Some("bad-pir")
        );
    }

    #[test]
    fn commands() {
        assert_eq!(parse_output_command("ON"), Ok(Op::On));
        assert_eq!(parse_output_command("off"), Ok(Op::Off));
        assert_eq!(parse_output_command(" 7 "), Ok(Op::Set(7)));
        assert_eq!(parse_output_command("Up"), Ok(Op::Up));
        assert_eq!(parse_output_command("300"), Err(DecodeError));
        assert_eq!(parse_output_command("sideways"), Err(DecodeError));
        assert_eq!(parse_automation_command("on"), Ok(Switch::Enable));
        assert_eq!(parse_automation_command("TOGGLE"), Ok(Switch::Toggle));
        assert_eq!(parse_automation_command("1"), Err(DecodeError));
    }

    #[test]
    fn state_bodies() {
        let state = |level, movement| {
            let mut s = OutputState::default();
            s.level = level;
            s.movement = movement;
            s
        };
        let body =
            |kind, s| output_state_body(kind, s).map(|b| std::string::String::from(b.as_str()));
        assert_eq!(
            body(OutputKind::Light, state(4, Movement::Stopped)).as_deref(),
            Some("4")
        );
        assert_eq!(
            body(OutputKind::Relay, state(1, Movement::Stopped)).as_deref(),
            Some("ON")
        );
        assert_eq!(
            body(OutputKind::Shutter, state(0, Movement::Down)).as_deref(),
            Some("down")
        );
        assert_eq!(body(OutputKind::Group, state(0, Movement::Stopped)), None);
        assert_eq!(automation_state_body(false), "OFF");
    }
}
