//! `/api/v1/automation`: the gateway's own automation (ADR 0025) — its
//! outputs, button bindings, motion lights and schedules, one JSON object
//! per item, and their live state.
//!
//! Items are written to flash as they come; the automation then loads the
//! configuration again ([`crate::automation::Event::Reload`]), keeping the
//! states of what is still there.

use super::error::ApiError;
use super::json::{json, with_status};
use crate::automation::store::{store, StoreError};
use crate::automation::{post, Event, RUNTIME};
use core::str::FromStr;
use gateway_core::automation::binding::{Action, Binding, Op, Trigger};
use gateway_core::automation::engine::{Kind, Movement, OutputKind};
use gateway_core::automation::motion::{Mode, MotionLight};
use gateway_core::automation::output::Output;
use gateway_core::automation::schedule::Schedule;
use gateway_core::automation::store::{Named, Section};
use gateway_core::automation::{valid_name, Invalid, Name, MAX_BINDINGS};
use heapless::Vec;
use picoserve::extract::{Json, Query};
use picoserve::response::{IntoResponse, StatusCode};
use picoserve::routing::{get, parse_path_segment, post as post_route, PathRouter};
use picoserve::Router;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const OUTPUTS: &str = "/automation/outputs";
const BINDINGS: &str = "/automation/bindings";
const MOTION: &str = "/automation/motion";
const SCHEDULES: &str = "/automation/schedules";
/// Bindings per page of `GET /automation/bindings`.
const PAGE: usize = 8;

pub fn routes<R: PathRouter>(router: Router<R>) -> Router<impl PathRouter> {
    let name = || parse_path_segment::<ItemName>();
    router
        .route("/automation", get(status).patch(update))
        .route(OUTPUTS, get(outputs))
        .route(
            (OUTPUTS, name()),
            get(get_output).put(put_output).delete(delete_output),
        )
        .route((OUTPUTS, name(), "/command"), post_route(command))
        .route(BINDINGS, get(bindings).post(add_binding))
        .route(
            (BINDINGS, parse_path_segment::<u8>()),
            get(get_binding).put(put_binding).delete(delete_binding),
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
        )
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
    bindings: u32,
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
        bindings: st.slots(Section::Binding).count(),
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

// ---- bindings ----

#[derive(Deserialize)]
struct PageQuery {
    #[serde(default)]
    offset: u8,
}

#[derive(Serialize)]
struct SlotBinding {
    slot: u8,
    trigger: Trigger,
    actions: Vec<Action, { gateway_core::automation::binding::MAX_ACTIONS }, u8>,
}

#[derive(Serialize)]
struct Page {
    total: u32,
    offset: u8,
    items: Vec<SlotBinding, PAGE>,
}

/// `GET /automation/bindings?offset=n`: [`PAGE`] bindings from the
/// `offset`th on, in slot order.
async fn bindings(Query(query): Query<PageQuery>) -> Result<impl IntoResponse, ApiError> {
    let page = {
        let mut st = store().await;
        let slots = st.slots(Section::Binding);
        let mut items = Vec::new();
        for slot in slots.iter().skip(usize::from(query.offset)).take(PAGE) {
            if let Some(binding) = st.get::<Binding>(Section::Binding, slot).await {
                let _ = items.push(SlotBinding {
                    slot,
                    trigger: binding.trigger,
                    actions: binding.actions,
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

async fn check_binding(binding: &Binding) -> Result<(), ApiError> {
    binding.validate().map_err(refused)?;
    for action in &binding.actions {
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

#[derive(Serialize)]
struct Added {
    slot: u8,
}

async fn add_binding(Json(binding): Json<Binding>) -> Result<impl IntoResponse, ApiError> {
    check_binding(&binding).await?;
    let slot = store()
        .await
        .add(Section::Binding, &binding)
        .await
        .map_err(store_error)?;
    post(Event::Reload);
    with_status(StatusCode::CREATED, &Added { slot }).await
}

async fn get_binding(slot: u8) -> Result<impl IntoResponse, ApiError> {
    let binding = store()
        .await
        .get::<Binding>(Section::Binding, slot)
        .await
        .ok_or(NOT_FOUND)?;
    json(&binding).await
}

async fn put_binding(
    slot: u8,
    Json(binding): Json<Binding>,
) -> Result<impl IntoResponse, ApiError> {
    if usize::from(slot) >= MAX_BINDINGS {
        return Err(NOT_FOUND);
    }
    check_binding(&binding).await?;
    store()
        .await
        .put(Section::Binding, slot, &binding)
        .await
        .map_err(store_error)?;
    post(Event::Reload);
    json(&Added { slot }).await
}

async fn delete_binding(slot: u8) -> Result<impl IntoResponse, ApiError> {
    {
        let mut st = store().await;
        if !st.slots(Section::Binding).taken(slot) {
            return Err(NOT_FOUND);
        }
        st.remove(Section::Binding, slot)
            .await
            .map_err(store_error)?;
    }
    post(Event::Reload);
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
