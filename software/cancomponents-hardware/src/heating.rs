//! The `HeatingControl` device: feeds the control logic in
//! [`cancomponents_core::heating`] with its inputs and applies its outputs
//! to the relays.
//!
//! Inputs are the three heating CAN messages (kept with their arrival time;
//! the logic treats anything older than 30 s as missing), the DS18B20
//! sensors from [`crate::temperature`] (assigned to their roles by id with
//! the `tank-sensor`/`flow-sensor` console commands) and a few settings
//! kept in flash. The control task runs every [`CONTROL_PERIOD`].
//!
//! The relays 0-10 belong to this module as long as the device type is
//! `HeatingControl`: CAN relay commands for them are ignored (see
//! `relais::relais_handler`), so nothing can switch the heating rod behind
//! the controller's back.

use crate::can::DEVICE_TYPE;
use crate::config::{self, config, TANK_SENSOR_KEY_BASE};
use crate::console_log;
use crate::error::{report_error, Component, ErrorCode, Severity};
use crate::relais::send_relais_command;
use crate::temperature;
use cancomponents_core::can_id::CanId;
use cancomponents_core::can_message_type::CanMessageType;
use cancomponents_core::device_type::DeviceType;
use cancomponents_core::heating::{
    rod_relays_on, EmergencyInput, EnergyInput, HeatingController, HeatingInputs, HeatingOutputs,
    HeatingStatus, MixerDrive, RodReason, TargetInput, CIRCUIT_PUMP_RELAY, MIXER_COLD_RELAY,
    MIXER_HOT_RELAY, ROD_RELAYS, TANK_SENSORS, VALVE_RELAYS,
};
use cancomponents_core::heating_message::{
    millicelsius_to_raw, HeatingEmergency, HeatingEnergy, HeatingManual, HeatingTarget,
};
use cancomponents_core::relais::{Message as RelaisMessage, State};
use core::cell::Cell;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_time::{Duration, Instant, Ticker};

/// How often the controller looks at its inputs and updates the relays.
pub const CONTROL_PERIOD: Duration = Duration::from_millis(500);
/// Sensor readings older than this are not used (a healthy sensor delivers
/// one every 10 s).
const MAX_READING_AGE: Duration = Duration::from_secs(25);
/// The relay states are sent again this often, in case one was lost.
const RESYNC_PERIOD: Duration = Duration::from_secs(10);
/// A condition that persists is reported again after this long.
const REPORT_REPEAT: Duration = Duration::from_secs(60);

/// Default charge target until one is set.
const DEFAULT_TANK_TARGET_C: u8 = 75;
/// Highest charge target the console accepts — safely below the 80 °C
/// hard limit.
pub const MAX_TANK_TARGET_C: u8 = 75;
pub const MIN_TANK_TARGET_C: u8 = 20;

type Shared<T> = BlockingMutex<CriticalSectionRawMutex, T>;

static TARGET: Shared<Cell<Option<TargetInput>>> = BlockingMutex::new(Cell::new(None));
static ENERGY: Shared<Cell<Option<EnergyInput>>> = BlockingMutex::new(Cell::new(None));
static EMERGENCY: Shared<Cell<Option<EmergencyInput>>> = BlockingMutex::new(Cell::new(None));

/// Sensor roles and the charge target.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// Topic id of the sensor at each tank position, top to bottom;
    /// 0 = none.
    pub tank_sensor_ids: [u32; TANK_SENSORS],
    /// Topic id of the heating circuit's flow sensor; 0 = none.
    pub flow_sensor_id: u32,
    pub tank_target_c: u8,
    /// Tank position the charge target is compared with.
    pub tank_ref_index: u8,
}

static SETTINGS: Shared<Cell<Settings>> = BlockingMutex::new(Cell::new(Settings {
    tank_sensor_ids: [0; TANK_SENSORS],
    flow_sensor_id: 0,
    tank_target_c: DEFAULT_TANK_TARGET_C,
    // Bottom of the tank by default: it's only full once that is warm. Set
    // it with `tank-ref` if the heating rod would reach the target higher up
    // before the bottom does.
    tank_ref_index: (TANK_SENSORS - 1) as u8,
}));

pub fn settings() -> Settings {
    SETTINGS.lock(Cell::get)
}

fn update_settings(change: impl FnOnce(&mut Settings)) {
    SETTINGS.lock(|cell| {
        let mut settings = cell.get();
        change(&mut settings);
        cell.set(settings);
    });
}

/// Assigns the sensor with topic id `id` (0 = none) to tank `position`.
pub async fn set_tank_sensor(position: usize, id: u32) -> Result<(), ()> {
    if position >= TANK_SENSORS {
        return Err(());
    }
    config()
        .await
        .set_u32(TANK_SENSOR_KEY_BASE + position as u8, id)
        .await?;
    update_settings(|s| s.tank_sensor_ids[position] = id);
    Ok(())
}

pub async fn set_flow_sensor(id: u32) -> Result<(), ()> {
    config()
        .await
        .set_u32(config::Key::HeatingFlowSensor, id)
        .await?;
    update_settings(|s| s.flow_sensor_id = id);
    Ok(())
}

pub async fn set_tank_target(celsius: u8) -> Result<(), ()> {
    if !(MIN_TANK_TARGET_C..=MAX_TANK_TARGET_C).contains(&celsius) {
        return Err(());
    }
    config()
        .await
        .set_u8(config::Key::HeatingTankTarget, celsius)
        .await?;
    update_settings(|s| s.tank_target_c = celsius);
    Ok(())
}

pub async fn set_tank_ref(index: u8) -> Result<(), ()> {
    if usize::from(index) >= TANK_SENSORS {
        return Err(());
    }
    config()
        .await
        .set_u8(config::Key::HeatingTankRef, index)
        .await?;
    update_settings(|s| s.tank_ref_index = index);
    Ok(())
}

/// Feeds a `HeatingTarget`, as if it had arrived over CAN.
pub fn inject_target(message: HeatingTarget) {
    TARGET.lock(|cell| {
        cell.set(Some(TargetInput {
            flow_target_mc: message.flow_target_millicelsius(),
            tank_mode: message.tank_mode,
            received: Instant::now(),
        }))
    });
}

/// Feeds a `HeatingEnergy`, as if it had arrived over CAN.
pub fn inject_energy(message: HeatingEnergy) {
    ENERGY.lock(|cell| {
        cell.set(Some(EnergyInput {
            power_w: message.power_w,
            rod_enabled: message.rod_enabled,
            received: Instant::now(),
        }))
    });
}

/// Feeds a `HeatingEmergency`, as if it had arrived over CAN.
pub fn inject_emergency(message: HeatingEmergency) {
    EMERGENCY.lock(|cell| {
        cell.set(Some(EmergencyInput {
            stage: message.stage,
            tank_target_mc: message.tank_target_millicelsius(),
            received: Instant::now(),
        }))
    });
}

/// Set once the controller task runs, i.e. on a `HeatingControl` node.
static CONTROLLER_RUNNING: AtomicBool = AtomicBool::new(false);

/// Carries out a `HeatingManual` message: the mode, then the relay. Turning
/// the mode on when it is on already leaves it (and its relays) as it is,
/// so that a client can repeat the message.
async fn apply_manual(message: HeatingManual) -> Result<(), &'static str> {
    // On any other kind of node the relays 6-10 are ordinary ones.
    if !CONTROLLER_RUNNING.load(Ordering::Relaxed) {
        return Ok(());
    }
    if !message.enabled {
        set_manual(false);
        return Ok(());
    }
    if !manual_active() {
        set_manual(true);
    }
    match message.relay {
        Some(relay) => manual_relay(usize::from(relay), message.relay_on).await,
        None => Ok(()),
    }
}

/// Handles the heating CAN messages. Malformed ones are reported and
/// dropped, and leave the previous value to time out on its own.
pub async fn heating_handler(id: CanId, data: &[u8], _remote_request: bool) {
    let decoded = match id.msg_type {
        CanMessageType::HeatingTarget => HeatingTarget::from_bytes(data).map(inject_target),
        CanMessageType::HeatingEnergy => HeatingEnergy::from_bytes(data).map(inject_energy),
        CanMessageType::HeatingEmergency => {
            HeatingEmergency::from_bytes(data).map(inject_emergency)
        }
        CanMessageType::HeatingManual => match HeatingManual::from_bytes(data) {
            Ok(message) => {
                if let Err(reason) = apply_manual(message).await {
                    console_log!("heating: manual command refused: {reason}");
                    report_error(
                        Component::Heating,
                        ErrorCode::InvalidData,
                        Severity::Warning,
                        id.msg_type as u8,
                        &[message.relay.unwrap_or(HeatingManual::NO_RELAY)],
                    )
                    .await;
                }
                Ok(())
            }
            Err(e) => Err(e),
        },
        _ => return,
    };
    if decoded.is_err() {
        report_error(
            Component::Heating,
            ErrorCode::InvalidData,
            Severity::Warning,
            id.msg_type as u8,
            &[data.len() as u8],
        )
        .await;
    }
}

/// How long the manual mode lasts after its last use.
const MANUAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// When the manual mode runs out; `None` while the controller is in charge.
static MANUAL_UNTIL: Shared<Cell<Option<Instant>>> = BlockingMutex::new(Cell::new(None));
/// Which of the relays 6-10 the console has switched on in manual mode
/// (index = relay - 6).
static MANUAL_ON: Shared<Cell<[bool; 5]>> = BlockingMutex::new(Cell::new([false; 5]));
/// First relay the manual mode may switch; 0-5 are the heating rod.
const FIRST_MANUAL_RELAY: usize = 6;
/// Last relay the manual mode may switch (relay 11 is never controlled).
const LAST_MANUAL_RELAY: usize = 10;

/// Whether the console has taken over the relays 6-10 (see
/// [`set_manual`]).
pub fn manual_active() -> bool {
    MANUAL_UNTIL
        .lock(Cell::get)
        .is_some_and(|until| Instant::now() < until)
}

/// Seconds until the manual mode ends by itself, if it is on.
pub fn manual_seconds_left() -> Option<u64> {
    let until = MANUAL_UNTIL.lock(Cell::get)?;
    until
        .checked_duration_since(Instant::now())
        .map(|left| left.as_secs())
}

/// Turns the manual (commissioning) mode on or off. While it is on the
/// controller leaves the relays 6-10 alone — valves, mixer and pump — so
/// they can be switched one by one with [`manual_relay`]; the heating rod
/// (relays 0-5) stays under the controller's care, and its safety limits,
/// all the time. Turning it off, or letting it time out after 10 minutes
/// without a command, switches the relays 6-10 off and hands them back,
/// with the mixer finding its position again.
pub fn set_manual(enabled: bool) {
    MANUAL_UNTIL.lock(|cell| cell.set(enabled.then(|| Instant::now() + MANUAL_TIMEOUT)));
    MANUAL_ON.lock(|cell| cell.set([false; 5]));
}

/// Switches one of the relays 6-10 by hand. Only in manual mode, and never
/// both mixer directions at once.
pub async fn manual_relay(num: usize, on: bool) -> Result<(), &'static str> {
    if !(FIRST_MANUAL_RELAY..=LAST_MANUAL_RELAY).contains(&num) {
        return Err("only relays 6-10 can be switched by hand");
    }
    if !manual_active() {
        return Err("the controller drives this relay; use `heating-manual on` first");
    }
    let index = num - FIRST_MANUAL_RELAY;
    let mut states = MANUAL_ON.lock(Cell::get);
    if on {
        let opposite = match num {
            MIXER_HOT_RELAY => Some(MIXER_COLD_RELAY),
            MIXER_COLD_RELAY => Some(MIXER_HOT_RELAY),
            _ => None,
        };
        if opposite.is_some_and(|other| states[other - FIRST_MANUAL_RELAY]) {
            return Err("the other mixer direction is on; switch it off first");
        }
    }
    states[index] = on;
    MANUAL_ON.lock(|cell| cell.set(states));
    MANUAL_UNTIL.lock(|cell| cell.set(Some(Instant::now() + MANUAL_TIMEOUT)));
    send_relais_command(RelaisMessage {
        num,
        state: if on { State::On } else { State::Off },
        duration: Duration::from_millis(0),
        bank: 0,
    })
    .await;
    Ok(())
}

/// What the controller did last, for the console.
#[derive(Debug, Clone, Copy)]
pub struct LiveStatus {
    pub outputs: HeatingOutputs,
    pub status: HeatingStatus,
    pub tank_temps_mc: [Option<i32>; TANK_SENSORS],
    pub flow_mc: Option<i32>,
    /// Seconds since each input message, `None` if there never was one.
    pub target_age_s: Option<u64>,
    pub energy_age_s: Option<u64>,
    pub emergency_age_s: Option<u64>,
    /// The relays 6-10 are under manual control (see [`set_manual`]).
    pub manual: bool,
}

static LIVE: Shared<Cell<Option<LiveStatus>>> = BlockingMutex::new(Cell::new(None));

/// The controller's latest state; `None` before its first cycle (or on a
/// device that isn't a `HeatingControl`).
pub fn live_status() -> Option<LiveStatus> {
    LIVE.lock(Cell::get)
}

/// Loads the settings from flash and starts the control task.
pub async fn init(spawner: &Spawner) {
    let mut loaded = settings();
    {
        let mut config = config().await;
        for (position, id) in loaded.tank_sensor_ids.iter_mut().enumerate() {
            *id = config
                .get_u32(TANK_SENSOR_KEY_BASE + position as u8)
                .await
                .unwrap_or(0);
        }
        loaded.flow_sensor_id = config
            .get_u32(config::Key::HeatingFlowSensor)
            .await
            .unwrap_or(0);
        if let Some(target) = config.get_u8(config::Key::HeatingTankTarget).await {
            loaded.tank_target_c = target.clamp(MIN_TANK_TARGET_C, MAX_TANK_TARGET_C);
        }
        if let Some(index) = config.get_u8(config::Key::HeatingTankRef).await {
            if usize::from(index) < TANK_SENSORS {
                loaded.tank_ref_index = index;
            }
        }
    }
    update_settings(|s| *s = loaded);
    console_log!("heating: {loaded:?}");
    CONTROLLER_RUNNING.store(true, Ordering::Relaxed);
    spawner.spawn(heating_task().unwrap());
}

/// The temperature of the sensor with topic id `id`, if it's delivering.
fn sensor_temperature_mc(sensors: &[temperature::SensorStatus], id: u32) -> Option<i32> {
    if id == 0 {
        return None;
    }
    let sensor = sensors.iter().find(|s| s.rom.topic_id() == id)?;
    let reading = sensor.reading.filter(|_| !sensor.lost)?;
    (reading.at.elapsed() <= MAX_READING_AGE).then(|| reading.temperature.millicelsius())
}

/// Where each relay should be for `outputs`.
fn relay_states(outputs: &HeatingOutputs) -> [State; 11] {
    let on_off = |on: bool| if on { State::On } else { State::Off };
    let mut states = [State::Off; 11];
    for (relay, on) in ROD_RELAYS.iter().zip(rod_relays_on(outputs.rod_stage)) {
        states[*relay] = on_off(on);
    }
    for relay in VALVE_RELAYS {
        states[relay] = on_off(outputs.valves_tank);
    }
    states[MIXER_HOT_RELAY] = on_off(outputs.mixer == MixerDrive::Hot);
    states[MIXER_COLD_RELAY] = on_off(outputs.mixer == MixerDrive::Cold);
    states[CIRCUIT_PUMP_RELAY] = on_off(outputs.pump);
    states
}

/// Sends the relays that differ from `applied` (all of them if `force`),
/// switching things off before anything is switched on — so the two mixer
/// directions can never be live together, even for an instant.
///
/// In `manual` mode the relays 6-10 are left alone.
async fn apply(applied: &mut [Option<State>; 11], wanted: [State; 11], force: bool, manual: bool) {
    for pass_on in [false, true] {
        for (num, state) in wanted.iter().enumerate() {
            if (*state == State::On) != pass_on {
                continue;
            }
            if manual && (FIRST_MANUAL_RELAY..=LAST_MANUAL_RELAY).contains(&num) {
                continue;
            }
            if !force && applied[num] == Some(*state) {
                continue;
            }
            send_relais_command(RelaisMessage {
                num,
                state: *state,
                duration: Duration::from_millis(0),
                bank: 0,
            })
            .await;
            applied[num] = Some(*state);
        }
    }
}

/// Remembers when a condition was last reported, so a persisting one is
/// repeated once a minute rather than every cycle.
#[derive(Default)]
struct Reporter {
    last: [Option<Instant>; 3],
}

impl Reporter {
    fn due(&mut self, slot: usize, active: bool, now: Instant) -> bool {
        if !active {
            self.last[slot] = None;
            return false;
        }
        match self.last[slot] {
            Some(at) if now.duration_since(at) < REPORT_REPEAT => false,
            _ => {
                self.last[slot] = Some(now);
                true
            }
        }
    }
}

#[embassy_executor::task]
async fn heating_task() {
    let mut controller = HeatingController::new();
    let mut applied: [Option<State>; 11] = [None; 11];
    let mut last_resync = Instant::now();
    let mut reporter = Reporter::default();
    let mut previous_reason = RodReason::Off;
    let mut previous_stage = 0u8;
    let mut ticker = Ticker::every(CONTROL_PERIOD);

    let mut was_manual = false;

    // Whatever state the relays were in before: everything off first.
    apply(
        &mut applied,
        relay_states(&HeatingOutputs::OFF),
        true,
        false,
    )
    .await;

    loop {
        let now = Instant::now();
        let settings = settings();
        let sensors = temperature::sensors();

        let mut tank_temps_mc = [None; TANK_SENSORS];
        for (temp, id) in tank_temps_mc.iter_mut().zip(settings.tank_sensor_ids) {
            *temp = sensor_temperature_mc(&sensors, id);
        }
        let flow_mc = sensor_temperature_mc(&sensors, settings.flow_sensor_id);
        let target = TARGET.lock(Cell::get);
        let energy = ENERGY.lock(Cell::get);
        let emergency = EMERGENCY.lock(Cell::get);

        let outputs = controller.step(
            now,
            &HeatingInputs {
                target,
                energy,
                emergency,
                tank_temps_mc,
                tank_ref_index: usize::from(settings.tank_ref_index),
                flow_mc,
                tank_target_mc: i32::from(settings.tank_target_c) * 1000,
            },
        );
        let status = controller.status();

        let mut force = now.duration_since(last_resync) >= RESYNC_PERIOD;
        let manual = manual_active();
        if manual != was_manual {
            if manual {
                // Taking over: the relays 6-10 go to a known state, off. The
                // rod relays keep what the controller wants for them.
                console_log!("heating: manual mode, relays 6-10 under console control");
                let mut wanted = relay_states(&outputs);
                for state in wanted.iter_mut().skip(FIRST_MANUAL_RELAY) {
                    *state = State::Off;
                }
                apply(&mut applied, wanted, true, false).await;
            } else {
                // Handing back: the mixer may be anywhere now, so it finds
                // its way again. The first cycle's outputs below put the
                // relays where the controller wants them.
                console_log!("heating: automatic mode again");
                MANUAL_UNTIL.lock(|cell| cell.set(None));
                controller.restart_circuit();
                force = true;
            }
            was_manual = manual;
        }
        if force {
            last_resync = now;
        }
        apply(&mut applied, relay_states(&outputs), force, manual).await;

        // A message can arrive between taking `now` and reading it.
        let age = |received: Option<Instant>| {
            received.map(|at| now.checked_duration_since(at).map_or(0, |d| d.as_secs()))
        };
        LIVE.lock(|cell| {
            cell.set(Some(LiveStatus {
                outputs,
                status,
                tank_temps_mc,
                flow_mc,
                target_age_s: age(target.map(|t| t.received)),
                energy_age_s: age(energy.map(|e| e.received)),
                emergency_age_s: age(emergency.map(|e| e.received)),
                manual,
            }))
        });

        if outputs.rod_stage != previous_stage || status.rod_reason != previous_reason {
            console_log!(
                "heating: rod stage {} ({:?}), valves {}, pump {}, mixer {:?} at {} %",
                outputs.rod_stage,
                status.rod_reason,
                if outputs.valves_tank {
                    "tank"
                } else {
                    "direct"
                },
                outputs.pump,
                outputs.mixer,
                status.mixer_percent,
            );
            previous_stage = outputs.rod_stage;
            previous_reason = status.rod_reason;
        }

        // Reports for what keeps the rod from working, repeated once a
        // minute while it persists. A missing tank sensor only counts while
        // somebody actually wants heat.
        let heat_wanted = emergency.is_some_and(|e| e.stage > 0)
            || energy.is_some_and(|e| e.rod_enabled && e.power_w > 0);
        if reporter.due(0, status.over_temperature, now) {
            let hottest = tank_temps_mc.iter().flatten().max().copied().unwrap_or(0);
            let raw = millicelsius_to_raw(hottest).to_le_bytes();
            report_error(
                Component::Heating,
                ErrorCode::OverTemperature,
                Severity::Error,
                0,
                &raw,
            )
            .await;
        }
        if reporter.due(
            1,
            status.rod_reason == RodReason::NoTankSensor && heat_wanted,
            now,
        ) {
            report_error(
                Component::Heating,
                ErrorCode::SensorLost,
                Severity::Error,
                0,
                &[],
            )
            .await;
        }
        if reporter.due(
            2,
            status.rod_reason == RodReason::InputsStale && heat_wanted_recently(energy, now),
            now,
        ) {
            report_error(
                Component::Heating,
                ErrorCode::Timeout,
                Severity::Warning,
                0,
                &[],
            )
            .await;
        }

        ticker.next().await;
    }
}

/// Whether the power messages used to ask for heat before they stopped:
/// a controller that has simply never been spoken to isn't an error.
fn heat_wanted_recently(energy: Option<EnergyInput>, now: Instant) -> bool {
    energy.is_some_and(|e| {
        e.rod_enabled
            && now
                .checked_duration_since(e.received)
                .is_none_or(|age| age < Duration::from_secs(300))
    })
}

/// True if this device is a heating controller (so the relays 0-10 are not
/// available for CAN/console switching).
pub async fn is_heating_controller() -> bool {
    *DEVICE_TYPE.lock().await == u8::from(DeviceType::HeatingControl)
}
