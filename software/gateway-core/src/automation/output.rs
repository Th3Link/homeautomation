//! What the automation switches. Each output has a name, by which the
//! bindings, motion lights, schedules, MQTT and Home Assistant refer to it.

use super::{invalid, valid_name, DeviceRef, Invalid, Name};
use crate::lamp::LampMsg;
use heapless::Vec;
use serde::{Deserialize, Serialize};

/// The most lamp groups in a [`Light`].
pub const MAX_LAMP_GROUPS: usize = 8;
/// The most levels a [`Light`] has (besides off).
pub const MAX_LEVELS: usize = 15;
/// The most members of a [`Group`].
pub const MAX_MEMBERS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    Light(Light),
    Dimmer(Dimmer),
    Relay(Relay),
    Shutter(Shutter),
    Group(Group),
}

/// Lamps of one controller (and bank) that always get the same value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LampGroup {
    pub device: DeviceRef,
    /// The bank, sent in the 8-byte `LampGroup` form; without one, the
    /// 4-byte form.
    #[serde(default)]
    pub bank: Option<u8>,
    /// The lamps, as the `LampGroup` frame's 24-bit mask.
    pub mask: u32,
}

/// A room light switched in levels: level `n` gives each lamp group the
/// value in row `n − 1` of `levels`, level 0 switches every lamp off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Light {
    pub groups: Vec<LampGroup, MAX_LAMP_GROUPS, u8>,
    /// One row per level, one value (0–255) per lamp group.
    pub levels: Vec<Vec<u8, MAX_LAMP_GROUPS, u8>, MAX_LEVELS, u8>,
    /// The level `on` switches to before the light has been on.
    pub default_level: u8,
}

/// The frames for one controller bank: the target and the commands.
pub type Frames = Vec<(DeviceRef, Option<u8>, LampMsg), { 2 * MAX_LAMP_GROUPS }>;

impl Light {
    /// How many levels the light has besides off.
    pub fn top(&self) -> u8 {
        u8::try_from(self.levels.len()).unwrap_or(u8::MAX)
    }

    /// The `LampGroup` frames that set the light to `level` (clamped to
    /// [`Light::top`]): per controller and bank one frame for the lamps
    /// that are off, then one per distinct value, in that order.
    pub fn frames(&self, level: u8) -> Frames {
        let level = level.min(self.top());
        let value = |group: usize| match level {
            0 => 0,
            n => self.levels[usize::from(n) - 1]
                .get(group)
                .copied()
                .unwrap_or(0),
        };
        let mut frames = Frames::new();
        for (i, group) in self.groups.iter().enumerate() {
            let target = (group.device, group.bank);
            // Each target once, at its first group.
            if self.groups[..i]
                .iter()
                .any(|g| (g.device, g.bank) == target)
            {
                continue;
            }
            let mut values: Vec<u8, MAX_LAMP_GROUPS, u8> = self
                .groups
                .iter()
                .enumerate()
                .filter(|(_, g)| (g.device, g.bank) == target)
                .map(|(j, _)| value(j))
                .collect();
            values.sort_unstable();
            let mut last = None;
            for v in values {
                if last == Some(v) {
                    continue;
                }
                last = Some(v);
                let mask = self
                    .groups
                    .iter()
                    .enumerate()
                    .filter(|(j, g)| (g.device, g.bank) == target && value(*j) == v)
                    .fold(0, |mask, (_, g)| mask | g.mask);
                let _ = frames.push((
                    group.device,
                    group.bank,
                    LampMsg {
                        value: v,
                        bitmask: mask,
                        bank: group.bank.unwrap_or(0),
                    },
                ));
            }
        }
        frames
    }

    fn validate(&self) -> Result<(), Invalid> {
        if self.groups.is_empty() {
            return invalid("groups", "at least one lamp group");
        }
        for g in &self.groups {
            if !g.device.valid() {
                return invalid("groups.device", "a device type below 64 and an id 1-255");
            }
            if g.mask == 0 || g.mask > 0x00FF_FFFF {
                return invalid("groups.mask", "1-24 bits");
            }
        }
        if self.levels.is_empty() {
            return invalid("levels", "at least one level");
        }
        if self.levels.iter().any(|row| row.len() != self.groups.len()) {
            return invalid("levels", "one value per lamp group in every level");
        }
        if self.default_level == 0 || self.default_level > self.top() {
            return invalid("default_level", "1 to the number of levels");
        }
        Ok(())
    }
}

/// Lamps dimmed directly, 0–255.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dimmer {
    pub lamps: LampGroup,
    /// The value `on` switches to before the dimmer has been on.
    pub default_value: u8,
}

/// A relay channel, on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relay {
    pub device: DeviceRef,
    pub channel: u8,
    #[serde(default)]
    pub bank: u8,
}

/// A roller shutter on a relay node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shutter {
    pub device: DeviceRef,
    pub channel: u8,
    #[serde(default)]
    pub bank: u8,
    /// How long the motor runs for a full move; sent with up and down.
    pub travel_ms: u32,
    /// The command value that moves it up, 1 or 2 (the other one is down):
    /// which one depends on the wiring.
    pub up: u8,
}

impl Shutter {
    /// The command value for down.
    pub fn down(self) -> u8 {
        3 - self.up
    }
}

/// Outputs switched together, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub members: Vec<Name, MAX_MEMBERS, u8>,
}

impl Output {
    /// Checks the values. Group members are checked against the other
    /// outputs by the caller.
    pub fn validate(&self) -> Result<(), Invalid> {
        match self {
            Self::Light(light) => light.validate(),
            Self::Dimmer(d) => {
                if !d.lamps.device.valid() {
                    return invalid("lamps.device", "a device type below 64 and an id 1-255");
                }
                if d.lamps.mask == 0 || d.lamps.mask > 0x00FF_FFFF {
                    return invalid("lamps.mask", "1-24 bits");
                }
                if d.default_value == 0 {
                    return invalid("default_value", "1-255");
                }
                Ok(())
            }
            Self::Relay(r) => {
                if !r.device.valid() {
                    return invalid("device", "a device type below 64 and an id 1-255");
                }
                Ok(())
            }
            Self::Shutter(s) => {
                if !s.device.valid() {
                    return invalid("device", "a device type below 64 and an id 1-255");
                }
                if s.up != 1 && s.up != 2 {
                    return invalid("up", "1 or 2");
                }
                if s.travel_ms == 0 || s.travel_ms > 0x00FF_FFFF {
                    return invalid("travel_ms", "1-16777215");
                }
                Ok(())
            }
            Self::Group(g) => {
                if g.members.is_empty() {
                    return invalid("members", "at least one");
                }
                if !g.members.iter().all(|m| valid_name(m)) {
                    return invalid("members", "output names");
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WZ: DeviceRef = DeviceRef {
        device_type: 3,
        id: 5,
    };

    fn group(mask: u32) -> LampGroup {
        LampGroup {
            device: WZ,
            bank: None,
            mask,
        }
    }

    fn vec<T: Clone, const N: usize>(items: &[T]) -> Vec<T, N, u8> {
        Vec::from_slice(items).unwrap()
    }

    /// Two lamps, white and red each: level 1 red only, level 2 both.
    fn light() -> Light {
        Light {
            groups: vec(&[group(0x01), group(0x02), group(0x04), group(0x08)]),
            levels: vec(&[vec(&[0, 34, 0, 34]), vec(&[17, 34, 17, 34])]),
            default_level: 1,
        }
    }

    fn summary(frames: &Frames) -> std::vec::Vec<(u8, u32)> {
        frames
            .iter()
            .map(|(_, _, m)| (m.value, m.bitmask))
            .collect()
    }

    #[test]
    fn a_level_sends_off_first_then_each_value_once() {
        let l = light();
        assert_eq!(summary(&l.frames(1)), [(0, 0x05), (34, 0x0A)]);
        assert_eq!(summary(&l.frames(2)), [(17, 0x05), (34, 0x0A)]);
        assert_eq!(summary(&l.frames(0)), [(0, 0x0F)]);
        // Above the top: the top.
        assert_eq!(l.frames(9), l.frames(2));
    }

    #[test]
    fn each_controller_gets_its_own_frames() {
        let other = DeviceRef {
            device_type: 3,
            id: 8,
        };
        let mut l = light();
        l.groups[2].device = other;
        l.groups[3].device = other;
        let frames = l.frames(1);
        let targets: std::vec::Vec<u8> = frames.iter().map(|(d, _, _)| d.id).collect();
        assert_eq!(targets, [5, 5, 8, 8]);
        assert_eq!(
            summary(&frames),
            [(0, 0x01), (34, 0x02), (0, 0x04), (34, 0x08)]
        );
    }

    #[test]
    fn validation() {
        assert_eq!(Output::Light(light()).validate(), Ok(()));
        let mut l = light();
        l.levels[1].pop();
        assert_eq!(Output::Light(l).validate().unwrap_err().field, "levels");
        let mut l = light();
        l.default_level = 3;
        assert_eq!(
            Output::Light(l).validate().unwrap_err().field,
            "default_level"
        );
        let shutter = Shutter {
            device: DeviceRef {
                device_type: 7,
                id: 1,
            },
            channel: 0,
            bank: 0,
            travel_ms: 20_000,
            up: 3,
        };
        assert_eq!(Output::Shutter(shutter).validate().unwrap_err().field, "up");
        assert_eq!(Shutter { up: 2, ..shutter }.down(), 1);
    }

    #[test]
    fn json_shape() {
        let json =
            r#"{"shutter":{"device":{"type":7,"id":1},"channel":4,"travel_ms":40000,"up":2}}"#;
        let (output, _): (Output, _) = serde_json_core::from_str(json).unwrap();
        assert_eq!(
            output,
            Output::Shutter(Shutter {
                device: DeviceRef {
                    device_type: 7,
                    id: 1
                },
                channel: 4,
                bank: 0,
                travel_ms: 40_000,
                up: 2,
            })
        );
        let json = r#"{"light":{"groups":[{"device":{"type":3,"id":5},"mask":1}],"levels":[[17],[127]],"default_level":1}}"#;
        let (output, _): (Output, _) = serde_json_core::from_str(json).unwrap();
        let Output::Light(l) = output else { panic!() };
        assert_eq!(l.top(), 2);
    }

    #[test]
    fn a_full_light_is_small_on_flash() {
        let full = Light {
            groups: Vec::from_slice(&[group(0x00FF_FFFF); MAX_LAMP_GROUPS]).unwrap(),
            levels: (0..MAX_LEVELS)
                .map(|_| Vec::from_slice(&[255; MAX_LAMP_GROUPS]).unwrap())
                .collect(),
            default_level: 1,
        };
        let mut buffer = [0u8; 512];
        let bytes = super::super::encode(&Output::Light(full), &mut buffer).unwrap();
        assert!(bytes.len() < 256, "{}", bytes.len());
    }
}
