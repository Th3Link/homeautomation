//! `/api/v1/automation`: the gateway's own automation (ADR 0025) — its
//! outputs, switch configurations, motion lights and schedules, one JSON
//! object per item (a switch configuration's buttons each one of their
//! own), and their live state.
//!
//! Items are written to flash as they come; the automation then loads the
//! configuration again ([`crate::automation::Event::Reload`]), keeping the
//! states of what is still there.

use super::error::ApiError;
use super::json::{json, with_status};
use crate::automation::store::{store, StoreError};
use crate::automation::{post, Event, RUNTIME};
use core::str::FromStr;
use gateway_core::automation::binding::{Action, Op, Trigger};
use gateway_core::automation::devices::{DeviceNotes, Feature, StoredNotes, MAX_FEATURES};
use gateway_core::automation::engine::{Kind, Movement, OutputKind};
use gateway_core::automation::motion::{Mode, MotionLight};
use gateway_core::automation::output::Output;
use gateway_core::automation::schedule::Schedule;
use gateway_core::automation::store::{button_slot, Named, Section};
use gateway_core::automation::switches::{ButtonActions, SwitchConfig, BUTTONS, MAX_SWITCHES};
use gateway_core::automation::{valid_name, DeviceRef, Invalid, Name};
use heapless::Vec;
use picoserve::extract::{Json, Query};
use picoserve::response::{IntoResponse, StatusCode};
use picoserve::routing::{get, parse_path_segment, post as post_route, PathRouter};
use picoserve::Router;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const OUTPUTS: &str = "/outputs";
const SWITCHES: &str = "/switches";
const MOTION: &str = "/motion";
const SCHEDULES: &str = "/schedules";
const DEVICES: &str = "/devices";
/// Switch configurations per page of `GET /automation/switches`.
const PAGE: usize = 8;

/// The routes under `/automation` sit in a router of their own: picoserve
/// tries a router's routes one inside the other, each a stack frame holding
/// the handler's future, so ten routes here would make every route before
/// them ten frames deeper.
pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    let name = || parse_path_segment::<ItemName>();
    let items = Router::new()
        .route(OUTPUTS, get(outputs))
        .route(
            (OUTPUTS, name()),
            get(get_output).put(put_output).delete(delete_output),
        )
        .route((OUTPUTS, name(), "/command"), post_route(command))
        .route(SWITCHES, get(switch_configs))
        .route(
            (SWITCHES, name()),
            get(get_switch_config)
                .put(put_switch_config)
                .delete(delete_switch_config),
        )
        .route(
            (SWITCHES, name(), parse_path_segment::<u8>()),
            get(get_button).put(put_button).delete(delete_button),
        )
        .route(DEVICES, get(device_notes))
        .route(
            (DEVICES, parse_path_segment::<DeviceKey>()),
            get(get_device_notes)
                .put(put_device_notes)
                .delete(delete_device_notes),
        )
        .route(MOTION, get(motion_lights))
        .route(
            (MOTION, name()),
            get(get_motion)
                .put(put_motion)
                .patch(switch_motion)
                .delete(delete_motion),
        )
        .route(SCHEDULES, get(schedules))
        .route(
            (SCHEDULES, name()),
            get(get_schedule)
                .put(put_schedule)
                .patch(switch_schedule)
                .delete(delete_schedule),
        );
    router
        .nest("/automation", items)
        .route("/automation", get(status).patch(update))
}

/// The `:name` path segment: a valid [`Name`].
pub struct ItemName(Name);

impl FromStr for ItemName {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        if !valid_name(s) {
            return Err(());
        }
        Name::try_from(s).map(Self).map_err(|_| ())
    }
}

fn refused(e: Invalid) -> ApiError {
    ApiError::invalid_field(e.field, e.reason)
}

fn store_error(e: StoreError) -> ApiError {
    match e {
        StoreError::TooLarge => ApiError::invalid("the item is too large to store"),
        StoreError::Full => ApiError::new(StatusCode::CONFLICT, "full", "no room for another one"),
        StoreError::Flash => ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "flash",
            "writing the flash failed",
        ),
    }
}

const NOT_FOUND: ApiError = ApiError::not_found("no such item");

// ---- status ----

#[derive(Serialize)]
struct LastButtonView {
    trigger: Trigger,
    /// Seconds since it was pressed.
    age_s: u32,
}

#[derive(Serialize)]
struct Status {
    /// The master switch.
    enabled: bool,
    outputs: u32,
    switches: u32,
    motion: u32,
    schedules: u32,
    last_button: Option<LastButtonView>,
}

async fn status() -> Result<impl IntoResponse, ApiError> {
    let (enabled, last_button) = {
        let rt = RUNTIME.lock().await;
        (rt.enabled, rt.last_button)
    };
    let st = store().await;
    let status = Status {
        enabled,
        outputs: st.slots(Section::Output).count(),
        switches: st.slots(Section::Switch).count(),
        motion: st.slots(Section::Motion).count(),
        schedules: st.slots(Section::Schedule).count(),
        last_button: last_button.map(|b| LastButtonView {
            trigger: b.trigger,
            age_s: crate::sensor_list::now_seconds().saturating_sub(b.at_s),
        }),
    };
    drop(st);
    json(&status).await
}

#[derive(Deserialize)]
struct Update {
    enabled: bool,
}

async fn update(Json(body): Json<Update>) -> Result<impl IntoResponse, ApiError> {
    crate::automation::set_enabled(body.enabled)
        .await
        .map_err(store_error)?;
    status().await
}

// ---- named items (outputs, motion lights, schedules) ----

async fn get_named<T: DeserializeOwned + Serialize>(
    section: Section,
    name: &Name,
) -> Result<impl IntoResponse, ApiError> {
    let item = {
        let mut st = store().await;
        let slot = st.find::<T>(section, name).await.ok_or(NOT_FOUND)?;
        st.get::<Named<T>>(section, slot).await.ok_or(NOT_FOUND)?
    };
    json(&item.item).await
}

/// Stores `item` under `name`, in its slot if it has one; a new motion
/// light or schedule is enabled.
async fn put_named<T: DeserializeOwned + Serialize>(
    section: Section,
    name: Name,
    item: T,
) -> Result<impl IntoResponse, ApiError> {
    let created = {
        let mut st = store().await;
        match st.find::<T>(section, &name).await {
            Some(slot) => {
                let enabled = st
                    .get::<Named<T>>(section, slot)
                    .await
                    .is_none_or(|old| old.enabled);
                let named = Named {
                    name,
                    enabled,
                    item,
                };
                st.put(section, slot, &named).await.map_err(store_error)?;
                false
            }
            None => {
                let named = Named {
                    name,
                    enabled: true,
                    item,
                };
                st.add(section, &named).await.map_err(store_error)?;
                true
            }
        }
    };
    post(Event::Reload);
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    with_status(status, &Done { done: true }).await
}

async fn delete_named<T: DeserializeOwned>(
    section: Section,
    name: &Name,
) -> Result<impl IntoResponse, ApiError> {
    {
        let mut st = store().await;
        let slot = st.find::<T>(section, name).await.ok_or(NOT_FOUND)?;
        st.remove(section, slot).await.map_err(store_error)?;
    }
    post(Event::Reload);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct Done {
    done: bool,
}

/// Whether there is an output `name`.
async fn output_exists(name: &str) -> bool {
    store()
        .await
        .find::<Output>(Section::Output, name)
        .await
        .is_some()
}

/// Whether there is a motion light or schedule `name`.
async fn automation_exists(name: &str) -> bool {
    let mut st = store().await;
    st.find::<MotionLight>(Section::Motion, name)
        .await
        .is_some()
        || st.find::<Schedule>(Section::Schedule, name).await.is_some()
}

// ---- outputs ----

#[derive(Serialize)]
struct OutputView<'a> {
    name: &'a str,
    kind: &'static str,
    /// Light level, dimmer value, relay 0/1.
    level: u8,
    /// Lights: the top level.
    #[serde(skip_serializing_if = "Option::is_none")]
    top: Option<u8>,
    /// Shutters: `up`, `down`, `stopped`.
    #[serde(skip_serializing_if = "Option::is_none")]
    movement: Option<&'static str>,
}

#[derive(Serialize)]
struct List<T> {
    items: T,
}

/// The outputs the automation runs, with their state.
async fn outputs() -> Result<impl IntoResponse, ApiError> {
    let buffer = super::json::acquire().await;
    let rt = RUNTIME.lock().await;
    let e = &rt.engine;
    let mut items: Vec<OutputView<'_>, { gateway_core::automation::MAX_OUTPUTS }> = Vec::new();
    for i in 0..e.counts().0 as u8 {
        let (Some(name), Some(kind), Some(state)) =
            (e.output_name(i), e.output_kind(i), e.output_state(i))
        else {
            continue;
        };
        let _ = items.push(OutputView {
            name,
            kind: match kind {
                OutputKind::Light => "light",
                OutputKind::Dimmer => "dimmer",
                OutputKind::Relay => "relay",
                OutputKind::Shutter => "shutter",
                OutputKind::Group => "group",
            },
            level: state.level,
            top: e.light_top(i),
            movement: (kind == OutputKind::Shutter).then_some(match state.movement {
                Movement::Up => "up",
                Movement::Down => "down",
                Movement::Stopped => "stopped",
            }),
        });
    }
    let body = buffer.rendered(&List { items })?;
    Ok(picoserve::response::Response::new(StatusCode::OK, body))
}

async fn get_output(ItemName(name): ItemName) -> Result<impl IntoResponse, ApiError> {
    get_named::<Output>(Section::Output, &name).await
}

async fn put_output(
    ItemName(name): ItemName,
    Json(output): Json<Output>,
) -> Result<impl IntoResponse, ApiError> {
    output.validate().map_err(refused)?;
    if let Output::Group(group) = &output {
        for member in &group.members {
            if *member == name || !output_exists(member).await {
                return Err(ApiError::invalid_field("members", "not an output"));
            }
        }
    }
    put_named(Section::Output, name, output).await
}

async fn delete_output(ItemName(name): ItemName) -> Result<impl IntoResponse, ApiError> {
    delete_named::<Output>(Section::Output, &name).await
}

/// `POST /automation/outputs/:name/command`: an operation such as `"on"`
/// or `{"set": 4}`, done by the automation task.
async fn command(
    ItemName(name): ItemName,
    Json(op): Json<Op>,
) -> Result<impl IntoResponse, ApiError> {
    if RUNTIME.lock().await.engine.output(&name).is_none() {
        return Err(NOT_FOUND);
    }
    post(Event::Output { name, op });
    with_status(StatusCode::ACCEPTED, &Done { done: false }).await
}

// ---- switches ----

#[derive(Deserialize)]
struct PageQuery {
    #[serde(default)]
    offset: u8,
}

#[derive(Serialize)]
struct SwitchView {
    name: Name,
    switches: Vec<DeviceRef, MAX_SWITCHES, u8>,
    /// The buttons that do something, one bit each.
    buttons: u8,
}

#[derive(Serialize)]
struct Page {
    total: u32,
    offset: u8,
    items: Vec<SwitchView, PAGE>,
}

/// The buttons of switch configuration `slot` that do something.
fn used_buttons(st: &crate::automation::store::Store, slot: u8) -> u8 {
    (0..BUTTONS)
        .filter(|&b| {
            let (section, s) = button_slot(slot, b);
            st.slots(section).taken(s)
        })
        .fold(0, |mask, b| mask | 1 << b)
}

/// `GET /automation/switches?offset=n`: [`PAGE`] switch configurations
/// from the `offset`th on.
async fn switch_configs(Query(query): Query<PageQuery>) -> Result<impl IntoResponse, ApiError> {
    let page = {
        let mut st = store().await;
        let slots = st.slots(Section::Switch);
        let mut items = Vec::new();
        for slot in slots.iter().skip(usize::from(query.offset)).take(PAGE) {
            if let Some(named) = st.get::<Named<SwitchConfig>>(Section::Switch, slot).await {
                let _ = items.push(SwitchView {
                    name: named.name,
                    switches: named.item.switches,
                    buttons: used_buttons(&st, slot),
                });
            }
        }
        Page {
            total: slots.count(),
            offset: query.offset,
            items,
        }
    };
    json(&page).await
}

async fn get_switch_config(ItemName(name): ItemName) -> Result<impl IntoResponse, ApiError> {
    get_named::<SwitchConfig>(Section::Switch, &name).await
}

async fn put_switch_config(
    ItemName(name): ItemName,
    Json(config): Json<SwitchConfig>,
) -> Result<impl IntoResponse, ApiError> {
    config.validate().map_err(refused)?;
    {
        let mut st = store().await;
        let own = st.find::<SwitchConfig>(Section::Switch, &name).await;
        for slot in st.slots(Section::Switch).iter().filter(|&s| Some(s) != own) {
            if let Some(other) = st.get::<Named<SwitchConfig>>(Section::Switch, slot).await {
                if other
                    .item
                    .switches
                    .iter()
                    .any(|d| config.switches.contains(d))
                {
                    return Err(ApiError::invalid_field(
                        "switches",
                        "a switch is already in another configuration",
                    ));
                }
            }
        }
    }
    put_named(Section::Switch, name, config).await
}

async fn delete_switch_config(ItemName(name): ItemName) -> Result<impl IntoResponse, ApiError> {
    {
        let mut st = store().await;
        let slot = st
            .find::<SwitchConfig>(Section::Switch, &name)
            .await
            .ok_or(NOT_FOUND)?;
        for button in 0..BUTTONS {
            let (section, s) = button_slot(slot, button);
            if st.slots(section).taken(s) {
                st.remove(section, s).await.map_err(store_error)?;
            }
        }
        st.remove(Section::Switch, slot)
            .await
            .map_err(store_error)?;
    }
    post(Event::Reload);
    Ok(StatusCode::NO_CONTENT)
}

/// Where the actions of `button` of configuration `name` are stored.
async fn button_place(name: &Name, button: u8) -> Result<(Section, u8), ApiError> {
    if button >= BUTTONS {
        return Err(NOT_FOUND);
    }
    let slot = store()
        .await
        .find::<SwitchConfig>(Section::Switch, name)
        .await
        .ok_or(NOT_FOUND)?;
    Ok(button_slot(slot, button))
}

/// `GET /automation/switches/:name/:button`: what the button does; no
/// events if nothing.
async fn get_button(
    (ItemName(name), button): (ItemName, u8),
) -> Result<impl IntoResponse, ApiError> {
    let (section, slot) = button_place(&name, button).await?;
    let actions = store()
        .await
        .get::<ButtonActions>(section, slot)
        .await
        .unwrap_or_default();
    json(&actions).await
}

/// `PUT /automation/switches/:name/:button`; no events removes them.
async fn put_button(
    (ItemName(name), button): (ItemName, u8),
    Json(actions): Json<ButtonActions>,
) -> Result<impl IntoResponse, ApiError> {
    actions.validate().map_err(refused)?;
    for e in &actions.events {
        check_actions(&e.actions).await?;
    }
    let (section, slot) = button_place(&name, button).await?;
    {
        let mut st = store().await;
        if actions.events.is_empty() {
            if st.slots(section).taken(slot) {
                st.remove(section, slot).await.map_err(store_error)?;
            }
        } else {
            st.put(section, slot, &actions).await.map_err(store_error)?;
        }
    }
    post(Event::Reload);
    with_status(StatusCode::OK, &Done { done: true }).await
}

async fn delete_button(
    (ItemName(name), button): (ItemName, u8),
) -> Result<impl IntoResponse, ApiError> {
    let (section, slot) = button_place(&name, button).await?;
    {
        let mut st = store().await;
        if st.slots(section).taken(slot) {
            st.remove(section, slot).await.map_err(store_error)?;
        }
    }
    post(Event::Reload);
    Ok(StatusCode::NO_CONTENT)
}

/// The outputs and automations the actions name exist.
async fn check_actions(actions: &[Action]) -> Result<(), ApiError> {
    for action in actions {
        match action {
            Action::Output { name, .. } if !output_exists(name).await => {
                return Err(ApiError::invalid_field("actions", "not an output"));
            }
            Action::Automation { name, .. } if !automation_exists(name).await => {
                return Err(ApiError::invalid_field(
                    "actions",
                    "not a motion light or schedule",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

// ---- device notes ----

/// The `:device` path segment: a device key, `0x…`.
pub struct DeviceKey(DeviceRef);

impl FromStr for DeviceKey {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        let hex = s.strip_prefix("0x").ok_or(())?;
        let key = u32::from_str_radix(hex, 16).map_err(|_| ())?;
        let device = DeviceRef::from_key(key);
        device.valid().then_some(Self(device)).ok_or(())
    }
}

/// Devices with notes per page of `GET /automation/devices`.
const DEVICE_PAGE: usize = 16;

#[derive(Serialize)]
struct DeviceFeatures {
    device: DeviceRef,
    features: Vec<Feature, MAX_FEATURES, u8>,
    /// Whether it has lamp names.
    lamps: bool,
}

#[derive(Serialize)]
struct DevicePage {
    total: u32,
    offset: u8,
    items: Vec<DeviceFeatures, DEVICE_PAGE>,
}

/// `GET /automation/devices?offset=n`: the devices with notes, their
/// features (the lamp names only by device).
async fn device_notes(Query(query): Query<PageQuery>) -> Result<impl IntoResponse, ApiError> {
    let page = {
        let mut st = store().await;
        let slots = st.slots(Section::Devices);
        let mut items = Vec::new();
        for slot in slots
            .iter()
            .skip(usize::from(query.offset))
            .take(DEVICE_PAGE)
        {
            if let Some(stored) = st.get::<StoredNotes>(Section::Devices, slot).await {
                let _ = items.push(DeviceFeatures {
                    device: stored.device,
                    lamps: stored.notes.lamps.iter().any(|n| !n.is_empty()),
                    features: stored.notes.features,
                });
            }
        }
        DevicePage {
            total: slots.count(),
            offset: query.offset,
            items,
        }
    };
    json(&page).await
}

/// The slot of `device`'s notes.
async fn notes_slot(st: &mut crate::automation::store::Store, device: DeviceRef) -> Option<u8> {
    for slot in st.slots(Section::Devices).iter() {
        if let Some(stored) = st.get::<StoredNotes>(Section::Devices, slot).await {
            if stored.device == device {
                return Some(slot);
            }
        }
    }
    None
}

/// `GET /automation/devices/:device`: no features and no lamp names if
/// there are no notes.
async fn get_device_notes(DeviceKey(device): DeviceKey) -> Result<impl IntoResponse, ApiError> {
    let notes = {
        let mut st = store().await;
        match notes_slot(&mut st, device).await {
            Some(slot) => st
                .get::<StoredNotes>(Section::Devices, slot)
                .await
                .map(|s| s.notes)
                .unwrap_or_default(),
            None => DeviceNotes::default(),
        }
    };
    json(&notes).await
}

/// `PUT /automation/devices/:device`; empty notes remove them.
async fn put_device_notes(
    DeviceKey(device): DeviceKey,
    Json(notes): Json<DeviceNotes>,
) -> Result<impl IntoResponse, ApiError> {
    notes.validate().map_err(refused)?;
    {
        let mut st = store().await;
        let slot = notes_slot(&mut st, device).await;
        match (slot, notes.is_empty()) {
            (Some(slot), true) => st.remove(Section::Devices, slot).await,
            (None, true) => Ok(()),
            (Some(slot), false) => {
                st.put(Section::Devices, slot, &StoredNotes { device, notes })
                    .await
            }
            (None, false) => st
                .add(Section::Devices, &StoredNotes { device, notes })
                .await
                .map(|_| ()),
        }
        .map_err(store_error)?;
    }
    with_status(StatusCode::OK, &Done { done: true }).await
}

async fn delete_device_notes(DeviceKey(device): DeviceKey) -> Result<impl IntoResponse, ApiError> {
    let mut st = store().await;
    if let Some(slot) = notes_slot(&mut st, device).await {
        st.remove(Section::Devices, slot)
            .await
            .map_err(store_error)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---- motion lights and schedules ----

#[derive(Serialize)]
struct AutomationView<'a> {
    name: &'a str,
    enabled: bool,
    /// Motion lights: `idle`, `auto`, `manual`, `lockout`; schedules:
    /// `running` or `idle`.
    state: &'static str,
    /// Motion lights in `auto` or `lockout`: seconds until that ends.
    #[serde(skip_serializing_if = "Option::is_none")]
    remaining_s: Option<u32>,
}

async fn automations(kind: Kind) -> Result<impl IntoResponse, ApiError> {
    let buffer = super::json::acquire().await;
    let rt = RUNTIME.lock().await;
    let e = &rt.engine;
    let now = embassy_time::Instant::now().as_millis();
    let remaining = |until: u64| Some((until.saturating_sub(now) / 1000) as u32);
    let mut items: Vec<AutomationView<'_>, { gateway_core::automation::MAX_MOTION_LIGHTS }> =
        Vec::new();
    let count = match kind {
        Kind::Motion => e.counts().1,
        Kind::Schedule => e.counts().2,
    };
    for i in 0..count as u8 {
        let (Some(name), Some(enabled)) =
            (e.automation_name(kind, i), e.automation_enabled(kind, i))
        else {
            continue;
        };
        let (state, remaining_s) = match kind {
            Kind::Motion => match e.motion_mode(i) {
                Some(Mode::Auto { off_at }) => ("auto", remaining(off_at)),
                Some(Mode::Lockout { until }) => ("lockout", remaining(until)),
                Some(Mode::Manual) => ("manual", None),
                _ => ("idle", None),
            },
            Kind::Schedule => match e.schedule_running(i) {
                Some(true) => ("running", None),
                _ => ("idle", None),
            },
        };
        let _ = items.push(AutomationView {
            name,
            enabled,
            state,
            remaining_s,
        });
    }
    let body = buffer.rendered(&List { items })?;
    Ok(picoserve::response::Response::new(StatusCode::OK, body))
}

#[derive(Deserialize)]
struct Enabled {
    enabled: bool,
}

/// Enables or disables a motion light or schedule (kept on flash by the
/// automation task).
async fn switch(name: Name, enabled: bool) -> Result<impl IntoResponse, ApiError> {
    if RUNTIME.lock().await.engine.automation(&name).is_none() {
        return Err(NOT_FOUND);
    }
    let switch = if enabled {
        gateway_core::automation::binding::Switch::Enable
    } else {
        gateway_core::automation::binding::Switch::Disable
    };
    post(Event::Switch { name, switch });
    with_status(StatusCode::ACCEPTED, &Done { done: false }).await
}

async fn motion_lights() -> Result<impl IntoResponse, ApiError> {
    automations(Kind::Motion).await
}

async fn check_output(name: &str) -> Result<(), ApiError> {
    if output_exists(name).await {
        Ok(())
    } else {
        Err(ApiError::invalid_field("output", "not an output"))
    }
}

async fn get_motion(ItemName(name): ItemName) -> Result<impl IntoResponse, ApiError> {
    get_named::<MotionLight>(Section::Motion, &name).await
}

async fn put_motion(
    ItemName(name): ItemName,
    Json(motion): Json<MotionLight>,
) -> Result<impl IntoResponse, ApiError> {
    motion.validate().map_err(refused)?;
    check_output(&motion.output).await?;
    if store()
        .await
        .find::<Schedule>(Section::Schedule, &name)
        .await
        .is_some()
    {
        return Err(ApiError::invalid_field("name", "already a schedule"));
    }
    put_named(Section::Motion, name, motion).await
}

async fn switch_motion(
    ItemName(name): ItemName,
    Json(body): Json<Enabled>,
) -> Result<impl IntoResponse, ApiError> {
    switch(name, body.enabled).await
}

async fn delete_motion(ItemName(name): ItemName) -> Result<impl IntoResponse, ApiError> {
    delete_named::<MotionLight>(Section::Motion, &name).await
}

async fn schedules() -> Result<impl IntoResponse, ApiError> {
    automations(Kind::Schedule).await
}

async fn get_schedule(ItemName(name): ItemName) -> Result<impl IntoResponse, ApiError> {
    get_named::<Schedule>(Section::Schedule, &name).await
}

async fn put_schedule(
    ItemName(name): ItemName,
    Json(schedule): Json<Schedule>,
) -> Result<impl IntoResponse, ApiError> {
    schedule.validate().map_err(refused)?;
    check_output(&schedule.output).await?;
    if store()
        .await
        .find::<MotionLight>(Section::Motion, &name)
        .await
        .is_some()
    {
        return Err(ApiError::invalid_field("name", "already a motion light"));
    }
    put_named(Section::Schedule, name, schedule).await
}

async fn switch_schedule(
    ItemName(name): ItemName,
    Json(body): Json<Enabled>,
) -> Result<impl IntoResponse, ApiError> {
    switch(name, body.enabled).await
}

async fn delete_schedule(ItemName(name): ItemName) -> Result<impl IntoResponse, ApiError> {
    delete_named::<Schedule>(Section::Schedule, &name).await
}
