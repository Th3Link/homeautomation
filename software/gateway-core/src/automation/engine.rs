//! The state of all outputs, motion lights and schedules, and what to send
//! when something happens.
//!
//! The engine keeps what it needs at every event in RAM: per output its
//! kind and state, the motion lights and the schedules. A light's level
//! table and the bindings' actions stay with the caller (on flash): a
//! button event comes in with its [`Binding`], and a light level goes out
//! as [`Command::Light`], for the caller to turn into frames with
//! [`super::output::Light::frames`].
//!
//! Monotonic time (`now`, milliseconds) runs the timers; wall-clock time
//! (in the [`Context`]) runs the schedules and the conditions.

use super::binding::{Action, Binding, Op, Switch};
use super::motion::{self, MotionLight};
use super::output::{Dimmer, Output, Relay, Shutter, MAX_MEMBERS};
use super::schedule::{self, Schedule};
use super::when::Context;
use super::{invalid, DeviceRef, Invalid, Name, MAX_MOTION_LIGHTS, MAX_OUTPUTS, MAX_SCHEDULES};
use crate::lamp::LampMsg;
use crate::relais::RelaisMsg;
use heapless::Vec;

/// Relay state value for on (Rust nodes need 3, legacy nodes take any
/// non-zero value).
const RELAY_ON: u8 = 3;
/// Dimmer steps for `inc`/`dec`.
const DIMMER_STEP: u8 = 25;

/// What to send, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Set the light output `output` (index) to `level`: send the frames of
    /// its stored configuration.
    Light {
        output: u8,
        level: u8,
    },
    /// A `LampGroup` frame (dimmers).
    Lamp {
        device: DeviceRef,
        with_bank: bool,
        msg: LampMsg,
    },
    /// A `Relais` frame, or a `Rollershutter` frame for shutters.
    Relais {
        device: DeviceRef,
        rollershutter: bool,
        msg: RelaisMsg,
    },
    Nightlight {
        device: DeviceRef,
        value: u8,
    },
    /// The output `output`'s state changed (for MQTT).
    OutputState {
        output: u8,
    },
    /// The automation `index` of `kind` was enabled or disabled.
    AutomationState {
        kind: Kind,
        index: u8,
    },
}

/// A motion light or a schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Motion,
    Schedule,
}

pub type Commands = Vec<Command, 48, u8>;

/// A shutter's movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Movement {
    #[default]
    Stopped,
    Up,
    Down,
}

/// What kind of output an output is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Light,
    Dimmer,
    Relay,
    Shutter,
    Group,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Spec {
    Light {
        top: u8,
        default_level: u8,
    },
    Dimmer(Dimmer),
    Relay(Relay),
    Shutter(Shutter),
    /// Member indices.
    Group(Vec<u8, MAX_MEMBERS, u8>),
}

/// An output's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OutputState {
    /// Light level, dimmer value, or relay 0/1. Unknown until switched or
    /// reported: 0.
    pub level: u8,
    /// The last non-zero level, for `on`.
    pub last_on: u8,
    pub movement: Movement,
    /// When a shutter's movement ends by itself (ms).
    moving_until: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Slot {
    name: Name,
    spec: Spec,
    state: OutputState,
}

/// Who switched an output, so a motion light can tell its own switching
/// from someone else's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Outside,
    Motion(usize),
}

#[derive(Debug, Default)]
pub struct Engine {
    outputs: Vec<Slot, MAX_OUTPUTS, u8>,
    motion: Vec<(Name, MotionLight, motion::State), MAX_MOTION_LIGHTS, u8>,
    schedules: Vec<(Name, Schedule, schedule::State), MAX_SCHEDULES, u8>,
}

impl Engine {
    pub const fn new() -> Self {
        Self {
            outputs: Vec::new(),
            motion: Vec::new(),
            schedules: Vec::new(),
        }
    }

    /// Adds an output. Group members must have been added before.
    pub fn add_output(&mut self, name: &str, output: &Output) -> Result<(), Invalid> {
        output.validate()?;
        if self.output(name).is_some() {
            return invalid("name", "already an output");
        }
        let spec = match output {
            Output::Light(l) => Spec::Light {
                top: l.top(),
                default_level: l.default_level,
            },
            Output::Dimmer(d) => Spec::Dimmer(*d),
            Output::Relay(r) => Spec::Relay(*r),
            Output::Shutter(s) => Spec::Shutter(*s),
            Output::Group(g) => {
                let mut members = Vec::new();
                for m in &g.members {
                    let Some(i) = self.output(m) else {
                        return invalid("members", "not an output");
                    };
                    if matches!(self.outputs[i].spec, Spec::Group(_)) {
                        return invalid("members", "not a group");
                    }
                    let _ = members.push(i as u8);
                }
                Spec::Group(members)
            }
        };
        let slot = Slot {
            name: Name::try_from(name).map_err(|_| Invalid {
                field: "name",
                reason: "too long",
            })?,
            spec,
            state: OutputState {
                level: 0,
                last_on: 0,
                movement: Movement::Stopped,
                moving_until: 0,
            },
        };
        self.outputs.push(slot).map_err(|_| Invalid {
            field: "outputs",
            reason: "too many",
        })
    }

    pub fn add_motion(
        &mut self,
        name: &str,
        config: MotionLight,
        enabled: bool,
    ) -> Result<(), Invalid> {
        config.validate()?;
        if self.output(&config.output).is_none() {
            return invalid("output", "not an output");
        }
        if self.automation(name).is_some() {
            return invalid("name", "already a motion light or schedule");
        }
        let name = Name::try_from(name).map_err(|_| Invalid {
            field: "name",
            reason: "too long",
        })?;
        self.motion
            .push((name, config, motion::State::new(enabled)))
            .map_err(|_| Invalid {
                field: "motion",
                reason: "too many",
            })
    }

    pub fn add_schedule(
        &mut self,
        name: &str,
        config: Schedule,
        enabled: bool,
    ) -> Result<(), Invalid> {
        config.validate()?;
        if self.output(&config.output).is_none() {
            return invalid("output", "not an output");
        }
        if self.automation(name).is_some() {
            return invalid("name", "already a motion light or schedule");
        }
        let name = Name::try_from(name).map_err(|_| Invalid {
            field: "name",
            reason: "too long",
        })?;
        self.schedules
            .push((name, config, schedule::State::new(enabled)))
            .map_err(|_| Invalid {
                field: "schedules",
                reason: "too many",
            })
    }

    /// The index of the output `name`.
    pub fn output(&self, name: &str) -> Option<usize> {
        self.outputs.iter().position(|s| s.name == name)
    }

    pub fn output_name(&self, index: u8) -> Option<&str> {
        self.outputs
            .get(usize::from(index))
            .map(|s| s.name.as_str())
    }

    pub fn output_state(&self, index: u8) -> Option<OutputState> {
        self.outputs.get(usize::from(index)).map(|s| s.state)
    }

    pub fn output_kind(&self, index: u8) -> Option<OutputKind> {
        Some(match self.outputs.get(usize::from(index))?.spec {
            Spec::Light { .. } => OutputKind::Light,
            Spec::Dimmer(_) => OutputKind::Dimmer,
            Spec::Relay(_) => OutputKind::Relay,
            Spec::Shutter(_) => OutputKind::Shutter,
            Spec::Group(_) => OutputKind::Group,
        })
    }

    /// Whether the output is a light (its level is 0 to its top level).
    pub fn light_top(&self, index: u8) -> Option<u8> {
        match self.outputs.get(usize::from(index))?.spec {
            Spec::Light { top, .. } => Some(top),
            _ => None,
        }
    }

    /// The motion light or schedule `name`.
    pub fn automation(&self, name: &str) -> Option<(Kind, usize)> {
        if let Some(i) = self.motion.iter().position(|m| m.0 == name) {
            return Some((Kind::Motion, i));
        }
        self.schedules
            .iter()
            .position(|s| s.0 == name)
            .map(|i| (Kind::Schedule, i))
    }

    pub fn automation_name(&self, kind: Kind, index: u8) -> Option<&str> {
        let i = usize::from(index);
        match kind {
            Kind::Motion => self.motion.get(i).map(|m| m.0.as_str()),
            Kind::Schedule => self.schedules.get(i).map(|s| s.0.as_str()),
        }
    }

    pub fn automation_enabled(&self, kind: Kind, index: u8) -> Option<bool> {
        let i = usize::from(index);
        match kind {
            Kind::Motion => self.motion.get(i).map(|m| m.2.enabled),
            Kind::Schedule => self.schedules.get(i).map(|s| s.2.enabled),
        }
    }

    pub fn motion_mode(&self, index: u8) -> Option<motion::Mode> {
        self.motion.get(usize::from(index)).map(|m| m.2.mode)
    }

    pub fn schedule_running(&self, index: u8) -> Option<bool> {
        self.schedules
            .get(usize::from(index))
            .map(|s| s.2.running())
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        (self.outputs.len(), self.motion.len(), self.schedules.len())
    }

    /// A button event with its binding: does the actions in order.
    pub fn button(&mut self, now: u64, binding: &Binding, out: &mut Commands) {
        for action in &binding.actions {
            match action {
                Action::Output { name, op } => {
                    if let Some(i) = self.output(name) {
                        self.apply(i, *op, now, Source::Outside, out);
                    }
                }
                Action::Automation { name, switch } => {
                    self.switch_automation(name, *switch, out);
                }
                Action::Nightlight { device, value } => {
                    let _ = out.push(Command::Nightlight {
                        device: *device,
                        value: *value,
                    });
                }
            }
        }
    }

    /// An operation on an output from outside (MQTT, Home Assistant, the
    /// web UI). `false` if there is no such output.
    pub fn command(&mut self, now: u64, output: &str, op: Op, out: &mut Commands) -> bool {
        match self.output(output) {
            Some(i) => {
                self.apply(i, op, now, Source::Outside, out);
                true
            }
            None => false,
        }
    }

    /// Enables, disables or toggles a motion light or schedule. Returns
    /// its new state, `None` if there is no such automation.
    pub fn switch_automation(
        &mut self,
        name: &str,
        switch: Switch,
        out: &mut Commands,
    ) -> Option<bool> {
        let (kind, i) = self.automation(name)?;
        let enabled = match kind {
            Kind::Motion => &mut self.motion[i].2.enabled,
            Kind::Schedule => &mut self.schedules[i].2.enabled,
        };
        let new = match switch {
            Switch::Enable => true,
            Switch::Disable => false,
            Switch::Toggle => !*enabled,
        };
        if new != *enabled {
            match kind {
                Kind::Motion => self.motion[i].2.set_enabled(new),
                Kind::Schedule => self.schedules[i].2.set_enabled(new),
            }
            let _ = out.push(Command::AutomationState {
                kind,
                index: i as u8,
            });
        }
        Some(new)
    }

    /// A motion detector (a `PirSensor` report) of `device_key` saw motion.
    pub fn motion(&mut self, now: u64, ctx: &Context<'_>, device_key: u32, out: &mut Commands) {
        for i in 0..self.motion.len() {
            let (_, config, state) = &mut self.motion[i];
            if !config.detectors.iter().any(|d| d.key() == device_key) {
                continue;
            }
            let conditions = &config.conditions;
            let decision = state.motion(config, now, || conditions.iter().all(|c| c.holds(ctx)));
            if decision == Some(motion::Do::On) {
                self.motion_switch(i, true, now, out);
            }
        }
    }

    /// A relay node reported a channel's state.
    pub fn relay_state(
        &mut self,
        now: u64,
        device_key: u32,
        bank: u8,
        channel: u8,
        on: bool,
        out: &mut Commands,
    ) {
        let found = self.outputs.iter().position(|s| match s.spec {
            Spec::Relay(r) => {
                r.device.key() == device_key && r.bank == bank && r.channel == channel
            }
            _ => false,
        });
        if let Some(i) = found {
            let level = u8::from(on);
            if self.outputs[i].state.level != level {
                self.set_level(i, level, now, Source::Outside, out);
            }
        }
    }

    /// A shutter node reported a shutter's state (0 stopped, 1 or 2 the
    /// direction it moves in, 0xFF both relays on: direction 2).
    pub fn shutter_state(
        &mut self,
        now: u64,
        device_key: u32,
        bank: u8,
        channel: u8,
        raw: u8,
        out: &mut Commands,
    ) {
        let found = self.outputs.iter().position(|s| match s.spec {
            Spec::Shutter(sh) => {
                sh.device.key() == device_key && sh.bank == bank && sh.channel == channel
            }
            _ => false,
        });
        let Some(i) = found else { return };
        let Spec::Shutter(shutter) = self.outputs[i].spec else {
            return;
        };
        let raw = if raw == 0xFF { 2 } else { raw };
        let movement = if raw == 0 {
            Movement::Stopped
        } else if raw == shutter.up {
            Movement::Up
        } else {
            Movement::Down
        };
        let state = &mut self.outputs[i].state;
        if state.movement != movement {
            state.movement = movement;
            state.moving_until = now + u64::from(shutter.travel_ms);
            let _ = out.push(Command::OutputState { output: i as u8 });
        }
    }

    /// Time passes: motion light timers, shutter movements, schedules.
    /// Call at [`Engine::deadline`] and whenever the clock or a
    /// brightness changes.
    pub fn tick(&mut self, now: u64, ctx: &Context<'_>, out: &mut Commands) {
        for i in 0..self.motion.len() {
            let (_, config, state) = &mut self.motion[i];
            if state.tick(config, now) == Some(motion::Do::Off) {
                self.motion_switch(i, false, now, out);
            }
        }
        for i in 0..self.outputs.len() {
            let state = &mut self.outputs[i].state;
            if state.movement != Movement::Stopped && now >= state.moving_until {
                state.movement = Movement::Stopped;
                let _ = out.push(Command::OutputState { output: i as u8 });
            }
        }
        for i in 0..self.schedules.len() {
            let (_, config, state) = &mut self.schedules[i];
            let output = config.output.clone();
            for d in state.evaluate(config, ctx) {
                if let Some(o) = self.output(&output) {
                    let op = match d {
                        schedule::Do::On => Op::On,
                        schedule::Do::Off => Op::Off,
                    };
                    self.apply(o, op, now, Source::Outside, out);
                }
            }
        }
    }

    /// When [`Engine::tick`] has something to do next: a monotonic time
    /// (ms) for the timers and a local time (seconds) for the schedules.
    pub fn deadline(&self, ctx: &Context<'_>) -> (Option<u64>, Option<i64>) {
        let timers = self
            .motion
            .iter()
            .filter_map(|m| m.2.deadline())
            .chain(
                self.outputs
                    .iter()
                    .filter(|s| s.state.movement != Movement::Stopped)
                    .map(|s| s.state.moving_until),
            )
            .min();
        let schedules = self
            .schedules
            .iter()
            .filter_map(|s| s.2.deadline(&s.1, ctx))
            .min();
        (timers, schedules)
    }

    fn motion_switch(&mut self, motion: usize, on: bool, now: u64, out: &mut Commands) {
        let config = &self.motion[motion].1;
        let Some(i) = self.output(&config.output) else {
            return;
        };
        let op = match (on, config.level) {
            (false, _) => Op::Off,
            (true, Some(level)) => Op::Set(level),
            (true, None) => Op::On,
        };
        self.apply(i, op, now, Source::Motion(motion), out);
    }

    /// Applies `op` to output `i`.
    fn apply(&mut self, i: usize, op: Op, now: u64, source: Source, out: &mut Commands) {
        let slot = &self.outputs[i];
        let state = slot.state;
        match slot.spec.clone() {
            Spec::Group(members) => {
                for m in members {
                    self.apply(usize::from(m), op, now, source, out);
                }
            }
            Spec::Light { top, default_level } => {
                let on_level = if state.last_on > 0 {
                    state.last_on
                } else {
                    default_level
                };
                let level = match op {
                    Op::On if state.level == 0 => on_level,
                    Op::On => return,
                    Op::Off => 0,
                    Op::Toggle if state.level == 0 => on_level,
                    Op::Toggle => 0,
                    Op::Set(n) => n.min(top),
                    Op::Max => top,
                    Op::Inc => (state.level + 1).min(top),
                    Op::Dec => state.level.saturating_sub(1),
                    _ => return,
                };
                // Explicit levels go out even when unchanged, to resync.
                let explicit = matches!(op, Op::Off | Op::Set(_) | Op::Max);
                if level != state.level || explicit {
                    let _ = out.push(Command::Light {
                        output: i as u8,
                        level,
                    });
                }
                self.set_level(i, level, now, source, out);
            }
            Spec::Dimmer(d) => {
                let on_value = if state.last_on > 0 {
                    state.last_on
                } else {
                    d.default_value
                };
                let value = match op {
                    Op::On if state.level == 0 => on_value,
                    Op::On => return,
                    Op::Off => 0,
                    Op::Toggle if state.level == 0 => on_value,
                    Op::Toggle => 0,
                    Op::Set(n) => n,
                    Op::Max => u8::MAX,
                    Op::Inc => state.level.saturating_add(DIMMER_STEP),
                    Op::Dec => state.level.saturating_sub(DIMMER_STEP),
                    _ => return,
                };
                let _ = out.push(Command::Lamp {
                    device: d.lamps.device,
                    with_bank: d.lamps.bank.is_some(),
                    msg: LampMsg {
                        value,
                        bitmask: d.lamps.mask,
                        bank: d.lamps.bank.unwrap_or(0),
                    },
                });
                self.set_level(i, value, now, source, out);
            }
            Spec::Relay(r) => {
                let (on, time_ms) = match op {
                    Op::On | Op::Set(1..) | Op::Max => (true, 0),
                    Op::Off | Op::Set(0) => (false, 0),
                    Op::Toggle => (state.level == 0, 0),
                    Op::OnFor(ms) => (true, ms),
                    _ => return,
                };
                let _ = out.push(Command::Relais {
                    device: r.device,
                    rollershutter: false,
                    msg: RelaisMsg {
                        number: r.channel,
                        state: if on { RELAY_ON } else { 0 },
                        time_ms,
                        bank: r.bank,
                    },
                });
                self.set_level(i, u8::from(on), now, source, out);
            }
            Spec::Shutter(s) => {
                let moving = state.movement;
                let target = match op {
                    Op::Up if moving == Movement::Up => Movement::Stopped,
                    Op::Up | Op::UpHold => Movement::Up,
                    Op::Down if moving == Movement::Down => Movement::Stopped,
                    Op::Down | Op::DownHold => Movement::Down,
                    Op::Stop => Movement::Stopped,
                    _ => return,
                };
                // Holding sends nothing while it already moves that way.
                if matches!(op, Op::UpHold | Op::DownHold) && target == moving {
                    return;
                }
                let (value, time_ms) = match target {
                    Movement::Up => (s.up, s.travel_ms),
                    Movement::Down => (s.down(), s.travel_ms),
                    Movement::Stopped => (0, 0),
                };
                let _ = out.push(Command::Relais {
                    device: s.device,
                    rollershutter: true,
                    msg: RelaisMsg {
                        number: s.channel,
                        state: value,
                        time_ms,
                        bank: s.bank,
                    },
                });
                let state = &mut self.outputs[i].state;
                state.movement = target;
                state.moving_until = now + u64::from(s.travel_ms);
                let _ = out.push(Command::OutputState { output: i as u8 });
            }
        }
    }

    /// Records output `i`'s new level and tells the motion lights on it.
    fn set_level(&mut self, i: usize, level: u8, now: u64, source: Source, out: &mut Commands) {
        let state = &mut self.outputs[i].state;
        let was = state.level;
        state.level = level;
        if level > 0 {
            state.last_on = level;
        }
        if was != level {
            let _ = out.push(Command::OutputState { output: i as u8 });
        }
        let name = &self.outputs[i].name;
        for (m, (_, config, motion)) in self.motion.iter_mut().enumerate() {
            if config.output != *name || source == Source::Motion(m) {
                continue;
            }
            if level > 0 {
                motion.switched_on();
            } else if was > 0 || matches!(motion.mode, motion::Mode::Auto { .. }) {
                motion.switched_off(config, now);
            }
        }
    }
}

/// What [`Engine::save`] keeps of an engine's state over a new
/// configuration: the outputs' levels and movements, the motion lights'
/// modes and the schedules' states, by name.
#[derive(Debug, Default)]
pub struct Saved {
    outputs: Vec<(Name, OutputKind, OutputState), MAX_OUTPUTS, u8>,
    motion: Vec<(Name, motion::Mode), MAX_MOTION_LIGHTS, u8>,
    /// With a fingerprint of the configuration ([`fingerprint`]).
    schedules: Vec<(Name, u32, schedule::State), MAX_SCHEDULES, u8>,
}

/// A 32-bit FNV-1a hash of `value`, to tell whether a configuration
/// changed without keeping a copy of it.
fn fingerprint(value: &impl core::hash::Hash) -> u32 {
    struct Fnv(u32);
    impl core::hash::Hasher for Fnv {
        fn finish(&self) -> u64 {
            u64::from(self.0)
        }
        fn write(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.0 = (self.0 ^ u32::from(b)).wrapping_mul(0x0100_0193);
            }
        }
    }
    let mut hasher = Fnv(0x811C_9DC5);
    value.hash(&mut hasher);
    hasher.0
}

impl Engine {
    /// The state worth keeping when the configuration is loaded again.
    pub fn save(&self, saved: &mut Saved) {
        saved.outputs.clear();
        saved.motion.clear();
        saved.schedules.clear();
        for (i, slot) in self.outputs.iter().enumerate() {
            if let Some(kind) = self.output_kind(i as u8) {
                let _ = saved.outputs.push((slot.name.clone(), kind, slot.state));
            }
        }
        for (name, _, state) in &self.motion {
            let _ = saved.motion.push((name.clone(), state.mode));
        }
        for (name, config, state) in &self.schedules {
            let _ = saved
                .schedules
                .push((name.clone(), fingerprint(config), *state));
        }
    }

    /// Forgets every output, motion light and schedule.
    pub fn clear(&mut self) {
        self.outputs.clear();
        self.motion.clear();
        self.schedules.clear();
    }

    /// Takes over what [`Engine::save`] kept, for what has the same name
    /// (and kind) again. A schedule only keeps its state if neither its
    /// configuration nor its enabled flag changed; a disabled motion light
    /// starts over.
    pub fn restore(&mut self, saved: &Saved) {
        for i in 0..self.outputs.len() {
            let kind = self.output_kind(i as u8);
            let slot = &mut self.outputs[i];
            if let Some((_, _, state)) = saved
                .outputs
                .iter()
                .find(|(name, k, _)| *name == slot.name && Some(*k) == kind)
            {
                slot.state = *state;
            }
        }
        for (name, _, state) in &mut self.motion {
            if let Some((_, mode)) = saved.motion.iter().find(|(n, _)| n == name) {
                if state.enabled {
                    state.mode = *mode;
                }
            }
        }
        for (name, config, state) in &mut self.schedules {
            if let Some((_, _, old)) = saved.schedules.iter().find(|(n, f, old)| {
                n == name && *f == fingerprint(config) && old.enabled == state.enabled
            }) {
                *state = *old;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::binding::{Event, Trigger};
    use crate::automation::output::{Group, LampGroup, Light};
    use crate::automation::when::tests::{cet, local, BERLIN};
    use crate::tz::TimeZone;

    const SWITCH: DeviceRef = DeviceRef {
        device_type: 4,
        id: 12,
    };
    const LAMPS: DeviceRef = DeviceRef {
        device_type: 3,
        id: 6,
    };
    const RELAYS: DeviceRef = DeviceRef {
        device_type: 5,
        id: 0x80,
    };
    const SHUTTERS: DeviceRef = DeviceRef {
        device_type: 7,
        id: 1,
    };

    fn name(s: &str) -> Name {
        Name::try_from(s).unwrap()
    }

    fn light() -> Output {
        Output::Light(Light {
            groups: Vec::from_slice(&[LampGroup {
                device: LAMPS,
                bank: None,
                mask: 0x0F,
            }])
            .unwrap(),
            levels: (1..=10)
                .map(|n| Vec::from_slice(&[n * 10]).unwrap())
                .collect(),
            default_level: 3,
        })
    }

    fn engine() -> Engine {
        let mut e = Engine::new();
        e.add_output("bad", &light()).unwrap();
        e.add_output(
            "stairs",
            &Output::Relay(Relay {
                device: RELAYS,
                channel: 9,
                bank: 0,
            }),
        )
        .unwrap();
        e.add_output(
            "wc",
            &Output::Shutter(Shutter {
                device: SHUTTERS,
                channel: 0,
                bank: 0,
                travel_ms: 20_000,
                up: 2,
            }),
        )
        .unwrap();
        e.add_output(
            "all",
            &Output::Group(Group {
                members: Vec::from_slice(&[name("bad"), name("stairs")]).unwrap(),
            }),
        )
        .unwrap();
        e
    }

    fn press(e: &mut Engine, now: u64, actions: &[Action]) -> Commands {
        let binding = Binding {
            trigger: Trigger {
                device: SWITCH,
                button: 0,
                event: Event::Single,
            },
            actions: Vec::from_slice(actions).unwrap(),
        };
        let mut out = Commands::new();
        e.button(now, &binding, &mut out);
        out
    }

    fn op(output: &str, op: Op) -> Action {
        Action::Output {
            name: name(output),
            op,
        }
    }

    fn sent(out: &Commands) -> std::vec::Vec<Command> {
        out.iter()
            .copied()
            .filter(|c| {
                !matches!(
                    c,
                    Command::OutputState { .. } | Command::AutomationState { .. }
                )
            })
            .collect()
    }

    fn level(e: &Engine, output: &str) -> u8 {
        e.output_state(e.output(output).unwrap() as u8)
            .unwrap()
            .level
    }

    #[test]
    fn light_levels_like_the_openhab_rules() {
        let mut e = engine();
        // on: the default level the first time.
        assert_eq!(
            sent(&press(&mut e, 0, &[op("bad", Op::On)])),
            [Command::Light {
                output: 0,
                level: 3
            }]
        );
        // on again: nothing (already on).
        assert!(sent(&press(&mut e, 0, &[op("bad", Op::On)])).is_empty());
        press(&mut e, 0, &[op("bad", Op::Inc)]);
        press(&mut e, 0, &[op("bad", Op::Inc)]);
        assert_eq!(level(&e, "bad"), 5);
        press(&mut e, 0, &[op("bad", Op::Off)]);
        assert_eq!(level(&e, "bad"), 0);
        // on: back to the last level.
        assert_eq!(
            sent(&press(&mut e, 0, &[op("bad", Op::On)])),
            [Command::Light {
                output: 0,
                level: 5
            }]
        );
        press(&mut e, 0, &[op("bad", Op::Max)]);
        assert_eq!(level(&e, "bad"), 10);
        press(&mut e, 0, &[op("bad", Op::Inc)]);
        assert_eq!(level(&e, "bad"), 10);
        press(&mut e, 0, &[op("bad", Op::Set(2))]);
        for _ in 0..3 {
            press(&mut e, 0, &[op("bad", Op::Dec)]);
        }
        assert_eq!(level(&e, "bad"), 0);
    }

    #[test]
    fn relays_and_groups() {
        let mut e = engine();
        let out = press(&mut e, 0, &[op("stairs", Op::On)]);
        assert_eq!(
            sent(&out),
            [Command::Relais {
                device: RELAYS,
                rollershutter: false,
                msg: RelaisMsg {
                    number: 9,
                    state: 3,
                    time_ms: 0,
                    bank: 0
                }
            }]
        );
        let out = press(&mut e, 0, &[op("all", Op::Off)]);
        assert_eq!(sent(&out).len(), 2);
        assert_eq!((level(&e, "bad"), level(&e, "stairs")), (0, 0));
        // A report from the node updates the state.
        let mut out = Commands::new();
        e.relay_state(0, RELAYS.key(), 0, 9, true, &mut out);
        assert_eq!(level(&e, "stairs"), 1);
        assert_eq!(out.as_slice(), [Command::OutputState { output: 1 }]);
    }

    #[test]
    fn shutters_toggle_to_stop_like_the_openhab_rules() {
        let mut e = engine();
        let msg = |out: &Commands| match sent(out)[..] {
            [Command::Relais {
                msg,
                rollershutter: true,
                ..
            }] => (msg.state, msg.time_ms),
            _ => panic!("{out:?}"),
        };
        // up = 2 for this one.
        assert_eq!(msg(&press(&mut e, 0, &[op("wc", Op::Up)])), (2, 20_000));
        // up while moving up: stop.
        assert_eq!(msg(&press(&mut e, 1000, &[op("wc", Op::Up)])), (0, 0));
        assert_eq!(
            msg(&press(&mut e, 2000, &[op("wc", Op::Down)])),
            (1, 20_000)
        );
        // Holding down while it moves down sends nothing.
        assert!(sent(&press(&mut e, 2500, &[op("wc", Op::DownHold)])).is_empty());
        assert_eq!(msg(&press(&mut e, 3000, &[op("wc", Op::Stop)])), (0, 0));
        // After the travel time it counts as stopped again.
        press(&mut e, 4000, &[op("wc", Op::Up)]);
        let tz = TimeZone::UTC;
        let ctx = Context {
            now: None,
            tz: &tz,
            place: None,
            lux: &|_| None,
        };
        let mut out = Commands::new();
        e.tick(24_000, &ctx, &mut out);
        assert_eq!(
            msg(&press(&mut e, 25_000, &[op("wc", Op::Up)])),
            (2, 20_000)
        );
        // The node's report wins: 1 is down for this one.
        let mut out = Commands::new();
        e.shutter_state(26_000, SHUTTERS.key(), 0, 0, 1, &mut out);
        assert_eq!(msg(&press(&mut e, 27_000, &[op("wc", Op::Down)])), (0, 0));
    }

    fn motion_engine() -> Engine {
        let mut e = engine();
        let json =
            r#"{"detectors":[{"type":4,"id":77}],"output":"bad","timeout_s":180,"lockout_s":60}"#;
        let (m, _): (MotionLight, _) = serde_json_core::from_str(json).unwrap();
        e.add_motion("bad-pir", m, true).unwrap();
        e
    }

    const PIR: u32 = 0x1004_4D00;

    #[test]
    fn motion_light_with_manual_override_and_triple_press_off() {
        let tz = TimeZone::UTC;
        let ctx = Context {
            now: None,
            tz: &tz,
            place: None,
            lux: &|_| None,
        };
        let mut e = motion_engine();
        let mut out = Commands::new();
        e.motion(0, &ctx, PIR, &mut out);
        assert_eq!(
            sent(&out),
            [Command::Light {
                output: 0,
                level: 3
            }]
        );
        // Off after 3 minutes.
        let mut out = Commands::new();
        e.tick(180_000, &ctx, &mut out);
        assert_eq!(
            sent(&out),
            [Command::Light {
                output: 0,
                level: 0
            }]
        );
        // Locked for a minute, then motion again; then on by hand: no timer.
        let mut out = Commands::new();
        e.motion(200_000, &ctx, PIR, &mut out);
        assert!(out.is_empty());
        e.tick(240_000, &ctx, &mut out);
        e.motion(241_000, &ctx, PIR, &mut out);
        press(&mut e, 242_000, &[op("bad", Op::Max)]);
        assert_eq!(e.motion_mode(0), Some(motion::Mode::Manual));
        let mut out = Commands::new();
        e.tick(10_000_000, &ctx, &mut out);
        assert!(sent(&out).is_empty());
        // Off by hand: lockout.
        press(&mut e, 10_000_000, &[op("bad", Op::Off)]);
        assert!(matches!(
            e.motion_mode(0),
            Some(motion::Mode::Lockout { .. })
        ));
        // Triple down disables it, triple up enables it.
        let out = press(
            &mut e,
            10_100_000,
            &[Action::Automation {
                name: name("bad-pir"),
                switch: Switch::Disable,
            }],
        );
        assert_eq!(
            out.as_slice(),
            [Command::AutomationState {
                kind: Kind::Motion,
                index: 0
            }]
        );
        let mut out = Commands::new();
        e.motion(10_200_000, &ctx, PIR, &mut out);
        assert!(out.is_empty());
        let mut out = Commands::new();
        assert_eq!(
            e.switch_automation("bad-pir", Switch::Toggle, &mut out),
            Some(true)
        );
        e.motion(10_300_000, &ctx, PIR, &mut out);
        assert!(sent(&out).contains(&Command::Light {
            output: 0,
            level: 10
        }));
    }

    #[test]
    fn schedules_switch_their_output() {
        let tz = cet();
        let mut e = engine();
        let json = r#"{"output":"stairs","windows":[{"start":{"sun":{"event":"civil_dusk","offset_min":-30}},
            "end":[{"at":{"clock":{"hour":23,"minute":0}}}]}]}"#;
        let (s, _): (Schedule, _) = serde_json_core::from_str(json).unwrap();
        e.add_schedule("aussen", s, true).unwrap();
        let ctx = |utc| Context {
            now: Some(utc),
            tz: &tz,
            place: Some(BERLIN),
            lux: &|_| None,
        };
        let mut out = Commands::new();
        e.tick(0, &ctx(local(&tz, 2026, 12, 21, 12, 0)), &mut out);
        assert!(out.is_empty());
        let (_, next) = e.deadline(&ctx(local(&tz, 2026, 12, 21, 12, 0)));
        let at = tz.to_utc(next.unwrap());
        e.tick(1, &ctx(at), &mut out);
        assert_eq!(level(&e, "stairs"), 1);
        let mut out = Commands::new();
        e.tick(2, &ctx(local(&tz, 2026, 12, 21, 23, 0)), &mut out);
        assert_eq!(level(&e, "stairs"), 0);
    }

    #[test]
    fn configuration_errors() {
        let mut e = engine();
        assert_eq!(e.add_output("bad", &light()).unwrap_err().field, "name");
        let group = Output::Group(Group {
            members: Vec::from_slice(&[name("nope")]).unwrap(),
        });
        assert_eq!(e.add_output("g", &group).unwrap_err().field, "members");
        let json =
            r#"{"detectors":[{"type":4,"id":77}],"output":"nope","timeout_s":180,"lockout_s":60}"#;
        let (m, _): (MotionLight, _) = serde_json_core::from_str(json).unwrap();
        assert_eq!(e.add_motion("x", m, true).unwrap_err().field, "output");
    }

    #[test]
    fn reloading_keeps_the_state_by_name() {
        let mut e = motion_engine();
        press(&mut e, 0, &[op("bad", Op::Set(4))]);
        press(&mut e, 0, &[op("stairs", Op::On)]);
        assert_eq!(e.motion_mode(0), Some(motion::Mode::Manual));
        let mut saved = Saved::default();
        e.save(&mut saved);
        e.clear();
        assert_eq!(e.counts(), (0, 0, 0));
        // Again, but "stairs" is a light now and "wc" is gone.
        e.add_output("bad", &light()).unwrap();
        e.add_output("stairs", &light()).unwrap();
        let json =
            r#"{"detectors":[{"type":4,"id":77}],"output":"bad","timeout_s":180,"lockout_s":60}"#;
        let (m, _): (MotionLight, _) = serde_json_core::from_str(json).unwrap();
        e.add_motion("bad-pir", m, true).unwrap();
        e.restore(&saved);
        assert_eq!(level(&e, "bad"), 4);
        assert_eq!(level(&e, "stairs"), 0);
        assert_eq!(e.motion_mode(0), Some(motion::Mode::Manual));
        assert_eq!(e.output_kind(1), Some(OutputKind::Light));
    }

    #[test]
    fn saved_state_is_small() {
        let size = core::mem::size_of::<Saved>();
        assert!(size < 4 * 1024, "{size}");
    }

    #[test]
    fn ram_stays_small() {
        let size = core::mem::size_of::<Engine>();
        // Outputs, motion lights and schedules at their limits.
        assert!(size < 8 * 1024, "{size}");
    }
}
