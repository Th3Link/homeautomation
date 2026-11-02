//! The gateway's own automation (ADR 0025): wall switches, motion lights
//! and schedules run here, on the CAN bus, without MQTT or a hub.
//!
//! [`gateway_core::automation`] has the logic; this module feeds it. The
//! configuration lives in [`store`] on flash; what every event needs is in
//! RAM ([`Runtime`]: the engine, which switch configurations each switch
//! is in, which flash slot each output came from). One task ([`automation_task`]) takes the events
//! from the CAN dispatch, MQTT and the web UI ([`post`]) one after the
//! other, ticks the timers and schedules, and sends what the engine
//! answers. A light's level table and a button's actions are read from
//! flash when they are needed.

pub mod store;

use crate::console_log;
use crate::location::{self, Location};
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};
use gateway_core::automation::binding::{Event as ButtonKind, Op, Switch, Trigger};
use gateway_core::automation::engine::{Command, Commands, Engine, Kind, Saved};
use gateway_core::automation::motion::MotionLight;
use gateway_core::automation::output::Output;
use gateway_core::automation::schedule::Schedule;
use gateway_core::automation::store::{
    button_of, button_slot, Named, Section, SwitchIndex, MASTER_SLOT,
};
use gateway_core::automation::switches::{ButtonActions, SwitchConfig};
use gateway_core::automation::when::Context;
use gateway_core::automation::{DeviceRef, Name, MAX_MOTION_LIGHTS, MAX_OUTPUTS, MAX_SCHEDULES};
use gateway_core::button::ButtonEventKind;
use gateway_core::can_message_type::CanMessageType;
use gateway_core::sensor_table::SensorKind;
use gateway_core::topics::automation as topics;
use heapless::Vec;
use store::store;

/// Something the automation reacts to.
#[derive(Debug, Clone)]
pub enum Event {
    Button {
        device_key: u32,
        button: u8,
        kind: ButtonEventKind,
    },
    Motion {
        device_key: u32,
    },
    Relay {
        device_key: u32,
        bank: u8,
        channel: u8,
        on: bool,
    },
    Shutter {
        device_key: u32,
        bank: u8,
        channel: u8,
        raw: u8,
    },
    /// A brightness reading came in (conditions may hold now).
    Brightness,
    /// An operation on an output from outside (MQTT, the web UI).
    Output {
        name: Name,
        op: Op,
    },
    /// Enabling or disabling a motion light or schedule from outside.
    Switch {
        name: Name,
        switch: Switch,
    },
    /// The configuration on flash changed.
    Reload,
}

static EVENTS: Channel<CriticalSectionRawMutex, Event, 16> = Channel::new();

/// Hands `event` to the automation task. Dropped (logged) if the task is
/// behind by more than 16 events.
pub fn post(event: Event) {
    if EVENTS.try_send(event).is_err() {
        console_log!("automation: events queue full, one dropped");
    }
}

/// The last button event seen, for learning a switch in the web UI.
#[derive(Debug, Clone, Copy)]
pub struct LastButton {
    pub trigger: Trigger,
    /// Uptime seconds.
    pub at_s: u32,
}

/// What every event needs, in RAM.
pub struct Runtime {
    pub engine: Engine,
    /// The flash slot of each engine output, motion light and schedule.
    output_slots: Vec<u8, MAX_OUTPUTS, u8>,
    motion_slots: Vec<u8, MAX_MOTION_LIGHTS, u8>,
    schedule_slots: Vec<u8, MAX_SCHEDULES, u8>,
    pub switches: SwitchIndex,
    /// The master switch: off, the automation does nothing (but keeps
    /// track of states).
    pub enabled: bool,
    pub last_button: Option<LastButton>,
    guard: Guard,
}

pub static RUNTIME: Mutex<CriticalSectionRawMutex, Runtime> = Mutex::new(Runtime {
    engine: Engine::new(),
    output_slots: Vec::new(),
    motion_slots: Vec::new(),
    schedule_slots: Vec::new(),
    switches: SwitchIndex::new(),
    enabled: true,
    last_button: None,
    guard: Guard::new(),
});

/// Stops the automation for a while when it sends far more than any
/// person could cause: a loop between rules, or a node that keeps
/// reporting.
struct Guard {
    window_start_ms: u64,
    count: u32,
    paused_until_ms: u64,
}

/// More commands than this in one second pause the automation.
const GUARD_LIMIT: u32 = 2 * gateway_core::automation::MAX_OUTPUTS as u32 + 24;
const GUARD_PAUSE_MS: u64 = 10_000;

impl Guard {
    const fn new() -> Self {
        Self {
            window_start_ms: 0,
            count: 0,
            paused_until_ms: 0,
        }
    }

    /// Whether `n` more commands may go out now.
    fn allow(&mut self, now: u64, n: usize) -> bool {
        if now < self.paused_until_ms {
            return false;
        }
        if now - self.window_start_ms >= 1000 {
            self.window_start_ms = now;
            self.count = 0;
        }
        self.count += n as u32;
        if self.count > GUARD_LIMIT {
            self.paused_until_ms = now + GUARD_PAUSE_MS;
            console_log!(
                "automation: more than {GUARD_LIMIT} commands in a second, paused for {} s",
                GUARD_PAUSE_MS / 1000
            );
            return false;
        }
        true
    }
}

/// The time-dependent inputs of the conditions and schedules.
struct Inputs {
    now: Option<i64>,
    location: Location,
    /// Brightness sensors' last readings: device key, lux.
    lux: Vec<(u32, u32), 16>,
}

impl Inputs {
    async fn read() -> Self {
        let mut lux = Vec::new();
        for entry in crate::sensor_list::SENSOR_LIST.lock().await.iter() {
            if entry.kind == SensorKind::Brightness {
                let _ = lux.push((entry.device_key, entry.value.max(0.0) as u32));
            }
        }
        Self {
            now: crate::wall_clock::now(),
            location: location::get().await,
            lux,
        }
    }

    fn lux(&self, key: u32) -> Option<u32> {
        self.lux.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }
}

fn now_ms() -> u64 {
    Instant::now().as_millis()
}

/// Builds the engine from what is on flash, keeping the states of what
/// is still there.
async fn load(rt: &mut Runtime) {
    let mut st = store().await;
    let mut saved = alloc::boxed::Box::<Saved>::default();
    rt.engine.save(&mut saved);
    rt.engine.clear();
    rt.output_slots.clear();
    rt.motion_slots.clear();
    rt.schedule_slots.clear();
    rt.switches.clear();

    rt.enabled = st
        .get::<bool>(Section::Meta, MASTER_SLOT)
        .await
        .unwrap_or(true);
    // Groups after the others: their members have to be there.
    for groups in [false, true] {
        for slot in st.slots(Section::Output).iter() {
            let Some(named) = st.get::<Named<Output>>(Section::Output, slot).await else {
                continue;
            };
            if matches!(named.item, Output::Group(_)) != groups {
                continue;
            }
            match rt.engine.add_output(&named.name, &named.item) {
                Ok(()) => {
                    let _ = rt.output_slots.push(slot);
                }
                Err(e) => console_log!(
                    "automation: output {} not loaded: {}: {}",
                    named.name,
                    e.field,
                    e.reason
                ),
            }
        }
    }
    for slot in st.slots(Section::Switch).iter() {
        if let Some(named) = st.get::<Named<SwitchConfig>>(Section::Switch, slot).await {
            if !rt.switches.set_switches(slot, &named.item.switches) {
                console_log!(
                    "automation: too many switches, {} not loaded fully",
                    named.name
                );
            }
        }
    }
    for section in [Section::ButtonsLow, Section::ButtonsHigh] {
        for slot in st.slots(section).iter() {
            if let Some((config, button)) = button_of(section, slot) {
                rt.switches.set_button(config, button, true);
            }
        }
    }
    for slot in st.slots(Section::Motion).iter() {
        let Some(named) = st.get::<Named<MotionLight>>(Section::Motion, slot).await else {
            continue;
        };
        match rt.engine.add_motion(&named.name, named.item, named.enabled) {
            Ok(()) => {
                let _ = rt.motion_slots.push(slot);
            }
            Err(e) => console_log!(
                "automation: motion light {} not loaded: {}: {}",
                named.name,
                e.field,
                e.reason
            ),
        }
    }
    for slot in st.slots(Section::Schedule).iter() {
        let Some(named) = st.get::<Named<Schedule>>(Section::Schedule, slot).await else {
            continue;
        };
        match rt
            .engine
            .add_schedule(&named.name, named.item, named.enabled)
        {
            Ok(()) => {
                let _ = rt.schedule_slots.push(slot);
            }
            Err(e) => console_log!(
                "automation: schedule {} not loaded: {}: {}",
                named.name,
                e.field,
                e.reason
            ),
        }
    }
    rt.engine.restore(&saved);
    let (outputs, motion, schedules) = rt.engine.counts();
    console_log!(
        "automation: {outputs} outputs, {} switch configurations, {motion} motion lights, {schedules} schedules{}",
        st.slots(Section::Switch).count(),
        if rt.enabled { "" } else { " (switched off)" }
    );
}

/// Takes one event to the engine; the commands it answers in `out`.
async fn handle(rt: &mut Runtime, event: Event, inputs: &Inputs, out: &mut Commands) {
    let now = now_ms();
    let lux = |key| inputs.lux(key);
    let ctx = Context {
        now: inputs.now,
        tz: &inputs.location.tz,
        place: inputs.location.place,
        lux: &lux,
    };
    match event {
        Event::Button {
            device_key,
            button,
            kind,
        } => {
            let Some(event) = ButtonKind::from_kind(kind) else {
                return;
            };
            let trigger = Trigger {
                device: DeviceRef::from_key(device_key),
                button,
                event,
            };
            rt.last_button = Some(LastButton {
                trigger,
                at_s: crate::sensor_list::now_seconds(),
            });
            if !rt.enabled {
                return;
            }
            let configs: Vec<u8, 8> = rt.switches.find(trigger.device, button).take(8).collect();
            for config in configs {
                let (section, slot) = button_slot(config, button);
                let actions = store().await.get::<ButtonActions>(section, slot).await;
                if let Some(actions) = actions.as_ref().and_then(|a| a.actions(event)) {
                    rt.engine.actions(now, actions, out);
                }
            }
        }
        Event::Motion { device_key } if rt.enabled => {
            rt.engine.motion(now, &ctx, device_key, out);
        }
        Event::Relay {
            device_key,
            bank,
            channel,
            on,
        } => rt
            .engine
            .relay_state(now, device_key, bank, channel, on, out),
        Event::Shutter {
            device_key,
            bank,
            channel,
            raw,
        } => rt
            .engine
            .shutter_state(now, device_key, bank, channel, raw, out),
        Event::Brightness if rt.enabled => rt.engine.tick(now, &ctx, out),
        Event::Output { name, op } => {
            rt.engine.command(now, &name, op, out);
        }
        Event::Switch { name, switch } => {
            rt.engine.switch_automation(&name, switch, out);
        }
        Event::Reload => {
            load(rt).await;
            if rt.enabled {
                rt.engine.tick(now, &ctx, out);
            }
        }
        Event::Motion { .. } | Event::Brightness => {}
    }
}

/// Sends what the engine answered.
async fn execute(rt: &mut Runtime, out: &Commands) {
    let sends = out
        .iter()
        .filter(|c| {
            !matches!(
                c,
                Command::OutputState { .. } | Command::AutomationState { .. }
            )
        })
        .count();
    if !rt.guard.allow(now_ms(), sends) {
        return;
    }
    for command in out {
        match *command {
            Command::Light { output, level } => {
                let Some(&slot) = rt.output_slots.get(usize::from(output)) else {
                    continue;
                };
                let light = store()
                    .await
                    .get::<Named<Output>>(Section::Output, slot)
                    .await;
                if let Some(Named {
                    item: Output::Light(light),
                    ..
                }) = light
                {
                    for (device, bank, msg) in light.frames(level) {
                        crate::lamps::send(device.key(), msg, bank.is_some()).await;
                    }
                }
            }
            Command::Lamp {
                device,
                with_bank,
                msg,
            } => crate::lamps::send(device.key(), msg, with_bank).await,
            Command::Relais {
                device,
                rollershutter,
                msg,
            } => {
                let msg_type = if rollershutter {
                    CanMessageType::Rollershutter
                } else {
                    CanMessageType::Relais
                };
                send(device, msg_type, &msg.to_bytes()).await;
            }
            Command::Nightlight { device, value } => {
                send(
                    device,
                    CanMessageType::Nightlight,
                    &gateway_core::lamp::nightlight_bytes(value),
                )
                .await;
            }
            Command::OutputState { output } => {
                let (Some(name), Some(kind), Some(state)) = (
                    rt.engine.output_name(output),
                    rt.engine.output_kind(output),
                    rt.engine.output_state(output),
                ) else {
                    continue;
                };
                if let Some(body) = topics::output_state_body(kind, state) {
                    crate::mqtt::publish(topics::output_state_topic(name).as_str(), &body).await;
                }
            }
            Command::AutomationState { kind, index } => {
                automation_state(rt, kind, index).await;
            }
        }
    }
}

async fn send(device: DeviceRef, msg_type: CanMessageType, data: &[u8]) {
    let id = device.key() + u32::from(u8::from(msg_type));
    crate::can::send_raw(id, data, false).await;
}

/// A motion light or schedule was enabled or disabled: kept on flash and
/// published.
async fn automation_state(rt: &Runtime, kind: Kind, index: u8) {
    let (Some(name), Some(enabled)) = (
        rt.engine.automation_name(kind, index),
        rt.engine.automation_enabled(kind, index),
    ) else {
        return;
    };
    let i = usize::from(index);
    let mut st = store().await;
    let stored = match kind {
        Kind::Motion => match rt.motion_slots.get(i) {
            Some(&slot) => match st.get::<Named<MotionLight>>(Section::Motion, slot).await {
                Some(mut named) if named.enabled != enabled => {
                    named.enabled = enabled;
                    st.put(Section::Motion, slot, &named).await.is_ok()
                }
                _ => true,
            },
            None => false,
        },
        Kind::Schedule => match rt.schedule_slots.get(i) {
            Some(&slot) => match st.get::<Named<Schedule>>(Section::Schedule, slot).await {
                Some(mut named) if named.enabled != enabled => {
                    named.enabled = enabled;
                    st.put(Section::Schedule, slot, &named).await.is_ok()
                }
                _ => true,
            },
            None => false,
        },
    };
    drop(st);
    if !stored {
        console_log!("automation: couldn't keep {name} enabled = {enabled} on flash");
    }
    crate::mqtt::publish(
        topics::automation_state_topic(name).as_str(),
        topics::automation_state_body(enabled),
    )
    .await;
}

/// The longest the task sleeps: the clock may have been set or have
/// jumped, and the schedules are checked again.
const MAX_SLEEP: Duration = Duration::from_secs(60);

/// When to tick next.
fn next_tick(rt: &Runtime, inputs: &Inputs) -> Instant {
    let lux = |key| inputs.lux(key);
    let ctx = Context {
        now: inputs.now,
        tz: &inputs.location.tz,
        place: inputs.location.place,
        lux: &lux,
    };
    let now = Instant::now();
    let mut next = now + MAX_SLEEP;
    let (timers, schedules) = rt.engine.deadline(&ctx);
    if let Some(ms) = timers {
        next = next.min(Instant::from_millis(ms));
    }
    if let (Some(at), Some(local_now)) = (schedules, ctx.local_now()) {
        let secs = u64::try_from(at - local_now).unwrap_or(0);
        next = next.min(now + Duration::from_secs(secs));
    }
    next
}

/// Loads the configuration, then runs the automation.
#[embassy_executor::task]
pub async fn automation_task() {
    store::init().await;
    {
        let mut rt = RUNTIME.lock().await;
        load(&mut rt).await;
    }
    let mut out = Commands::new();
    loop {
        let wake = {
            let inputs = Inputs::read().await;
            next_tick(&*RUNTIME.lock().await, &inputs)
        };
        let event = match select(EVENTS.receive(), Timer::at(wake)).await {
            Either::First(event) => Some(event),
            Either::Second(()) => None,
        };
        let inputs = Inputs::read().await;
        let mut rt = RUNTIME.lock().await;
        out.clear();
        match event {
            Some(event) => handle(&mut rt, event, &inputs, &mut out).await,
            None if rt.enabled => {
                let lux = |key| inputs.lux(key);
                let ctx = Context {
                    now: inputs.now,
                    tz: &inputs.location.tz,
                    place: inputs.location.place,
                    lux: &lux,
                };
                rt.engine.tick(now_ms(), &ctx, &mut out);
            }
            None => {}
        }
        execute(&mut rt, &out).await;
    }
}

/// Switches the whole automation on or off, and keeps that on flash.
pub async fn set_enabled(enabled: bool) -> Result<(), store::StoreError> {
    store()
        .await
        .put(Section::Meta, MASTER_SLOT, &enabled)
        .await?;
    RUNTIME.lock().await.enabled = enabled;
    console_log!(
        "automation: switched {}",
        if enabled { "on" } else { "off" }
    );
    Ok(())
}
