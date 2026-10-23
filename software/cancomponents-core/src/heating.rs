//! Control logic of the `HeatingControl` device: a tank with a 3-stage
//! heating rod, two 3-way valves that switch the heating circuit between
//! direct heat-pump operation and tank operation, a circuit pump, and a
//! motorised mixer.
//!
//! Everything in here is pure: the caller passes the current [`Instant`] and
//! the latest inputs to [`HeatingController::step`] and applies the returned
//! [`HeatingOutputs`] to the relays. No clock, no I/O, so the whole thing is
//! tested on the host.
//!
//! # Relay assignment
//!
//! | Relays | Function |
//! |---|---|
//! | 0-5 | Heating rod, always switched as pairs (0+1, 2+3, 4+5) |
//! | 6, 7 | 3-way valves (flow and return), always switched together |
//! | 8, 9 | Mixer: 8 = towards hot, 9 = towards cold (more return water) |
//! | 10 | Heating circuit pump |
//! | 11 | Hot-water circuit pump: never driven by the controller, switched only by CAN/MQTT relay commands (to pre-pump hot water to the fresh-water station) |
//!
//! # Heating rod
//!
//! Each stage adds 3 kW: stage 1 = relays 0+1, stage 2 = 0-3, stage 3 = 0-5.
//! Stage *n* starts at *n* × 3000 W + 300 W and stops below *n* × 3000 W.
//! In normal operation the stage follows the available (feed-in) power, as
//! long as the rod is released and the tank isn't full yet. In emergency
//! operation the stage is dictated and the available power is ignored.
//! **Anything that makes continued heating unsafe or pointless switches
//! the rod off immediately**: the 80 °C hard limit, a missing tank sensor,
//! stale inputs (no message for 30 s), a withdrawn release, a full tank.
//! Only the *discretionary* moves — starting up again and changing between
//! stages with the power — wait for a minimum dwell time, to spare the
//! relays.
//!
//! The independent hardware protection (a contactor at 85 °C, the STB at
//! 90 °C) is not part of this software and must not be relied upon being
//! unnecessary.

use embassy_time::{Duration, Instant};

/// Relays 0-5: the heating rod, as three pairs.
pub const ROD_RELAYS: [usize; 6] = [0, 1, 2, 3, 4, 5];
/// Relays 6 and 7: the flow and return 3-way valve.
pub const VALVE_RELAYS: [usize; 2] = [6, 7];
/// Relay 8: mixer towards hot.
pub const MIXER_HOT_RELAY: usize = 8;
/// Relay 9: mixer towards cold.
pub const MIXER_COLD_RELAY: usize = 9;
/// Relay 10: heating circuit pump.
pub const CIRCUIT_PUMP_RELAY: usize = 10;
/// Highest relay number the controller drives.
pub const LAST_CONTROLLED_RELAY: usize = 10;

/// Heating rod power added by each stage.
pub const STAGE_POWER_W: i32 = 3000;
/// The rod has this many stages.
pub const MAX_STAGE: u8 = 3;
/// Tank temperature sensors, ordered top to bottom.
pub const TANK_SENSORS: usize = 7;

/// True for relays that belong to the controller. A `HeatingControl` node
/// ignores CAN relay commands for these, so nothing else can switch the rod
/// (or the valves and mixer) behind the controller's back.
pub const fn is_controlled_relay(num: usize) -> bool {
    num <= LAST_CONTROLLED_RELAY
}

/// Which of the six rod relays are on in `stage` (anything above
/// [`MAX_STAGE`] counts as the top stage).
pub const fn rod_relays_on(stage: u8) -> [bool; 6] {
    let pairs = if stage > MAX_STAGE {
        MAX_STAGE as usize
    } else {
        stage as usize
    };
    [
        pairs >= 1,
        pairs >= 1,
        pairs >= 2,
        pairs >= 2,
        pairs >= 3,
        pairs >= 3,
    ]
}

fn age(now: Instant, then: Instant) -> Duration {
    now.checked_duration_since(then).unwrap_or(Duration::MIN)
}

/// Available power, as last received.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnergyInput {
    /// Surplus (feed-in) power in watts.
    pub power_w: i32,
    /// The rod is released at all.
    pub rod_enabled: bool,
    pub received: Instant,
}

/// Emergency operation, as last received.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmergencyInput {
    pub stage: u8,
    pub tank_target_mc: i32,
    pub received: Instant,
}

/// Heating circuit request, as last received.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetInput {
    pub flow_target_mc: i32,
    pub tank_mode: bool,
    pub received: Instant,
}

/// Why the rod is in its current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RodReason {
    /// No reason to heat (no surplus, or the rod isn't released).
    Off,
    /// Following the surplus power.
    Surplus,
    /// Emergency operation.
    Emergency,
    /// A tank sensor reached the hard limit.
    OverTemperature,
    /// No usable tank sensor, so nothing can be regulated safely.
    NoTankSensor,
    /// The tank reached its target.
    TankFull,
    /// The power message stopped arriving.
    InputsStale,
}

/// Tuning of the [`RodController`].
#[derive(Debug, Clone, Copy)]
pub struct RodConfig {
    pub stage_power_w: i32,
    /// Stage *n* switches on at `n × stage_power_w + stage_hysteresis_w` and
    /// off again below `n × stage_power_w`, so a stage never starts on a
    /// surplus it couldn't keep running on.
    pub stage_hysteresis_w: i32,
    /// Minimum time between two discretionary stage changes.
    pub min_dwell: Duration,
    /// Inputs older than this count as missing.
    pub input_timeout: Duration,
    /// Any tank sensor at or above this switches the rod off.
    pub hard_limit_mc: i32,
    /// The over-temperature latch releases this far below the limit.
    pub hard_limit_hysteresis_mc: i32,
    /// Heating resumes this far below the tank target.
    pub tank_hysteresis_mc: i32,
}

impl RodConfig {
    pub const fn new() -> Self {
        Self {
            stage_power_w: STAGE_POWER_W,
            stage_hysteresis_w: 300,
            min_dwell: Duration::from_secs(120),
            input_timeout: Duration::from_secs(30),
            hard_limit_mc: 80_000,
            hard_limit_hysteresis_mc: 5_000,
            tank_hysteresis_mc: 2_000,
        }
    }
}

impl Default for RodConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything the rod decision depends on.
#[derive(Debug, Clone, Copy)]
pub struct RodInputs {
    pub energy: Option<EnergyInput>,
    pub emergency: Option<EmergencyInput>,
    /// The hottest valid tank sensor; `None` if there is none.
    pub tank_max_mc: Option<i32>,
    /// The sensor the charge target is compared with; `None` if it's not
    /// configured or not delivering.
    pub tank_ref_mc: Option<i32>,
    /// Charge target for surplus operation.
    pub tank_target_mc: i32,
}

/// Decides the rod stage (0-3).
#[derive(Debug, Clone)]
pub struct RodController {
    cfg: RodConfig,
    stage: u8,
    last_change: Option<Instant>,
    over_temperature: bool,
    tank_full: bool,
    reason: RodReason,
}

impl RodController {
    pub const fn new(cfg: RodConfig) -> Self {
        Self {
            cfg,
            stage: 0,
            last_change: None,
            over_temperature: false,
            tank_full: false,
            reason: RodReason::Off,
        }
    }

    pub fn stage(&self) -> u8 {
        self.stage
    }

    pub fn reason(&self) -> RodReason {
        self.reason
    }

    pub fn over_temperature(&self) -> bool {
        self.over_temperature
    }

    /// The stage the power asks for, given the stage we're at now.
    fn stage_for_power(&self, power_w: i32) -> u8 {
        for n in (1..=MAX_STAGE).rev() {
            let mut threshold = i32::from(n) * self.cfg.stage_power_w;
            if self.stage < n {
                threshold += self.cfg.stage_hysteresis_w;
            }
            if power_w >= threshold {
                return n;
            }
        }
        0
    }

    fn update_tank_full(&mut self, reference_mc: i32, target_mc: i32) {
        if reference_mc >= target_mc {
            self.tank_full = true;
        } else if reference_mc <= target_mc - self.cfg.tank_hysteresis_mc {
            self.tank_full = false;
        }
    }

    pub fn step(&mut self, now: Instant, inputs: &RodInputs) -> u8 {
        if let Some(hottest) = inputs.tank_max_mc {
            if hottest >= self.cfg.hard_limit_mc {
                self.over_temperature = true;
            } else if hottest <= self.cfg.hard_limit_mc - self.cfg.hard_limit_hysteresis_mc {
                self.over_temperature = false;
            }
        }
        let fresh = |received: Instant| age(now, received) < self.cfg.input_timeout;

        let emergency = inputs
            .emergency
            .filter(|e| fresh(e.received) && e.stage > 0);
        let energy = inputs.energy.filter(|e| fresh(e.received));

        // (stage, reason, is_emergency)
        let (desired, reason, dictated) = if self.over_temperature {
            (0, RodReason::OverTemperature, false)
        } else if inputs.tank_max_mc.is_none() {
            (0, RodReason::NoTankSensor, false)
        } else if let Some(emergency) = emergency {
            match inputs.tank_ref_mc {
                None => (0, RodReason::NoTankSensor, false),
                Some(reference) => {
                    self.update_tank_full(reference, emergency.tank_target_mc);
                    if self.tank_full {
                        (0, RodReason::TankFull, false)
                    } else {
                        (emergency.stage.min(MAX_STAGE), RodReason::Emergency, true)
                    }
                }
            }
        } else if let Some(energy) = energy {
            if !energy.rod_enabled {
                (0, RodReason::Off, false)
            } else {
                match inputs.tank_ref_mc {
                    None => (0, RodReason::NoTankSensor, false),
                    Some(reference) => {
                        self.update_tank_full(reference, inputs.tank_target_mc);
                        if self.tank_full {
                            (0, RodReason::TankFull, false)
                        } else {
                            let stage = self.stage_for_power(energy.power_w);
                            let reason = if stage > 0 {
                                RodReason::Surplus
                            } else {
                                RodReason::Off
                            };
                            (stage, reason, false)
                        }
                    }
                }
            }
        } else {
            (0, RodReason::InputsStale, false)
        };
        self.reason = reason;

        let dwell_elapsed = self
            .last_change
            .is_none_or(|changed| age(now, changed) >= self.cfg.min_dwell);
        let new_stage = if desired == self.stage {
            self.stage
        } else if desired == 0 || dictated || dwell_elapsed {
            desired
        } else {
            self.stage
        };
        if new_stage != self.stage {
            self.stage = new_stage;
            self.last_change = Some(now);
        }
        self.stage
    }
}

/// Valve and circuit pump outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CircuitOutputs {
    /// Both 3-way valves in tank operation (otherwise: direct heat-pump
    /// operation).
    pub valves_tank: bool,
    pub pump: bool,
}

/// Switches the valves for tank operation and runs the circuit pump with a
/// delay after them, but stops it together with them.
#[derive(Debug, Clone)]
pub struct CircuitController {
    pump_delay: Duration,
    valves_since: Option<Instant>,
}

impl CircuitController {
    pub const fn new(pump_delay: Duration) -> Self {
        Self {
            pump_delay,
            valves_since: None,
        }
    }

    pub fn step(&mut self, now: Instant, tank_mode: bool) -> CircuitOutputs {
        if !tank_mode {
            self.valves_since = None;
            return CircuitOutputs {
                valves_tank: false,
                pump: false,
            };
        }
        let since = *self.valves_since.get_or_insert(now);
        CircuitOutputs {
            valves_tank: true,
            pump: age(now, since) >= self.pump_delay,
        }
    }
}

/// Tells whether the tank is hot enough to supply the heating circuit.
#[derive(Debug, Clone)]
pub struct TankAvailability {
    usable: bool,
}

impl TankAvailability {
    /// Usable once the top of the tank is this far above the flow target…
    const ON_MARGIN_MC: i32 = 3_000;
    /// …and unusable again once it falls this close to it.
    const OFF_MARGIN_MC: i32 = 1_000;

    pub const fn new() -> Self {
        Self { usable: false }
    }

    pub fn usable(&self) -> bool {
        self.usable
    }

    pub fn update(&mut self, tank_top_mc: Option<i32>, flow_target_mc: i32) -> bool {
        self.usable = match tank_top_mc {
            None => false,
            Some(top) if top >= flow_target_mc + Self::ON_MARGIN_MC => true,
            Some(top) if top < flow_target_mc + Self::OFF_MARGIN_MC => false,
            Some(_) => self.usable,
        };
        self.usable
    }
}

impl Default for TankAvailability {
    fn default() -> Self {
        Self::new()
    }
}

/// What the mixer should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerRequest {
    /// Stay where it is.
    Hold,
    /// Drive to the hot end (no return water mixed in).
    FullHot,
    /// Drive to the cold end: all return water mixed in. The safe position,
    /// and where the mixer is when it isn't part of the circuit.
    FullCold,
    /// Regulate the flow temperature to the target.
    Regulate {
        target_mc: i32,
        flow_mc: Option<i32>,
    },
}

/// Which mixer relay is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerDrive {
    Stop,
    Hot,
    Cold,
}

/// Tuning of the [`MixerController`].
#[derive(Debug, Clone, Copy)]
pub struct MixerConfig {
    /// Full travel of the actuator (VRG131 with ARA661: 90° in 120 s).
    pub travel: Duration,
    /// How long to drive towards cold at start-up to be sure of reaching the
    /// end stop.
    pub homing: Duration,
    /// No movement while the flow is this close to the target.
    pub deadband_mc: i32,
    /// Drive time per kelvin of control error.
    pub pulse_per_kelvin: Duration,
    pub min_pulse: Duration,
    pub max_pulse: Duration,
    /// Wait after each pulse, for the flow sensor to follow.
    pub settle: Duration,
    /// Both relays stay off at least this long when the direction changes.
    pub reversal_pause: Duration,
    /// Extra drive time when going to an end, so that a drifting position
    /// estimate gets corrected by the end stop.
    pub end_margin: Duration,
    /// Re-drive to an end stop after this long without having been there, as
    /// long as that end is still the one asked for.
    pub resync_after: Duration,
}

impl MixerConfig {
    pub const fn new() -> Self {
        Self {
            travel: Duration::from_secs(120),
            homing: Duration::from_secs(130),
            deadband_mc: 500,
            pulse_per_kelvin: Duration::from_secs(3),
            min_pulse: Duration::from_secs(1),
            max_pulse: Duration::from_secs(15),
            settle: Duration::from_secs(60),
            reversal_pause: Duration::from_secs(1),
            end_margin: Duration::from_secs(10),
            resync_after: Duration::from_secs(6 * 60 * 60),
        }
    }
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
enum MixerState {
    /// Nothing happened yet; the first step starts homing.
    Start,
    Homing {
        until: Instant,
    },
    Driving {
        hot: bool,
        started: Instant,
        until: Instant,
        /// The drive aims for the end stop, which then defines the position.
        to_end: bool,
    },
    /// Both relays off, until `until`.
    Pause {
        until: Instant,
    },
    Idle,
}

/// Three-point controller for the mixer motor. It has no position feedback,
/// so it keeps an estimate of the position from the drive times, and
/// corrects it whenever it drives to an end stop.
#[derive(Debug, Clone)]
pub struct MixerController {
    cfg: MixerConfig,
    state: MixerState,
    /// Position in milliseconds of travel; 0 = cold end, `travel` = hot end.
    position_ms: i64,
    last_hot_sync: Instant,
    last_cold_sync: Instant,
}

impl MixerController {
    pub const fn new(cfg: MixerConfig) -> Self {
        Self {
            cfg,
            state: MixerState::Start,
            position_ms: 0,
            last_hot_sync: Instant::MIN,
            last_cold_sync: Instant::MIN,
        }
    }

    fn travel_ms(&self) -> i64 {
        self.cfg.travel.as_millis() as i64
    }

    /// Estimated position, 0 % = cold end, 100 % = hot end.
    pub fn position_percent(&self) -> u8 {
        (self.position_ms * 100 / self.travel_ms().max(1)).clamp(0, 100) as u8
    }

    /// True while the start-up reference run (to the cold end) is going.
    pub fn homing(&self) -> bool {
        matches!(self.state, MixerState::Start | MixerState::Homing { .. })
    }

    fn start_drive(&mut self, now: Instant, hot: bool, duration: Duration, to_end: bool) {
        self.state = MixerState::Driving {
            hot,
            started: now,
            until: now + duration,
            to_end,
        };
    }

    pub fn step(&mut self, now: Instant, request: MixerRequest) -> MixerDrive {
        match self.state {
            MixerState::Start => {
                self.state = MixerState::Homing {
                    until: now + self.cfg.homing,
                };
                MixerDrive::Cold
            }
            MixerState::Homing { until } => {
                if now >= until {
                    self.position_ms = 0;
                    self.last_cold_sync = now;
                    self.state = MixerState::Pause {
                        until: now + self.cfg.reversal_pause,
                    };
                    MixerDrive::Stop
                } else {
                    MixerDrive::Cold
                }
            }
            MixerState::Driving {
                hot,
                started,
                until,
                to_end,
            } => {
                if now < until {
                    return if hot {
                        MixerDrive::Hot
                    } else {
                        MixerDrive::Cold
                    };
                }
                let moved = age(until, started).as_millis() as i64;
                let travel = self.travel_ms();
                self.position_ms = if to_end {
                    if hot {
                        travel
                    } else {
                        0
                    }
                } else if hot {
                    (self.position_ms + moved).min(travel)
                } else {
                    (self.position_ms - moved).max(0)
                };
                if to_end {
                    if hot {
                        self.last_hot_sync = now;
                    } else {
                        self.last_cold_sync = now;
                    }
                }
                let pause = if to_end {
                    self.cfg.reversal_pause
                } else {
                    self.cfg.settle
                };
                self.state = MixerState::Pause { until: now + pause };
                MixerDrive::Stop
            }
            MixerState::Pause { until } => {
                if now >= until {
                    self.state = MixerState::Idle;
                    return self.step_idle(now, request);
                }
                MixerDrive::Stop
            }
            MixerState::Idle => self.step_idle(now, request),
        }
    }

    fn step_idle(&mut self, now: Instant, request: MixerRequest) -> MixerDrive {
        let travel = self.travel_ms();
        match request {
            MixerRequest::Hold => MixerDrive::Stop,
            MixerRequest::FullHot => {
                let stale = age(now, self.last_hot_sync) >= self.cfg.resync_after;
                if self.position_ms < travel || stale {
                    let remaining = Duration::from_millis((travel - self.position_ms) as u64);
                    self.start_drive(now, true, remaining + self.cfg.end_margin, true);
                    MixerDrive::Hot
                } else {
                    MixerDrive::Stop
                }
            }
            MixerRequest::FullCold => {
                let stale = age(now, self.last_cold_sync) >= self.cfg.resync_after;
                if self.position_ms > 0 || stale {
                    let remaining = Duration::from_millis(self.position_ms as u64);
                    self.start_drive(now, false, remaining + self.cfg.end_margin, true);
                    MixerDrive::Cold
                } else {
                    MixerDrive::Stop
                }
            }
            MixerRequest::Regulate {
                target_mc,
                flow_mc: None,
            } => {
                let _ = target_mc;
                MixerDrive::Stop
            }
            MixerRequest::Regulate {
                target_mc,
                flow_mc: Some(flow_mc),
            } => {
                let error = target_mc - flow_mc;
                if error.abs() <= self.cfg.deadband_mc {
                    return MixerDrive::Stop;
                }
                let hot = error > 0;
                let available_ms = if hot {
                    travel - self.position_ms
                } else {
                    self.position_ms
                };
                let wanted_ms = i64::from(error.unsigned_abs())
                    * self.cfg.pulse_per_kelvin.as_millis() as i64
                    / 1000;
                let pulse_ms = wanted_ms
                    .clamp(
                        self.cfg.min_pulse.as_millis() as i64,
                        self.cfg.max_pulse.as_millis() as i64,
                    )
                    .min(available_ms);
                if pulse_ms < self.cfg.min_pulse.as_millis() as i64 {
                    // At the end stop already, nothing left to give.
                    return MixerDrive::Stop;
                }
                self.start_drive(now, hot, Duration::from_millis(pulse_ms as u64), false);
                if hot {
                    MixerDrive::Hot
                } else {
                    MixerDrive::Cold
                }
            }
        }
    }
}

/// Everything the controller needs, gathered by the caller.
#[derive(Debug, Clone, Copy)]
pub struct HeatingInputs {
    pub target: Option<TargetInput>,
    pub energy: Option<EnergyInput>,
    pub emergency: Option<EmergencyInput>,
    /// Tank sensors top to bottom; `None` for a position without a working
    /// sensor.
    pub tank_temps_mc: [Option<i32>; TANK_SENSORS],
    /// Position (in `tank_temps_mc`) of the sensor the charge target is
    /// compared with.
    pub tank_ref_index: usize,
    /// Flow temperature of the heating circuit.
    pub flow_mc: Option<i32>,
    /// Charge target for surplus operation.
    pub tank_target_mc: i32,
}

/// Where the relays should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatingOutputs {
    pub rod_stage: u8,
    pub valves_tank: bool,
    pub pump: bool,
    pub mixer: MixerDrive,
}

impl HeatingOutputs {
    /// The safe state: everything off.
    pub const OFF: Self = Self {
        rod_stage: 0,
        valves_tank: false,
        pump: false,
        mixer: MixerDrive::Stop,
    };
}

/// For the status display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeatingStatus {
    pub rod_reason: RodReason,
    pub over_temperature: bool,
    pub tank_usable: bool,
    pub mixer_percent: u8,
    pub mixer_homing: bool,
}

/// The whole heating controller.
#[derive(Debug, Clone)]
pub struct HeatingController {
    input_timeout: Duration,
    rod: RodController,
    circuit: CircuitController,
    availability: TankAvailability,
    mixer: MixerController,
}

impl HeatingController {
    pub const fn new() -> Self {
        let rod = RodConfig::new();
        Self {
            input_timeout: rod.input_timeout,
            rod: RodController::new(rod),
            circuit: CircuitController::new(Duration::from_secs(120)),
            availability: TankAvailability::new(),
            mixer: MixerController::new(MixerConfig::new()),
        }
    }

    pub fn status(&self) -> HeatingStatus {
        HeatingStatus {
            rod_reason: self.rod.reason(),
            over_temperature: self.rod.over_temperature(),
            tank_usable: self.availability.usable(),
            mixer_percent: self.mixer.position_percent(),
            mixer_homing: self.mixer.homing(),
        }
    }

    pub fn step(&mut self, now: Instant, inputs: &HeatingInputs) -> HeatingOutputs {
        let target = inputs
            .target
            .filter(|t| age(now, t.received) < self.input_timeout);

        let hottest = inputs.tank_temps_mc.iter().flatten().copied().max();
        let reference = inputs
            .tank_temps_mc
            .get(inputs.tank_ref_index)
            .copied()
            .flatten();
        let rod_stage = self.rod.step(
            now,
            &RodInputs {
                energy: inputs.energy,
                emergency: inputs.emergency,
                tank_max_mc: hottest,
                tank_ref_mc: reference,
                tank_target_mc: inputs.tank_target_mc,
            },
        );

        let tank_mode = target.is_some_and(|t| t.tank_mode);
        let circuit = self.circuit.step(now, tank_mode);

        // The top of the tank is what feeds the circuit; if that sensor is
        // gone, the next one down is the cautious stand-in.
        let tank_top = inputs.tank_temps_mc.iter().flatten().next().copied();
        let mixer_request = match target {
            // Not part of the circuit, or nobody to tell what to do: all the
            // way to return, the safe end.
            None => MixerRequest::FullCold,
            Some(t) if !t.tank_mode => MixerRequest::FullCold,
            Some(t) => {
                if self.availability.update(tank_top, t.flow_target_mc) {
                    MixerRequest::Regulate {
                        target_mc: t.flow_target_mc,
                        flow_mc: inputs.flow_mc,
                    }
                } else {
                    // Tank too cold to regulate with: let everything through.
                    MixerRequest::FullHot
                }
            }
        };
        let mixer = self.mixer.step(now, mixer_request);

        HeatingOutputs {
            rod_stage,
            valves_tank: circuit.valves_tank,
            pump: circuit.pump,
            mixer,
        }
    }
}

impl Default for HeatingController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: u64) -> Instant {
        Instant::from_secs(secs)
    }

    #[test]
    fn rod_relays_are_always_switched_in_pairs() {
        assert_eq!(rod_relays_on(0), [false; 6]);
        assert_eq!(rod_relays_on(1), [true, true, false, false, false, false]);
        assert_eq!(rod_relays_on(2), [true, true, true, true, false, false]);
        assert_eq!(rod_relays_on(3), [true; 6]);
        assert_eq!(rod_relays_on(9), [true; 6]);
        for stage in 0..=4 {
            let on = rod_relays_on(stage);
            for pair in on.chunks(2) {
                assert_eq!(pair[0], pair[1]);
            }
        }
    }

    #[test]
    fn controller_relays_are_0_to_10() {
        assert!(is_controlled_relay(0));
        assert!(is_controlled_relay(10));
        assert!(!is_controlled_relay(11));
    }

    // ---- heating rod -------------------------------------------------

    fn energy(power_w: i32, received: u64) -> Option<EnergyInput> {
        Some(EnergyInput {
            power_w,
            rod_enabled: true,
            received: t(received),
        })
    }

    /// Cool tank, 60 °C charge target, sensors fine.
    fn rod_inputs(energy: Option<EnergyInput>) -> RodInputs {
        RodInputs {
            energy,
            emergency: None,
            tank_max_mc: Some(40_000),
            tank_ref_mc: Some(35_000),
            tank_target_mc: 60_000,
        }
    }

    fn rod() -> RodController {
        RodController::new(RodConfig::new())
    }

    #[test]
    fn surplus_selects_the_stage() {
        let mut rod = rod();
        // A stage starts 300 W above its nominal power.
        assert_eq!(rod.step(t(0), &rod_inputs(energy(3_299, 0))), 0);
        assert_eq!(rod.step(t(5), &rod_inputs(energy(3_300, 5))), 1);
        assert_eq!(rod.reason(), RodReason::Surplus);
    }

    #[test]
    fn higher_stages_wait_for_the_dwell_time() {
        let mut rod = rod();
        assert_eq!(rod.step(t(0), &rod_inputs(energy(3_300, 0))), 1);
        // 6.6 kW would be stage 2, but stage 1 was just entered.
        assert_eq!(rod.step(t(5), &rod_inputs(energy(6_600, 5))), 1);
        assert_eq!(rod.step(t(119), &rod_inputs(energy(6_600, 119))), 1);
        assert_eq!(rod.step(t(120), &rod_inputs(energy(6_600, 120))), 2);
        // Stage 2 needs 6.3 kW, stage 3 9.3 kW.
        assert_eq!(rod.step(t(240), &rod_inputs(energy(9_299, 240))), 2);
        assert_eq!(rod.step(t(241), &rod_inputs(energy(9_300, 241))), 3);
    }

    #[test]
    fn a_running_stage_stays_on_down_to_its_nominal_power() {
        let mut rod = rod();
        assert_eq!(rod.step(t(0), &rod_inputs(energy(3_200, 0))), 0);
        assert_eq!(rod.step(t(5), &rod_inputs(energy(3_300, 5))), 1);
        // Once on, 3 kW is enough to keep stage 1 running…
        assert_eq!(rod.step(t(10), &rod_inputs(energy(3_000, 10))), 1);
        // …and below that it switches off at once.
        assert_eq!(rod.step(t(20), &rod_inputs(energy(2_999, 20))), 0);
    }

    #[test]
    fn dropping_to_a_lower_stage_waits_but_dropping_to_zero_does_not() {
        let mut rod = rod();
        rod.step(t(0), &rod_inputs(energy(3_300, 0)));
        assert_eq!(rod.step(t(120), &rod_inputs(energy(9_900, 120))), 3);
        // Power falls to stage 1 level: held at 3 until the dwell is over…
        assert_eq!(rod.step(t(125), &rod_inputs(energy(3_500, 125))), 3);
        assert_eq!(rod.step(t(239), &rod_inputs(energy(3_500, 239))), 3);
        assert_eq!(rod.step(t(240), &rod_inputs(energy(3_500, 240))), 1);
        // …but running out of surplus switches off at once.
        assert_eq!(rod.step(t(245), &rod_inputs(energy(100, 245))), 0);
    }

    #[test]
    fn restarting_after_a_shutdown_waits_for_the_dwell_time() {
        let mut rod = rod();
        rod.step(t(0), &rod_inputs(energy(3_300, 0)));
        assert_eq!(rod.step(t(10), &rod_inputs(energy(0, 10))), 0);
        assert_eq!(rod.step(t(20), &rod_inputs(energy(3_300, 20))), 0);
        assert_eq!(rod.step(t(130), &rod_inputs(energy(3_300, 130))), 1);
    }

    #[test]
    fn no_messages_for_30_seconds_switches_everything_off() {
        let mut rod = rod();
        rod.step(t(0), &rod_inputs(energy(9_500, 0)));
        assert_eq!(rod.step(t(129), &rod_inputs(energy(9_500, 100))), 3);
        // The last message is 30 s old now.
        assert_eq!(rod.step(t(130), &rod_inputs(energy(9_500, 100))), 0);
        assert_eq!(rod.reason(), RodReason::InputsStale);
        // And without any message ever.
        let mut rod = RodController::new(RodConfig::new());
        assert_eq!(rod.step(t(0), &rod_inputs(None)), 0);
    }

    #[test]
    fn a_withdrawn_release_switches_off_immediately() {
        let mut rod = rod();
        rod.step(t(0), &rod_inputs(energy(3_300, 0)));
        let mut released = energy(9_000, 5);
        released.as_mut().unwrap().rod_enabled = false;
        assert_eq!(rod.step(t(5), &rod_inputs(released)), 0);
    }

    #[test]
    fn hard_limit_switches_off_and_latches_with_hysteresis() {
        let mut rod = rod();
        rod.step(t(0), &rod_inputs(energy(9_500, 0)));
        let mut hot = rod_inputs(energy(9_500, 10));
        hot.tank_max_mc = Some(80_000);
        assert_eq!(rod.step(t(10), &hot), 0);
        assert_eq!(rod.reason(), RodReason::OverTemperature);
        assert!(rod.over_temperature());

        // Just below the limit is not enough…
        hot.tank_max_mc = Some(79_000);
        hot.energy = energy(9_500, 200);
        assert_eq!(rod.step(t(200), &hot), 0);
        // …5 K below is.
        hot.tank_max_mc = Some(75_000);
        hot.energy = energy(9_500, 210);
        assert_eq!(rod.step(t(210), &hot), 3);
        assert!(!rod.over_temperature());
    }

    #[test]
    fn hard_limit_also_stops_emergency_operation() {
        let mut rod = rod();
        let mut inputs = rod_inputs(None);
        inputs.emergency = Some(EmergencyInput {
            stage: 3,
            tank_target_mc: 70_000,
            received: t(0),
        });
        assert_eq!(rod.step(t(0), &inputs), 3);
        inputs.tank_max_mc = Some(81_000);
        assert_eq!(rod.step(t(1), &inputs), 0);
    }

    #[test]
    fn without_a_tank_sensor_the_rod_stays_off() {
        let mut rod = rod();
        let mut inputs = rod_inputs(energy(9_500, 0));
        inputs.tank_max_mc = None;
        inputs.tank_ref_mc = None;
        assert_eq!(rod.step(t(0), &inputs), 0);
        assert_eq!(rod.reason(), RodReason::NoTankSensor);

        // Only the reference sensor missing is just as unsafe to regulate.
        let mut inputs = rod_inputs(energy(9_500, 5));
        inputs.tank_ref_mc = None;
        assert_eq!(rod.step(t(5), &inputs), 0);
        assert_eq!(rod.reason(), RodReason::NoTankSensor);
    }

    #[test]
    fn a_full_tank_stops_the_rod_until_it_has_cooled_a_little() {
        let mut rod = rod();
        assert_eq!(rod.step(t(0), &rod_inputs(energy(9_500, 0))), 3);
        let mut full = rod_inputs(energy(9_500, 10));
        full.tank_ref_mc = Some(60_000);
        assert_eq!(rod.step(t(10), &full), 0);
        assert_eq!(rod.reason(), RodReason::TankFull);

        full.energy = energy(9_500, 200);
        full.tank_ref_mc = Some(59_000);
        assert_eq!(rod.step(t(200), &full), 0);
        full.energy = energy(9_500, 210);
        full.tank_ref_mc = Some(58_000);
        assert_eq!(rod.step(t(210), &full), 3);
    }

    #[test]
    fn emergency_runs_the_dictated_stage_regardless_of_power() {
        let mut rod = rod();
        let mut inputs = rod_inputs(None);
        inputs.emergency = Some(EmergencyInput {
            stage: 2,
            tank_target_mc: 55_000,
            received: t(0),
        });
        assert_eq!(rod.step(t(0), &inputs), 2);
        assert_eq!(rod.reason(), RodReason::Emergency);

        // Even with the release withdrawn and no surplus.
        inputs.energy = Some(EnergyInput {
            power_w: -2_000,
            rod_enabled: false,
            received: t(5),
        });
        assert_eq!(rod.step(t(5), &inputs), 2);

        // A new stage applies at once, no dwell.
        inputs.emergency = Some(EmergencyInput {
            stage: 3,
            tank_target_mc: 55_000,
            received: t(10),
        });
        assert_eq!(rod.step(t(10), &inputs), 3);
    }

    #[test]
    fn emergency_regulates_to_its_own_tank_target_and_expires() {
        let mut rod = rod();
        let mut inputs = rod_inputs(None);
        inputs.emergency = Some(EmergencyInput {
            stage: 2,
            tank_target_mc: 55_000,
            received: t(0),
        });
        inputs.tank_ref_mc = Some(55_000);
        assert_eq!(rod.step(t(0), &inputs), 0);
        assert_eq!(rod.reason(), RodReason::TankFull);

        inputs.tank_ref_mc = Some(40_000);
        inputs.emergency.as_mut().unwrap().received = t(10);
        assert_eq!(rod.step(t(10), &inputs), 2);
        // The messages stop: off after 30 s.
        assert_eq!(rod.step(t(40), &inputs), 0);
    }

    #[test]
    fn emergency_stage_zero_means_no_emergency() {
        let mut rod = rod();
        let mut inputs = rod_inputs(energy(3_300, 0));
        inputs.emergency = Some(EmergencyInput {
            stage: 0,
            tank_target_mc: 55_000,
            received: t(0),
        });
        // Falls back to following the surplus.
        assert_eq!(rod.step(t(0), &inputs), 1);
    }

    #[test]
    fn emergency_stage_is_capped_at_three() {
        let mut rod = rod();
        let mut inputs = rod_inputs(None);
        inputs.emergency = Some(EmergencyInput {
            stage: 200,
            tank_target_mc: 70_000,
            received: t(0),
        });
        assert_eq!(rod.step(t(0), &inputs), 3);
    }

    // ---- valves and pump ---------------------------------------------

    #[test]
    fn pump_starts_two_minutes_after_the_valves_and_stops_with_them() {
        let mut circuit = CircuitController::new(Duration::from_secs(120));
        assert_eq!(
            circuit.step(t(0), false),
            CircuitOutputs {
                valves_tank: false,
                pump: false
            }
        );
        assert_eq!(
            circuit.step(t(10), true),
            CircuitOutputs {
                valves_tank: true,
                pump: false
            }
        );
        assert!(!circuit.step(t(129), true).pump);
        assert!(circuit.step(t(130), true).pump);
        // Valves back: pump off in the same step.
        assert_eq!(
            circuit.step(t(200), false),
            CircuitOutputs {
                valves_tank: false,
                pump: false
            }
        );
        // A new tank phase starts the delay from scratch.
        assert!(!circuit.step(t(300), true).pump);
        assert!(circuit.step(t(420), true).pump);
    }

    #[test]
    fn tank_availability_has_hysteresis() {
        let mut tank = TankAvailability::new();
        assert!(!tank.update(None, 40_000));
        assert!(!tank.update(Some(42_999), 40_000));
        assert!(tank.update(Some(43_000), 40_000));
        // Stays usable until it gets within 1 K of the target.
        assert!(tank.update(Some(41_500), 40_000));
        assert!(!tank.update(Some(40_999), 40_000));
        assert!(!tank.update(Some(42_000), 40_000));
    }

    // ---- mixer -------------------------------------------------------

    fn regulate(target_mc: i32, flow_mc: i32) -> MixerRequest {
        MixerRequest::Regulate {
            target_mc,
            flow_mc: Some(flow_mc),
        }
    }

    /// A mixer that has finished its start-up run (at the cold end) at the
    /// returned time.
    fn cold_mixer() -> (MixerController, u64) {
        let mut mixer = MixerController::new(MixerConfig::new());
        let mut now = 0;
        assert_eq!(mixer.step(t(now), MixerRequest::Hold), MixerDrive::Cold);
        while mixer.homing() {
            now += 1;
            mixer.step(t(now), MixerRequest::Hold);
        }
        // Let the post-homing pause run out.
        for _ in 0..3 {
            now += 1;
            mixer.step(t(now), MixerRequest::Hold);
        }
        (mixer, now)
    }

    /// A mixer at the hot end, ready for regulating towards colder.
    fn homed_mixer() -> (MixerController, u64) {
        let (mut mixer, mut now) = cold_mixer();
        assert_eq!(mixer.step(t(now), MixerRequest::FullHot), MixerDrive::Hot);
        while mixer.step(t(now), MixerRequest::FullHot) == MixerDrive::Hot {
            now += 1;
        }
        for _ in 0..3 {
            now += 1;
            mixer.step(t(now), MixerRequest::Hold);
        }
        assert_eq!(mixer.position_percent(), 100);
        (mixer, now)
    }

    #[test]
    fn mixer_homes_towards_cold_at_start_up() {
        let mut mixer = MixerController::new(MixerConfig::new());
        assert_eq!(mixer.step(t(0), regulate(40_000, 30_000)), MixerDrive::Cold);
        // Requests are ignored while homing.
        assert_eq!(
            mixer.step(t(60), regulate(60_000, 30_000)),
            MixerDrive::Cold
        );
        assert_eq!(mixer.step(t(129), MixerRequest::FullHot), MixerDrive::Cold);
        assert!(mixer.homing());
        assert_eq!(mixer.step(t(130), MixerRequest::Hold), MixerDrive::Stop);
        assert!(!mixer.homing());
        assert_eq!(mixer.position_percent(), 0);
    }

    #[test]
    fn mixer_regulates_towards_hot_from_the_cold_end() {
        let (mut mixer, now) = cold_mixer();
        // 2 K too cold: a 6 s pulse towards hot = 5 % of the travel.
        assert_eq!(
            mixer.step(t(now + 1), regulate(40_000, 38_000)),
            MixerDrive::Hot
        );
        assert_eq!(
            mixer.step(t(now + 7), regulate(40_000, 38_000)),
            MixerDrive::Stop
        );
        assert_eq!(mixer.position_percent(), 5);
    }

    #[test]
    fn full_cold_drives_to_the_return_end_with_margin_and_then_stays() {
        let (mut mixer, mut now) = homed_mixer();
        assert_eq!(mixer.step(t(now), MixerRequest::FullCold), MixerDrive::Cold);
        let started = now;
        while mixer.step(t(now), MixerRequest::FullCold) == MixerDrive::Cold {
            now += 1;
            assert!(now - started < 200);
        }
        // A whole travel plus the 10 s margin.
        assert_eq!(now - started, 130);
        assert_eq!(mixer.position_percent(), 0);
        now += 5;
        assert_eq!(mixer.step(t(now), MixerRequest::FullCold), MixerDrive::Stop);
    }

    #[test]
    fn full_cold_redrives_to_the_end_stop_after_six_hours() {
        let (mut mixer, now) = cold_mixer();
        assert_eq!(
            mixer.step(t(now + 60), MixerRequest::FullCold),
            MixerDrive::Stop
        );
        let later = now + 6 * 60 * 60 + 1;
        assert_eq!(
            mixer.step(t(later), MixerRequest::FullCold),
            MixerDrive::Cold
        );
        assert_eq!(
            mixer.step(t(later + 10), MixerRequest::FullCold),
            MixerDrive::Stop
        );
    }

    #[test]
    fn mixer_pulses_proportionally_then_waits_for_the_sensor() {
        let (mut mixer, now) = homed_mixer();
        // Flow 4 K too hot: 3 s per K -> a 12 s pulse towards cold.
        assert_eq!(
            mixer.step(t(now + 1), regulate(40_000, 44_000)),
            MixerDrive::Cold
        );
        assert_eq!(
            mixer.step(t(now + 12), regulate(40_000, 44_000)),
            MixerDrive::Cold
        );
        assert_eq!(
            mixer.step(t(now + 13), regulate(40_000, 44_000)),
            MixerDrive::Stop
        );
        // 12 s of 120 s = 10 % towards cold.
        assert_eq!(mixer.position_percent(), 90);
        // Settling: no new pulse for 60 s even though the error remains.
        assert_eq!(
            mixer.step(t(now + 30), regulate(40_000, 44_000)),
            MixerDrive::Stop
        );
        assert_eq!(
            mixer.step(t(now + 72), regulate(40_000, 44_000)),
            MixerDrive::Stop
        );
        assert_eq!(
            mixer.step(t(now + 73), regulate(40_000, 44_000)),
            MixerDrive::Cold
        );
    }

    #[test]
    fn mixer_does_nothing_inside_the_deadband() {
        let (mut mixer, now) = homed_mixer();
        assert_eq!(
            mixer.step(t(now + 1), regulate(40_000, 40_400)),
            MixerDrive::Stop
        );
        assert_eq!(
            mixer.step(t(now + 2), regulate(40_000, 39_500)),
            MixerDrive::Stop
        );
        assert_eq!(mixer.position_percent(), 100);
    }

    #[test]
    fn mixer_pulses_are_limited_in_length() {
        let (mut mixer, now) = homed_mixer();
        // 30 K off would be 90 s; capped at 15 s.
        assert_eq!(
            mixer.step(t(now + 1), regulate(40_000, 70_000)),
            MixerDrive::Cold
        );
        assert_eq!(
            mixer.step(t(now + 15), regulate(40_000, 70_000)),
            MixerDrive::Cold
        );
        assert_eq!(
            mixer.step(t(now + 16), regulate(40_000, 70_000)),
            MixerDrive::Stop
        );
        assert_eq!(mixer.position_percent(), 87);
    }

    #[test]
    fn mixer_does_not_push_against_the_end_stop() {
        let (mut mixer, now) = homed_mixer();
        // Already at the hot end and still too cold: nothing to do.
        assert_eq!(
            mixer.step(t(now + 1), regulate(40_000, 30_000)),
            MixerDrive::Stop
        );
    }

    #[test]
    fn mixer_never_reverses_without_a_pause() {
        let (mut mixer, mut now) = homed_mixer();
        let mut previous = MixerDrive::Stop;
        let mut stopped_since = 1u32;
        // Alternate between too hot and too cold requests for a long time.
        for i in 0..2_000u32 {
            now += 1;
            let flow = if (i / 100) % 2 == 0 { 45_000 } else { 35_000 };
            let drive = mixer.step(t(now), regulate(40_000, flow));
            match (previous, drive) {
                (MixerDrive::Hot, MixerDrive::Cold) | (MixerDrive::Cold, MixerDrive::Hot) => {
                    panic!("reversed directly at {now}");
                }
                (_, MixerDrive::Stop) => stopped_since += 1,
                (MixerDrive::Stop, _) => {
                    assert!(stopped_since >= 1, "no pause before moving at {now}");
                    stopped_since = 0;
                }
                _ => {}
            }
            previous = drive;
        }
    }

    #[test]
    fn mixer_without_a_flow_reading_holds_its_position() {
        let (mut mixer, now) = homed_mixer();
        let request = MixerRequest::Regulate {
            target_mc: 40_000,
            flow_mc: None,
        };
        assert_eq!(mixer.step(t(now + 1), request), MixerDrive::Stop);
    }

    #[test]
    fn full_hot_drives_from_the_middle_with_margin_and_then_stays() {
        let (mut mixer, mut now) = homed_mixer();
        // Move to roughly the middle first: several cold pulses.
        for _ in 0..4 {
            while mixer.step(t(now), regulate(40_000, 55_000)) != MixerDrive::Cold {
                now += 1;
            }
            while mixer.step(t(now), regulate(40_000, 55_000)) == MixerDrive::Cold {
                now += 1;
            }
        }
        assert!(mixer.position_percent() < 60);
        // Wait out the settle time, then ask for full hot.
        now += 61;
        assert_eq!(mixer.step(t(now), MixerRequest::FullHot), MixerDrive::Hot);
        let started = now;
        let mut duration = 0;
        while mixer.step(t(now), MixerRequest::FullHot) == MixerDrive::Hot {
            now += 1;
            duration += 1;
            assert!(duration < 200);
        }
        // Remaining travel plus the 10 s margin.
        assert!(now - started >= 70, "{}", now - started);
        assert_eq!(mixer.position_percent(), 100);
        now += 5;
        assert_eq!(mixer.step(t(now), MixerRequest::FullHot), MixerDrive::Stop);
    }

    #[test]
    fn full_hot_redrives_to_the_end_stop_after_six_hours() {
        let (mut mixer, now) = homed_mixer();
        assert_eq!(
            mixer.step(t(now + 60), MixerRequest::FullHot),
            MixerDrive::Stop
        );
        let later = now + 6 * 60 * 60 + 1;
        assert_eq!(mixer.step(t(later), MixerRequest::FullHot), MixerDrive::Hot);
        // Margin only: the estimate says it's there already.
        assert_eq!(
            mixer.step(t(later + 9), MixerRequest::FullHot),
            MixerDrive::Hot
        );
        assert_eq!(
            mixer.step(t(later + 10), MixerRequest::FullHot),
            MixerDrive::Stop
        );
    }

    // ---- everything together ----------------------------------------

    fn inputs(now: u64) -> HeatingInputs {
        let mut tank = [None; TANK_SENSORS];
        // Top to bottom: 62, 58, 50, 45, 40, 35 °C.
        for (slot, value) in tank
            .iter_mut()
            .zip([62_000, 58_000, 50_000, 45_000, 40_000, 35_000])
        {
            *slot = Some(value);
        }
        HeatingInputs {
            target: Some(TargetInput {
                flow_target_mc: 40_000,
                tank_mode: true,
                received: t(now),
            }),
            energy: energy(0, now),
            emergency: None,
            tank_temps_mc: tank,
            tank_ref_index: 5,
            flow_mc: Some(40_000),
            tank_target_mc: 70_000,
        }
    }

    /// Steps once per second up to `until`, feeding fresh messages.
    fn run(
        controller: &mut HeatingController,
        from: u64,
        until: u64,
        edit: impl Fn(&mut HeatingInputs),
    ) -> HeatingOutputs {
        let mut out = HeatingOutputs::OFF;
        for now in from..=until {
            let mut i = inputs(now);
            edit(&mut i);
            out = controller.step(t(now), &i);
        }
        out
    }

    #[test]
    fn tank_operation_switches_the_valves_and_delays_the_pump() {
        let mut c = HeatingController::new();
        run(&mut c, 0, 140, |i| {
            i.target.as_mut().unwrap().tank_mode = false
        });

        let out = run(&mut c, 141, 141, |_| {});
        assert!(out.valves_tank);
        assert!(!out.pump);
        let out = run(&mut c, 142, 141 + 119, |_| {});
        assert!(!out.pump);
        let out = run(&mut c, 141 + 120, 141 + 120, |_| {});
        assert!(out.pump);

        // Back to direct operation: both off at once.
        let out = run(&mut c, 400, 400, |i| {
            i.target.as_mut().unwrap().tank_mode = false
        });
        assert!(!out.valves_tank);
        assert!(!out.pump);
    }

    #[test]
    fn a_cold_tank_sends_the_mixer_to_full_flow_instead_of_regulating() {
        let mut c = HeatingController::new();
        // Past homing (to the return end), valves on. Tank top 62 °C vs
        // 40 °C target: regulates.
        run(&mut c, 0, 140, |_| {});
        assert!(c.status().tank_usable);
        assert_eq!(c.status().mixer_percent, 0);

        // The flow is 10 K too cold: it regulates towards hot.
        let mut hot_pulse = false;
        for now in 141..200 {
            let mut i = inputs(now);
            i.flow_mc = Some(30_000);
            hot_pulse |= c.step(t(now), &i).mixer == MixerDrive::Hot;
        }
        assert!(hot_pulse);
        assert!(c.status().mixer_percent > 0 && c.status().mixer_percent < 100);

        // Now the tank is lukewarm: full flow, whatever the flow reads.
        for now in 200..400 {
            let mut i = inputs(now);
            i.tank_temps_mc = [Some(40_500); TANK_SENSORS];
            i.flow_mc = Some(50_000);
            c.step(t(now), &i);
        }
        assert!(!c.status().tank_usable);
        assert_eq!(c.status().mixer_percent, 100);
    }

    #[test]
    fn direct_operation_parks_the_mixer_at_the_return_end() {
        let mut c = HeatingController::new();
        // Tank operation first, with the mixer pulled away from the end.
        for now in 0..400 {
            let mut i = inputs(now);
            i.tank_temps_mc = [Some(40_500); TANK_SENSORS];
            c.step(t(now), &i);
        }
        assert_eq!(c.status().mixer_percent, 100);

        // Back to direct operation: all the way to return, then stays.
        let mut cold_drive = false;
        for now in 400..600 {
            let mut i = inputs(now);
            i.target.as_mut().unwrap().tank_mode = false;
            cold_drive |= c.step(t(now), &i).mixer == MixerDrive::Cold;
        }
        assert!(cold_drive);
        assert_eq!(c.status().mixer_percent, 0);
        let out = run(&mut c, 601, 601, |i| {
            i.target.as_mut().unwrap().tank_mode = false;
            i.flow_mc = Some(10_000);
        });
        assert_eq!(out.mixer, MixerDrive::Stop);
    }

    #[test]
    fn losing_the_heating_target_parks_the_circuit_and_leaves_the_rod_logic_alone() {
        let mut c = HeatingController::new();
        for now in 0..400 {
            let mut i = inputs(now);
            i.tank_temps_mc = [Some(40_500); TANK_SENSORS];
            c.step(t(now), &i);
        }
        assert_eq!(c.status().mixer_percent, 100);

        // The target message is 100 s old; power keeps coming.
        let mut cold_drive = false;
        let mut last = HeatingOutputs::OFF;
        for now in 500..700 {
            last = c.step(
                t(now),
                &HeatingInputs {
                    target: Some(TargetInput {
                        flow_target_mc: 40_000,
                        tank_mode: true,
                        received: t(400),
                    }),
                    energy: energy(9_900, now),
                    ..inputs(now)
                },
            );
            cold_drive |= last.mixer == MixerDrive::Cold;
        }
        assert!(!last.valves_tank);
        assert!(!last.pump);
        assert!(cold_drive);
        assert_eq!(c.status().mixer_percent, 0);
        assert_eq!(last.rod_stage, 3);
    }

    #[test]
    fn the_rod_follows_the_surplus_in_the_full_controller() {
        let mut c = HeatingController::new();
        let out = run(&mut c, 0, 10, |i| i.energy = energy(6_500, 10));
        assert_eq!(out.rod_stage, 2);
        // Everything off when the tank sensors vanish.
        let out = run(&mut c, 11, 11, |i| {
            i.energy = energy(6_500, 11);
            i.tank_temps_mc = [None; TANK_SENSORS];
        });
        assert_eq!(out.rod_stage, 0);
        assert_eq!(c.status().rod_reason, RodReason::NoTankSensor);
    }
}
