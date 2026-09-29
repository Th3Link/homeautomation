//! Interactive commissioning console, sharing UART0 with `esp_println`'s
//! debug output (see [`crate::logging`]). Structurally identical to
//! `cancomponent-rs/cc-hardware`'s `cli.rs` (same passive-until-Enter,
//! `CLI_ACTIVE`-muted-logging, `bareminal_cli`-driven design) — see that
//! module's doc comment for the session lifecycle.
//!
//! The command set covers what `ConsoleCommandDevice.cpp` exposed (device
//! id/type/hwrev/custom-string/bitrate) plus the WiFi/MQTT/web-auth fields
//! `Command::save_config` accepts over `/control.json` — unlike a
//! `cancomponent-rs` node, this gateway needs WiFi/MQTT credentials before
//! its own web UI is even reachable, so the serial console has to be able
//! to bootstrap them too. The original's generic `nvs_set`/`nvs_get`
//! passthrough (`cmd_nvs.c`) isn't reproduced — every value it could touch
//! now has its own typed command here instead.

use crate::can::{send_can_message, DEVICE_ID, DEVICE_TYPE};
use crate::config::{self, config};
use crate::console_log;
use crate::logging::CLI_ACTIVE;
use bareminal_cli::cli::{Bareminal, CommandWriter};
use bareminal_macros::Command;
use core::fmt::Write as _;
use core::str::FromStr;
use core::sync::atomic::Ordering;
use embassy_executor::Spawner;
use embassy_time::Duration;
use embedded_io_async::Read;
use esp_hal::gpio::{InputPin, OutputPin};
use esp_hal::uart;
use esp_hal::Async;
use gateway_core::can_message_type::CanMessageType;
use gateway_core::config::{Bitrate, WifiMode};
use gateway_core::device_type::DeviceType;
use heapless::String;

const LINE_CAP: usize = 160;
const MAX_CMD_BUFFER: usize = 192;
const HISTORY_SIZE: usize = 8;

type Uart = uart::Uart<'static, Async>;
type UartTx = uart::UartTx<'static, Async>;
type UartRx = uart::UartRx<'static, Async>;
type Cli = Bareminal<Commands<'static>, UartTx, MAX_CMD_BUFFER, HISTORY_SIZE>;

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

#[derive(Debug, Command)]
enum Commands<'a> {
    /// Show all current configuration values.
    Show,
    /// Set this gateway's own CAN device id (used when it addresses
    /// itself, e.g. `Available` broadcasts).
    DeviceId(u8),
    /// Set this gateway's own CAN device type.
    #[set(one_of = [
        "unknown", "legacyrelais", "legacylamps", "button", "relais",
        "gateway", "rollershutter", "ssr",
    ])]
    DeviceType(DeviceTypeArg),
    /// Set the hardware revision.
    Hwrev(u8),
    /// Set an 8-byte identification string.
    CustomString(&'a str),
    /// Set the CAN bus bitrate.
    #[set(one_of = ["b22_222", "b25", "b50", "b100"])]
    Bitrate(BitrateArg),
    /// Set the WiFi mode.
    #[set(one_of = ["client", "ap", "off"])]
    WifiMode(WifiModeArg),
    /// Set the WiFi SSID (station or access-point mode).
    WifiSsid(&'a str),
    /// Set the WiFi password (station or access-point mode; under 8
    /// characters forces access-point mode with the setup-AP fallback
    /// credentials, same as the web UI).
    WifiPassword(&'a str),
    /// Set the DHCP/web hostname.
    Hostname(&'a str),
    /// Set the MQTT broker URI, e.g. `mqtt://192.168.1.10:1883`.
    MqttUri(&'a str),
    /// Set the MQTT username.
    MqttUser(&'a str),
    /// Set the MQTT password (under 8 characters is rejected, same as the
    /// web UI).
    MqttPassword(&'a str),
    /// Enable/disable MQTT.
    #[set(one_of = ["on", "off"])]
    MqttEnabled(OnOff),
    /// Set the web UI's Basic Auth username.
    WebUsername(&'a str),
    /// Set the web UI's Basic Auth password.
    WebPassword(&'a str),
    /// Broadcast a CAN Ping, for bus-wiring sanity checks.
    Ping,
    /// Restart the gateway.
    Restart,
    /// Leave the console (same as Ctrl+C / Ctrl+D).
    Exit,
}

#[derive(Debug, Clone, Copy)]
struct DeviceTypeArg(DeviceType);

impl FromStr for DeviceTypeArg {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(match s {
            "unknown" => DeviceType::Unknown,
            "legacyrelais" => DeviceType::LegacyRelais,
            "legacylamps" => DeviceType::LegacyLamps,
            "button" => DeviceType::Button,
            "relais" => DeviceType::Relais,
            "gateway" => DeviceType::Gateway,
            "rollershutter" => DeviceType::Rollershutter,
            "ssr" => DeviceType::SSR,
            _ => return Err(()),
        }))
    }
}

#[derive(Debug, Clone, Copy)]
struct BitrateArg(Bitrate);

impl FromStr for BitrateArg {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(match s {
            "b22_222" | "b25" | "b50" | "b100" => Bitrate::from_name(s),
            _ => return Err(()),
        }))
    }
}

#[derive(Debug, Clone, Copy)]
struct WifiModeArg(WifiMode);

impl FromStr for WifiModeArg {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "client" | "ap" | "off" => Ok(Self(WifiMode::from_name(s))),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum OnOff {
    On,
    Off,
}

impl FromStr for OnOff {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
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

async fn run_session(cli: &mut Cli, rx: &mut UartRx) {
    let mut byte = [0u8; 1];
    loop {
        if rx.read_exact(&mut byte).await.is_err() {
            return;
        }
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
        Commands::DeviceType(t) => respond(writer, &cmd_device_type(t.0).await).await,
        Commands::Hwrev(rev) => respond(writer, &cmd_hwrev(rev).await).await,
        Commands::CustomString(s) => respond(writer, &cmd_custom_string(s).await).await,
        Commands::Bitrate(b) => respond(writer, &cmd_bitrate(b.0).await).await,
        Commands::WifiMode(m) => respond(writer, &cmd_wifi_mode(m.0).await).await,
        Commands::WifiSsid(s) => {
            respond(
                writer,
                &cmd_str_field(s, config::Key::WifiSsid, "wifi_ssid").await,
            )
            .await
        }
        Commands::WifiPassword(s) => {
            respond(
                writer,
                &cmd_str_field(s, config::Key::WifiPassword, "wifi_password").await,
            )
            .await
        }
        Commands::Hostname(s) => {
            respond(
                writer,
                &cmd_str_field(s, config::Key::Hostname, "hostname").await,
            )
            .await
        }
        Commands::MqttUri(s) => {
            respond(
                writer,
                &cmd_str_field(s, config::Key::MqttUri, "mqtt_uri").await,
            )
            .await
        }
        Commands::MqttUser(s) => {
            respond(
                writer,
                &cmd_str_field(s, config::Key::MqttUser, "mqtt_user").await,
            )
            .await
        }
        Commands::MqttPassword(s) => respond(writer, &cmd_mqtt_password(s).await).await,
        Commands::MqttEnabled(v) => {
            respond(writer, &cmd_mqtt_enabled(matches!(v, OnOff::On)).await).await
        }
        Commands::WebUsername(s) => {
            respond(
                writer,
                &cmd_str_field(s, config::Key::WebUsername, "web_username").await,
            )
            .await
        }
        Commands::WebPassword(s) => {
            respond(
                writer,
                &cmd_str_field(s, config::Key::WebPassword, "web_password").await,
            )
            .await
        }
        Commands::Ping => {
            send_can_message(CanMessageType::Available, &[], true).await;
            respond(writer, "ping sent").await
        }
        Commands::Restart => {
            respond(writer, "restarting...").await.ok();
            embassy_time::Timer::after(Duration::from_millis(50)).await;
            esp_hal::system::software_reset();
        }
        Commands::Exit => Ok(()),
    }
}

async fn cmd_show(writer: &mut CommandWriter<UartTx>) -> Result<(), ()> {
    let device_id = DEVICE_ID.load(Ordering::Relaxed);
    let device_type = DEVICE_TYPE.load(Ordering::Relaxed);
    let cfg = config().await.load_or_init().await;

    respond(writer, &fmt_msg(format_args!("device_id: {device_id}"))).await?;
    respond(
        writer,
        &fmt_msg(format_args!(
            "device_type: {device_type} ({})",
            DeviceType::from(device_type).name()
        )),
    )
    .await?;
    respond(writer, &fmt_msg(format_args!("hwrev: {}", cfg.hw_rev))).await?;
    respond(
        writer,
        &fmt_msg(format_args!("custom_string: {}", cfg.custom_string)),
    )
    .await?;
    respond(
        writer,
        &fmt_msg(format_args!("can_bitrate: {}", cfg.can_bitrate.as_str())),
    )
    .await?;
    respond(
        writer,
        &fmt_msg(format_args!("wifi_mode: {}", cfg.wifi_mode.as_str())),
    )
    .await?;
    respond(
        writer,
        &fmt_msg(format_args!("wifi_ssid: {}", cfg.wifi_ssid)),
    )
    .await?;
    respond(writer, &fmt_msg(format_args!("hostname: {}", cfg.hostname))).await?;
    respond(
        writer,
        &fmt_msg(format_args!("mqtt_enabled: {}", cfg.mqtt_enabled)),
    )
    .await?;
    respond(writer, &fmt_msg(format_args!("mqtt_uri: {}", cfg.mqtt_uri))).await?;
    respond(
        writer,
        &fmt_msg(format_args!("mqtt_user: {}", cfg.mqtt_user)),
    )
    .await?;
    respond(
        writer,
        &fmt_msg(format_args!("web_username: {}", cfg.web_username)),
    )
    .await?;
    respond(
        writer,
        &fmt_msg(format_args!("update_delay_ms: {}", cfg.update_delay_ms)),
    )
    .await
}

async fn cmd_device_id(id: u8) -> String<LINE_CAP> {
    if config().await.set_u8(config::Key::CanId, id).await.is_err() {
        return str_msg("failed to persist device_id");
    }
    DEVICE_ID.store(id, Ordering::Relaxed);
    fmt_msg(format_args!("device_id set to {id}"))
}

async fn cmd_device_type(t: DeviceType) -> String<LINE_CAP> {
    let value: u8 = t.into();
    if config()
        .await
        .set_u8(config::Key::CanType, value)
        .await
        .is_err()
    {
        return str_msg("failed to persist device_type");
    }
    DEVICE_TYPE.store(value, Ordering::Relaxed);
    fmt_msg(format_args!("device_type set to {}", t.name()))
}

async fn cmd_hwrev(rev: u8) -> String<LINE_CAP> {
    if config()
        .await
        .set_u8(config::Key::HwRev, rev)
        .await
        .is_err()
    {
        return str_msg("failed to persist hwrev");
    }
    fmt_msg(format_args!("hwrev set to {rev}"))
}

async fn cmd_custom_string(text: &str) -> String<LINE_CAP> {
    let truncated = text.len() > 8;
    let mut end = text.len().min(8);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut value: String<8> = String::new();
    let _ = value.push_str(&text[..end]);

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

async fn cmd_bitrate(bitrate: Bitrate) -> String<LINE_CAP> {
    if config()
        .await
        .set_u8(config::Key::CanBitrate, bitrate.into())
        .await
        .is_err()
    {
        return str_msg("failed to persist can_bitrate");
    }
    fmt_msg(format_args!("can_bitrate set to {}", bitrate.as_str()))
}

async fn cmd_wifi_mode(mode: WifiMode) -> String<LINE_CAP> {
    if config()
        .await
        .set_str(config::Key::WifiMode, mode.as_str())
        .await
        .is_err()
    {
        return str_msg("failed to persist wifi_mode");
    }
    fmt_msg(format_args!(
        "wifi_mode set to {} (takes effect after restart)",
        mode.as_str()
    ))
}

async fn cmd_str_field(value: &str, key: config::Key, name: &str) -> String<LINE_CAP> {
    if config().await.set_str(key, value).await.is_err() {
        return fmt_msg(format_args!("failed to persist {name}"));
    }
    fmt_msg(format_args!("{name} set to \"{value}\""))
}

async fn cmd_mqtt_password(value: &str) -> String<LINE_CAP> {
    if !gateway_core::command::should_apply_mqtt_password(value) {
        return str_msg("mqtt_password rejected: must be at least 8 characters");
    }
    cmd_str_field(value, config::Key::MqttPassword, "mqtt_password").await
}

async fn cmd_mqtt_enabled(enabled: bool) -> String<LINE_CAP> {
    if config()
        .await
        .set_bool(config::Key::MqttEnabled, enabled)
        .await
        .is_err()
    {
        return str_msg("failed to persist mqtt_enabled");
    }
    fmt_msg(format_args!("mqtt_enabled set to {enabled}"))
}
