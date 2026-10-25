//! Interactive commissioning console, sharing UART0 with `esp_println`'s
//! debug output (see `crate::logging`).
//!
//! The console stays passive until a bare Enter keypress activates it —
//! until then, incoming bytes are ignored and debug logging via
//! [`console_log!`](crate::console_log) prints normally. Once active,
//! `console_log!` output is muted (through [`crate::logging::CLI_ACTIVE`])
//! so it can't corrupt the line editor's prompt, and the session runs until
//! `exit` is typed or Ctrl+D/Ctrl+C is pressed, after which logging resumes.
//!
//! Line editing (cursor movement, backspace, history via the up/down
//! arrows, tab-completion) and command parsing/validation/help are provided
//! by the [`bareminal_cli`] crate rather than hand-rolled here.

use crate::can::{send_can_message, DEVICE_ID, DEVICE_TYPE};
use crate::config::{self, config};
use crate::console_log;
use crate::device::device;
use crate::echo_guard;
use crate::heating;
use crate::logging::CLI_ACTIVE;
use crate::relais::send_relais_command;
use crate::temperature;
use bareminal_cli::cli::{Bareminal, CommandWriter};
use bareminal_macros::Command;
use cancomponents_core::can_message_type::CanMessageType;
use cancomponents_core::extension::Mode as ExtensionMode;
use cancomponents_core::heating_message::{
    millicelsius_to_raw, HeatingEmergency, HeatingEnergy, HeatingTarget,
};
use cancomponents_core::relais::{Message as RelaisMessage, Mode as RelaisMode, State};
use core::fmt::Write as _;
use core::str::FromStr;
use core::sync::atomic::Ordering;
use embassy_executor::Spawner;
use embassy_time::Duration;
use embedded_io_async::Read;
use esp_hal::gpio::{InputPin, OutputPin};
use esp_hal::uart;
use esp_hal::Async;
use heapless::String;

/// Max length of a single formatted response line.
const LINE_CAP: usize = 128;
/// Max length of a typed command line (bareminal_cli's own line buffer).
const MAX_CMD_BUFFER: usize = 160;
/// Number of previous lines kept for up/down-arrow recall.
const HISTORY_SIZE: usize = 8;

type Uart = uart::Uart<'static, Async>;
type UartTx = uart::UartTx<'static, Async>;
type UartRx = uart::UartRx<'static, Async>;
type Cli = Bareminal<Commands<'static>, UartTx, MAX_CMD_BUFFER, HISTORY_SIZE>;

/// Starts the console task.
pub async fn init(
    uart: esp_hal::peripherals::UART0<'static>,
    rx: impl InputPin + 'static,
    tx: impl OutputPin + 'static,
    spawner: &Spawner,
) {
    let uart: Uart = uart::Uart::new(uart, uart::Config::default())
        .unwrap()
        .with_rx(rx)
        .with_tx(tx)
        .into_async();

    spawner.spawn(console_task(uart).unwrap());
}

/// Config-setting and bring-up-testing commands. Doc comments become the
/// text shown by the built-in `help`/`help <command>`.
#[derive(Debug, Command)]
enum Commands<'a> {
    /// Show all current configuration values.
    Show,
    /// Set this device's CAN device id (a restart is needed for the CAN
    /// acceptance filter to actually follow it).
    DeviceId(u8),
    /// Set this device's CAN device type (a restart is needed for the CAN
    /// acceptance filter to actually follow it).
    DeviceType(u8),
    /// Set the hardware revision (takes effect after restart).
    Hwrev(u8),
    /// Set an 8-byte identification string, e.g. "kitchen" (single word,
    /// truncated to 8 bytes).
    CustomString(&'a str),
    /// Set the relay output mode (takes effect after restart).
    #[set(one_of = ["off", "relais", "software_rollershutter", "hardware_rollershutter"])]
    RelaisMode(RelaisModeArg),
    /// Set the extension header mode (takes effect after restart).
    #[set(one_of = [
        "off", "button", "sensors", "pwm", "relais", "legacy_sensors",
        "software_rollershutter", "hardware_rollershutter",
    ])]
    ExtensionMode(ExtensionModeArg),
    /// Set how often (in seconds, 10..=86400) the DS18B20 temperatures are
    /// sent over CAN. They are measured every 10 s regardless.
    #[set(min = 10, max = 86400)]
    SensorInterval(u32),
    /// List all DS18B20 temperature sensors: slot, ROM, the id used in CAN
    /// frames and MQTT topics, current temperature and age.
    Sensors,
    /// Stop tracking (and reporting) sensors that are currently lost, e.g.
    /// after removing one on purpose.
    SensorsForget,
    /// Give the sensor in this slot (as shown by `sensors`) a name of up to 7
    /// characters (A-Z a-z 0-9 - _), which the gateway uses in its MQTT
    /// topics and the web UI shows. The name stays with the sensor.
    SensorName {
        #[set(short)]
        slot: usize,
        #[set(short)]
        name: &'a str,
    },
    /// Heating controller: assign the DS18B20 with this id (as shown by
    /// `sensors`, e.g. 0x32c3b022) to a tank position, 0 = top … 6 =
    /// bottom. Use `none` as id to clear a position.
    TankSensor {
        #[set(short)]
        position: usize,
        #[set(short)]
        id: &'a str,
    },
    /// Heating controller: the DS18B20 measuring the heating circuit's flow
    /// temperature (id as shown by `sensors`, or `none`).
    FlowSensor(&'a str),
    /// Heating controller: stop charging the tank with surplus power once
    /// the reference sensor reaches this temperature (°C).
    #[set(min = 20, max = 75)]
    TankTarget(u8),
    /// Heating controller: tank position (0 = top … 6 = bottom) whose
    /// sensor the tank target is compared with.
    #[set(min = 0, max = 6)]
    TankRef(u8),
    /// Heating controller: show what it is doing and why.
    Heating,
    /// Bring-up: feed the heating controller a HeatingTarget as if it had
    /// arrived over CAN (flow target in °C, tank operation 0/1). Expires
    /// after 30 s like the real message.
    SimTarget {
        #[set(short)]
        celsius: u8,
        #[set(short, min = 0, max = 1)]
        tank: u8,
    },
    /// Bring-up: feed the heating controller a HeatingEnergy (available
    /// power in W, rod released 0/1).
    SimEnergy {
        #[set(short)]
        watts: i32,
        #[set(short, min = 0, max = 1)]
        rod: u8,
    },
    /// Bring-up: feed the heating controller a HeatingEmergency (rod stage
    /// 0-3, tank target in °C).
    SimEmergency {
        #[set(short, min = 0, max = 3)]
        stage: u8,
        #[set(short)]
        celsius: u8,
    },
    /// Whether a missing CAN `Echo` (bus or gateway gone) restarts the
    /// device. On by default; turn it off for a device without a gateway.
    #[set(one_of = ["on", "off"])]
    EchoRestart(OnOffArg),
    /// Heating controller commissioning: take over the relays 6-10 (valves,
    /// mixer, pump) so `relais` can switch them one by one. They are all
    /// off when it starts. The heating rod (relays 0-5) stays under the
    /// controller's care with all its limits. `off` — or 10 minutes without
    /// a `relais` command — hands the relays back to the controller.
    #[set(one_of = ["on", "off"])]
    HeatingManual(OnOffArg),
    /// Directly drive one relay/rollershutter channel, for bring-up testing
    /// without a CAN bus master.
    Relais {
        #[set(short)]
        num: usize,
        #[set(short, one_of = ["on", "off", "up", "down"])]
        state: RelaisStateArg,
    },
    /// Broadcast a CAN Ping, for bus-wiring sanity checks.
    Ping,
    /// Restart the device.
    Restart,
    /// Leave the console (same as Ctrl+C / Ctrl+D).
    Exit,
}

/// CLI-facing spelling of [`RelaisMode`] (kept separate from the core type
/// since `FromStr` can't be implemented on a foreign type here).
#[derive(Debug, Clone, Copy)]
enum RelaisModeArg {
    Off,
    Relais,
    SoftwareRollershutter,
    HardwareRollershutter,
}

impl RelaisModeArg {
    fn as_u8(self) -> u8 {
        match self {
            Self::Off => RelaisMode::Off as u8,
            Self::Relais => RelaisMode::Relais as u8,
            Self::SoftwareRollershutter => RelaisMode::SoftwareRollershutter as u8,
            Self::HardwareRollershutter => RelaisMode::HardwareRollershutter as u8,
        }
    }
}

impl FromStr for RelaisModeArg {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "off" => Ok(Self::Off),
            "relais" => Ok(Self::Relais),
            "software_rollershutter" => Ok(Self::SoftwareRollershutter),
            "hardware_rollershutter" => Ok(Self::HardwareRollershutter),
            _ => Err(()),
        }
    }
}

/// CLI-facing spelling of [`ExtensionMode`].
#[derive(Debug, Clone, Copy)]
enum ExtensionModeArg {
    Off,
    Button,
    Sensors,
    Pwm,
    Relais,
    LegacySensors,
    SoftwareRollershutter,
    HardwareRollershutter,
}

impl ExtensionModeArg {
    fn as_u8(self) -> u8 {
        match self {
            Self::Off => ExtensionMode::Off as u8,
            Self::Button => ExtensionMode::Button as u8,
            Self::Sensors => ExtensionMode::Sensors as u8,
            Self::Pwm => ExtensionMode::Pwm as u8,
            Self::Relais => ExtensionMode::Relais as u8,
            Self::LegacySensors => ExtensionMode::LegacySensors as u8,
            Self::SoftwareRollershutter => ExtensionMode::SoftwareRollershutter as u8,
            Self::HardwareRollershutter => ExtensionMode::HardwareRollershutter as u8,
        }
    }
}

impl FromStr for ExtensionModeArg {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "off" => Ok(Self::Off),
            "button" => Ok(Self::Button),
            "sensors" => Ok(Self::Sensors),
            "pwm" => Ok(Self::Pwm),
            "relais" => Ok(Self::Relais),
            "legacy_sensors" => Ok(Self::LegacySensors),
            "software_rollershutter" => Ok(Self::SoftwareRollershutter),
            "hardware_rollershutter" => Ok(Self::HardwareRollershutter),
            _ => Err(()),
        }
    }
}

/// `on`/`off`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnOffArg {
    On,
    Off,
}

impl FromStr for OnOffArg {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            _ => Err(()),
        }
    }
}

/// CLI-facing spelling of [`State`], for the `relais` bring-up command.
#[derive(Debug, Clone, Copy)]
enum RelaisStateArg {
    On,
    Off,
    Up,
    Down,
}

impl From<RelaisStateArg> for State {
    fn from(v: RelaisStateArg) -> Self {
        match v {
            RelaisStateArg::On => State::On,
            RelaisStateArg::Off => State::Off,
            RelaisStateArg::Up => State::Up,
            RelaisStateArg::Down => State::Down,
        }
    }
}

impl FromStr for RelaisStateArg {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            "up" => Ok(Self::Up),
            "down" => Ok(Self::Down),
            _ => Err(()),
        }
    }
}

#[embassy_executor::task]
async fn console_task(uart: Uart) {
    console_log!("console: press enter for the commissioning console");

    let (mut rx, tx) = uart.split();
    let mut cli: Cli = match Bareminal::new(tx).await {
        Ok(cli) => cli,
        Err(_) => return,
    };

    loop {
        wait_for_enter(&mut rx).await;

        CLI_ACTIVE.store(true, Ordering::Relaxed);
        cli.redraw_prompt().await.ok();
        run_session(&mut cli, &mut rx).await;
        CLI_ACTIVE.store(false, Ordering::Relaxed);
        console_log!("console: closed");
    }
}

/// Blocks (yielding to other tasks) until a bare CR or LF byte arrives,
/// discarding everything else. This is the "log mode" idle state.
async fn wait_for_enter(rx: &mut UartRx) {
    let mut byte = [0u8; 1];
    loop {
        match rx.read_exact(&mut byte).await {
            Ok(()) if matches!(byte[0], b'\r' | b'\n') => return,
            Ok(()) => {}
            Err(_) => embassy_time::Timer::after(Duration::from_millis(50)).await,
        }
    }
}

/// Feeds bytes to `cli` until the user exits (Ctrl+C, Ctrl+D, `exit`, or an
/// unrecoverable I/O error).
async fn run_session(cli: &mut Cli, rx: &mut UartRx) {
    let mut byte = [0u8; 1];
    loop {
        if rx.read_exact(&mut byte).await.is_err() {
            return;
        }
        // Ctrl+C / Ctrl+D: bareminal_cli recognizes Ctrl+C as an input event
        // but (by design) leaves acting on it to the caller; Ctrl+D isn't a
        // control code it knows at all. Handle both the same way here.
        if matches!(byte[0], 0x03 | 0x04) {
            return;
        }

        let ready = match cli.add_byte(byte[0]).await {
            Ok(ready) => ready,
            Err(_) => return,
        };
        if !ready {
            continue;
        }

        loop {
            match cli.next_command().await {
                Ok(None) => {
                    if cli.finalize().await.is_err() {
                        return;
                    }
                    break;
                }
                Ok(Some((command, writer))) => {
                    let is_exit = matches!(command, Commands::Exit);
                    if handle_command(command, writer).await.is_err() || is_exit {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    }
}

async fn respond(writer: &mut CommandWriter<UartTx>, text: &str) -> Result<(), ()> {
    writer.write_line(text.as_bytes()).await.map_err(|_| ())
}

fn str_msg(s: &str) -> String<LINE_CAP> {
    let mut out = String::new();
    let _ = out.push_str(s);
    out
}

fn fmt_msg(args: core::fmt::Arguments) -> String<LINE_CAP> {
    let mut out = String::new();
    let _ = out.write_fmt(args);
    out
}

async fn handle_command(
    command: Commands<'_>,
    writer: &mut CommandWriter<UartTx>,
) -> Result<(), ()> {
    match command {
        Commands::Show => cmd_show(writer).await,
        Commands::DeviceId(id) => respond(writer, &cmd_device_id(id).await).await,
        Commands::DeviceType(dtype) => respond(writer, &cmd_device_type(dtype).await).await,
        Commands::Hwrev(rev) => respond(writer, &cmd_hwrev(rev).await).await,
        Commands::CustomString(text) => respond(writer, &cmd_custom_string(text).await).await,
        Commands::RelaisMode(mode) => respond(writer, &cmd_relais_mode(mode).await).await,
        Commands::ExtensionMode(mode) => respond(writer, &cmd_extension_mode(mode).await).await,
        Commands::SensorName { slot, name } => {
            respond(writer, &cmd_sensor_name(slot, name).await).await
        }
        Commands::TankSensor { position, id } => {
            respond(writer, &cmd_tank_sensor(position, id).await).await
        }
        Commands::FlowSensor(id) => respond(writer, &cmd_flow_sensor(id).await).await,
        Commands::TankTarget(celsius) => respond(writer, &cmd_tank_target(celsius).await).await,
        Commands::TankRef(index) => respond(writer, &cmd_tank_ref(index).await).await,
        Commands::Heating => cmd_heating(writer).await,
        Commands::SimTarget { celsius, tank } => {
            respond(writer, &cmd_sim_target(celsius, tank != 0).await).await
        }
        Commands::SimEnergy { watts, rod } => {
            respond(writer, &cmd_sim_energy(watts, rod != 0).await).await
        }
        Commands::SimEmergency { stage, celsius } => {
            respond(writer, &cmd_sim_emergency(stage, celsius).await).await
        }
        Commands::HeatingManual(setting) => {
            respond(writer, &cmd_heating_manual(setting == OnOffArg::On).await).await
        }
        Commands::EchoRestart(setting) => {
            respond(writer, &cmd_echo_restart(setting == OnOffArg::On).await).await
        }
        Commands::Sensors => cmd_sensors(writer).await,
        Commands::SensorsForget => {
            temperature::forget_lost_sensors();
            respond(writer, "lost sensors are dropped within 10 s").await
        }
        Commands::SensorInterval(secs) => respond(writer, &cmd_sensor_interval(secs).await).await,
        Commands::Relais { num, state } => respond(writer, &cmd_relais(num, state).await).await,
        Commands::Ping => {
            send_can_message(CanMessageType::Ping, &[], false).await;
            respond(writer, "ping sent").await
        }
        Commands::Restart => {
            respond(writer, "restarting...").await.ok();
            // Give the UART a moment to actually flush the line above.
            embassy_time::Timer::after(Duration::from_millis(50)).await;
            esp_hal::system::software_reset();
        }
        Commands::Exit => Ok(()),
    }
}

async fn cmd_show(writer: &mut CommandWriter<UartTx>) -> Result<(), ()> {
    let device_id = *DEVICE_ID.lock().await;
    let device_type = *DEVICE_TYPE.lock().await;
    let uptime_minutes = device().await.uptime_minutes();

    let mut cfg = config().await;
    let hwrev = cfg.get_u8(config::Key::HardwareRevision).await;
    let relais_mode = cfg
        .get_u8(config::Key::RelaisMode)
        .await
        .map(RelaisMode::from);
    let extension_mode = cfg
        .get_u8(config::Key::ExtensionMode)
        .await
        .map(ExtensionMode::from);
    let custom_string = cfg.get_str::<8>(config::Key::CustomString).await;
    drop(cfg);

    respond(writer, &fmt_msg(format_args!("device_id: {device_id}"))).await?;
    respond(writer, &fmt_msg(format_args!("device_type: {device_type}"))).await?;
    respond(writer, &fmt_msg(format_args!("hwrev: {hwrev:?}"))).await?;
    respond(
        writer,
        &fmt_msg(format_args!("relais_mode: {relais_mode:?}")),
    )
    .await?;
    respond(
        writer,
        &fmt_msg(format_args!("extension_mode: {extension_mode:?}")),
    )
    .await?;
    match custom_string {
        Some(s) => respond(writer, &fmt_msg(format_args!("custom_string: {s}"))).await?,
        None => respond(writer, "custom_string: (unset)").await?,
    }
    respond(
        writer,
        &fmt_msg(format_args!(
            "sensor_interval: {} s",
            temperature::send_interval_secs()
        )),
    )
    .await?;
    respond(
        writer,
        &fmt_msg(format_args!(
            "echo_restart: {}",
            if echo_guard::restart_enabled() {
                "on"
            } else {
                "off"
            }
        )),
    )
    .await?;
    let sensors = temperature::sensors();
    if !sensors.is_empty() {
        let lost = sensors.iter().filter(|s| s.lost).count();
        respond(
            writer,
            &fmt_msg(format_args!(
                "sensors: {} ({lost} lost, see 'sensors')",
                sensors.len()
            )),
        )
        .await?;
    }
    respond(
        writer,
        &fmt_msg(format_args!("uptime: {uptime_minutes} min")),
    )
    .await
}

async fn cmd_device_id(id: u8) -> String<LINE_CAP> {
    if config()
        .await
        .set_u8(config::Key::DeviceId, id)
        .await
        .is_err()
    {
        return str_msg("failed to persist device_id");
    }
    *DEVICE_ID.lock().await = id;
    fmt_msg(format_args!(
        "device_id set to {id} (CAN filter needs a restart to follow it)"
    ))
}

async fn cmd_device_type(dtype: u8) -> String<LINE_CAP> {
    if config()
        .await
        .set_u8(config::Key::DeviceType, dtype)
        .await
        .is_err()
    {
        return str_msg("failed to persist device_type");
    }
    *DEVICE_TYPE.lock().await = dtype;
    fmt_msg(format_args!(
        "device_type set to {dtype} (CAN filter needs a restart to follow it)"
    ))
}

async fn cmd_hwrev(rev: u8) -> String<LINE_CAP> {
    if config()
        .await
        .set_u8(config::Key::HardwareRevision, rev)
        .await
        .is_err()
    {
        return str_msg("failed to persist hwrev");
    }
    fmt_msg(format_args!(
        "hwrev set to {rev} (takes effect after restart)"
    ))
}

async fn cmd_custom_string(text: &str) -> String<LINE_CAP> {
    let mut value: String<8> = String::new();
    let mut truncated = false;
    for ch in text.chars() {
        if value.push(ch).is_err() {
            truncated = true;
            break;
        }
    }

    if config()
        .await
        .set_str(config::Key::CustomString, &value)
        .await
        .is_err()
    {
        return str_msg("failed to persist custom_string");
    }
    if truncated {
        fmt_msg(format_args!(
            "custom_string set to \"{value}\" (truncated to 8 bytes)"
        ))
    } else {
        fmt_msg(format_args!("custom_string set to \"{value}\""))
    }
}

async fn cmd_relais_mode(mode: RelaisModeArg) -> String<LINE_CAP> {
    let value = mode.as_u8();
    if config()
        .await
        .set_u8(config::Key::RelaisMode, value)
        .await
        .is_err()
    {
        return str_msg("failed to persist relais_mode");
    }
    fmt_msg(format_args!(
        "relais_mode set to {:?} (takes effect after restart)",
        RelaisMode::from(value)
    ))
}

async fn cmd_extension_mode(mode: ExtensionModeArg) -> String<LINE_CAP> {
    let value = mode.as_u8();
    if config()
        .await
        .set_u8(config::Key::ExtensionMode, value)
        .await
        .is_err()
    {
        return str_msg("failed to persist extension_mode");
    }
    fmt_msg(format_args!(
        "extension_mode set to {:?} (takes effect after restart)",
        ExtensionMode::from(value)
    ))
}

/// Parses a sensor id as printed by `sensors` (`0x32c3b022`, with or
/// without the prefix); `none` and `0` mean "no sensor".
fn parse_sensor_id(text: &str) -> Option<u32> {
    if text.eq_ignore_ascii_case("none") {
        return Some(0);
    }
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    u32::from_str_radix(digits, 16).ok()
}

/// Formats a milli-degree value as `-12.345`.
fn fmt_millicelsius(milli: i32) -> String<12> {
    let mut out = String::new();
    let sign = if milli < 0 { "-" } else { "" };
    let _ = write!(
        out,
        "{sign}{}.{:03}",
        milli.unsigned_abs() / 1000,
        milli.unsigned_abs() % 1000
    );
    out
}

async fn cmd_sensor_name(slot: usize, name: &str) -> String<LINE_CAP> {
    let Some(name) = cancomponents_core::sensor_config::SensorName::new(name) else {
        return str_msg("a name has 1-7 characters out of A-Z a-z 0-9 - _");
    };
    let Some(sensor) = temperature::sensors().into_iter().find(|s| s.slot == slot) else {
        return str_msg("no sensor in that slot (see `sensors`)");
    };
    match crate::sensor_config::set_name(sensor.rom.serial(), name).await {
        Ok(()) => fmt_msg(format_args!("sensor {slot} is called {}", name.as_str())),
        Err(reason) => str_msg(reason),
    }
}

async fn cmd_tank_sensor(position: usize, id: &str) -> String<LINE_CAP> {
    let Some(id) = parse_sensor_id(id) else {
        return str_msg("id must be hex like 0x32c3b022, or 'none'");
    };
    match heating::set_tank_sensor(position, id).await {
        Ok(()) => fmt_msg(format_args!("tank position {position} -> id 0x{id:x}")),
        Err(()) => str_msg("position must be 0 (top) to 6 (bottom), or storing failed"),
    }
}

async fn cmd_flow_sensor(id: &str) -> String<LINE_CAP> {
    let Some(id) = parse_sensor_id(id) else {
        return str_msg("id must be hex like 0x32c3b022, or 'none'");
    };
    match heating::set_flow_sensor(id).await {
        Ok(()) => fmt_msg(format_args!("flow sensor -> id 0x{id:x}")),
        Err(()) => str_msg("failed to persist flow sensor"),
    }
}

async fn cmd_tank_target(celsius: u8) -> String<LINE_CAP> {
    match heating::set_tank_target(celsius).await {
        Ok(()) => fmt_msg(format_args!("tank target set to {celsius} C")),
        Err(()) => str_msg("tank target out of range, or storing failed"),
    }
}

async fn cmd_tank_ref(index: u8) -> String<LINE_CAP> {
    match heating::set_tank_ref(index).await {
        Ok(()) => fmt_msg(format_args!("tank reference sensor is position {index}")),
        Err(()) => str_msg("position must be 0 (top) to 6 (bottom), or storing failed"),
    }
}

const NOT_A_CONTROLLER: &str = "not a HeatingControl device";

async fn cmd_sim_target(celsius: u8, tank: bool) -> String<LINE_CAP> {
    if !heating::is_heating_controller().await {
        return str_msg(NOT_A_CONTROLLER);
    }
    heating::inject_target(HeatingTarget {
        flow_target_raw: millicelsius_to_raw(i32::from(celsius) * 1000),
        tank_mode: tank,
    });
    fmt_msg(format_args!("target {celsius} C, tank operation {tank}"))
}

async fn cmd_sim_energy(watts: i32, rod: bool) -> String<LINE_CAP> {
    if !heating::is_heating_controller().await {
        return str_msg(NOT_A_CONTROLLER);
    }
    heating::inject_energy(HeatingEnergy {
        power_w: watts,
        rod_enabled: rod,
    });
    fmt_msg(format_args!("energy {watts} W, rod released {rod}"))
}

async fn cmd_sim_emergency(stage: u8, celsius: u8) -> String<LINE_CAP> {
    if !heating::is_heating_controller().await {
        return str_msg(NOT_A_CONTROLLER);
    }
    heating::inject_emergency(HeatingEmergency {
        stage,
        tank_target_raw: millicelsius_to_raw(i32::from(celsius) * 1000),
    });
    fmt_msg(format_args!(
        "emergency stage {stage}, tank target {celsius} C"
    ))
}

async fn cmd_heating(writer: &mut CommandWriter<UartTx>) -> Result<(), ()> {
    let settings = heating::settings();
    let Some(live) = heating::live_status() else {
        return respond(writer, "heating controller is not running on this device").await;
    };
    let (out, status) = (live.outputs, live.status);

    if live.manual {
        respond(
            writer,
            &fmt_msg(format_args!(
                "MANUAL MODE: relays 6-10 are under manual control until it is turned off; \
                 circuit and mixer below show what the controller would do"
            )),
        )
        .await?;
    }

    respond(
        writer,
        &fmt_msg(format_args!(
            "rod: stage {} ({:?}){}",
            out.rod_stage,
            status.rod_reason,
            if status.over_temperature {
                "  OVER-TEMPERATURE LATCHED"
            } else {
                ""
            }
        )),
    )
    .await?;
    respond(
        writer,
        &fmt_msg(format_args!(
            "circuit: {}, pump {}, tank {}",
            if out.valves_tank {
                "tank operation"
            } else {
                "direct"
            },
            if out.pump { "on" } else { "off" },
            if status.tank_usable {
                "hot enough"
            } else {
                "too cold"
            },
        )),
    )
    .await?;
    respond(
        writer,
        &fmt_msg(format_args!(
            "mixer: {:?}, about {} % hot{}",
            out.mixer,
            status.mixer_percent,
            if status.mixer_homing { " (homing)" } else { "" }
        )),
    )
    .await?;

    let mut tank: String<LINE_CAP> = String::new();
    for (position, temp) in live.tank_temps_mc.iter().enumerate() {
        let marker = if position == usize::from(settings.tank_ref_index) {
            "*"
        } else {
            ""
        };
        match temp {
            Some(milli) => {
                let _ = write!(tank, "#{position}{marker} {}  ", fmt_millicelsius(*milli));
            }
            None => {
                let _ = write!(tank, "#{position}{marker} --  ");
            }
        }
    }
    respond(
        writer,
        &fmt_msg(format_args!(
            "tank (top to bottom, * = reference, target {} C): {tank}",
            settings.tank_target_c
        )),
    )
    .await?;
    match live.flow_mc {
        Some(milli) => {
            respond(
                writer,
                &fmt_msg(format_args!("flow: {} C", fmt_millicelsius(milli))),
            )
            .await?
        }
        None => respond(writer, "flow: --").await?,
    }

    let age = |secs: Option<u64>| -> String<16> {
        let mut out = String::new();
        match secs {
            Some(secs) => {
                let _ = write!(out, "{secs} s ago");
            }
            None => {
                let _ = write!(out, "never");
            }
        }
        out
    };
    respond(
        writer,
        &fmt_msg(format_args!(
            "messages: target {}, energy {}, emergency {}",
            age(live.target_age_s),
            age(live.energy_age_s),
            age(live.emergency_age_s)
        )),
    )
    .await
}

async fn cmd_sensors(writer: &mut CommandWriter<UartTx>) -> Result<(), ()> {
    let sensors = temperature::sensors();
    if sensors.is_empty() {
        return respond(writer, "no sensors found").await;
    }
    for sensor in sensors {
        let rom = sensor.rom.as_bytes();
        let mut line: String<LINE_CAP> = String::new();
        let _ = write!(line, "#{} rom ", sensor.slot);
        for byte in rom {
            let _ = write!(line, "{byte:02X}");
        }
        let _ = write!(line, "  id 0x{:x}  ", sensor.rom.topic_id());
        if let Some(name) = crate::sensor_config::name_of(&sensor.rom.serial()) {
            let _ = write!(line, "{}  ", name.as_str());
        }
        let role = crate::sensor_config::role_for(sensor.rom.topic_id());
        if role != cancomponents_core::sensor_config::SensorRole::None {
            let _ = write!(line, "[{}]  ", role.name());
        }
        match (sensor.reading, sensor.lost) {
            (Some(reading), _) => {
                let milli = reading.temperature.millicelsius();
                let sign = if milli < 0 { "-" } else { "" };
                let _ = write!(
                    line,
                    "{sign}{}.{:03} C  ({} s ago)",
                    milli.unsigned_abs() / 1000,
                    milli.unsigned_abs() % 1000,
                    reading.at.elapsed().as_secs(),
                );
            }
            (None, true) => {
                let _ = write!(line, "LOST");
            }
            (None, false) if sensor.settling => {
                let _ = write!(line, "settling");
            }
            (None, false) => {
                let _ = write!(line, "no reading yet");
            }
        }
        respond(writer, &line).await?;
    }
    Ok(())
}

async fn cmd_echo_restart(enabled: bool) -> String<LINE_CAP> {
    match echo_guard::set_restart_enabled(enabled).await {
        Ok(()) => fmt_msg(format_args!(
            "restart on missing echo: {}",
            if enabled { "on" } else { "off" }
        )),
        Err(()) => str_msg("failed to persist echo_restart"),
    }
}

async fn cmd_sensor_interval(secs: u32) -> String<LINE_CAP> {
    match temperature::set_send_interval_secs(secs).await {
        Ok(applied) => fmt_msg(format_args!("sensor_interval set to {applied} s")),
        Err(()) => str_msg("failed to persist sensor_interval"),
    }
}

async fn cmd_heating_manual(enabled: bool) -> String<LINE_CAP> {
    if !heating::is_heating_controller().await {
        return str_msg(NOT_A_CONTROLLER);
    }
    heating::set_manual(enabled);
    if enabled {
        str_msg(
            "manual mode: relays 6-10 are yours (all off now), `relais <6-10> on|off`; \
             it stays on until `heating-manual off` (a restart ends it as well)",
        )
    } else {
        str_msg("automatic mode: relays 6-10 off, the mixer homes again, then the controller takes over")
    }
}

async fn cmd_relais(num: usize, state: RelaisStateArg) -> String<LINE_CAP> {
    if cancomponents_core::heating::is_controlled_relay(num)
        && heating::is_heating_controller().await
    {
        // Rod relays: only the controller, always. The others only in
        // manual mode, one by one.
        if num < 6 {
            return fmt_msg(format_args!(
                "relay {num} is part of the heating rod; only the controller switches it"
            ));
        }
        let on = match state {
            RelaisStateArg::On => true,
            RelaisStateArg::Off => false,
            RelaisStateArg::Up | RelaisStateArg::Down => {
                return str_msg("use on or off for this relay");
            }
        };
        return match heating::manual_relay(num, on).await {
            Ok(()) => fmt_msg(format_args!(
                "relay {num} -> {} (manual)",
                if on { "on" } else { "off" }
            )),
            Err(reason) => str_msg(reason),
        };
    }
    let state: State = state.into();
    send_relais_command(RelaisMessage {
        num,
        state,
        duration: Duration::from_millis(0),
        bank: 0,
    })
    .await;
    fmt_msg(format_args!("relais {num} -> {state:?}"))
}
