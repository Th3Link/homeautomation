//! Interactive commissioning console, sharing UART0 with `esp_println`'s
//! debug output (see [`crate::logging`]). Structurally identical to
//! `cancomponent-rs/cc-hardware`'s `cli.rs` (same passive-until-Enter,
//! `CLI_ACTIVE`-muted-logging, `bareminal_cli`-driven design) — see that
//! module's doc comment for the session lifecycle.
//!
//! The command set covers what `ConsoleCommandDevice.cpp` exposed (device
//! id/type/hwrev/custom-string/bitrate) plus the hostname/MQTT/web-auth
//! fields `Command::save_config` accepts over `/control.json` — unlike a
//! `cancomponent-rs` node, this gateway needs MQTT credentials before it
//! is useful, so the serial console has to be able to bootstrap them (the
//! network side needs no setup: DHCPv4 plus IPv6 SLAAC on the wired
//! link). The original's generic `nvs_set`/`nvs_get`
//! passthrough (`cmd_nvs.c`) isn't reproduced — every value it could touch
//! now has its own typed command here instead.

use crate::can::{send_can_message, DEVICE_ID, DEVICE_TYPE};
use crate::config::{self, config, LoadOrInit};
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
use gateway_core::config::Bitrate;
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
    /// Set the GPIO the Ethernet PHY's 50 MHz RMII clock comes out on (16 or
    /// 17, depending on the board revision; takes effect after a restart).
    #[set(min = 16, max = 17)]
    EthClock(u8),
    /// Set the CAN bus bitrate.
    #[set(one_of = ["b22_222", "b25", "b50", "b100"])]
    Bitrate(BitrateArg),
    /// Set the hostname sent to the DHCP server (takes effect after restart).
    Hostname(&'a str),
    /// Set the first MQTT broker's URI, e.g. `mqtt://192.168.1.10:1883`;
    /// `mqtts://host[:port]` encrypts the connection with TLS.
    MqttUri(&'a str),
    /// Set the first MQTT broker's username.
    MqttUser(&'a str),
    /// Set the first MQTT broker's password (under 8 characters is
    /// rejected, same as the web UI).
    MqttPassword(&'a str),
    /// Enable/disable the first MQTT broker.
    #[set(one_of = ["on", "off"])]
    MqttEnabled(OnOff),
    /// With `mqtts://`, accept any certificate the first broker presents
    /// (encrypted, but not authenticated) instead of the pinned one.
    #[set(one_of = ["on", "off"])]
    MqttInsecure(OnOff),
    /// Pin the first broker's TLS certificate: the next connection to it
    /// takes the certificate it presents (it must be self-signed) and
    /// stores it; from then on only that certificate is accepted. Check the
    /// SHA-256 fingerprint printed in the log against the broker's own.
    MqttTrust,
    /// Forget the first broker's pinned certificate.
    MqttUntrust,
    /// Set the second MQTT broker's URI (see `mqtt-uri`).
    Mqtt2Uri(&'a str),
    /// Set the second MQTT broker's username.
    Mqtt2User(&'a str),
    /// Set the second MQTT broker's password (under 8 characters is
    /// rejected).
    Mqtt2Password(&'a str),
    /// Enable/disable the second MQTT broker.
    #[set(one_of = ["on", "off"])]
    Mqtt2Enabled(OnOff),
    /// With `mqtts://`, accept any certificate the second broker presents.
    #[set(one_of = ["on", "off"])]
    Mqtt2Insecure(OnOff),
    /// Pin the second broker's TLS certificate (see `mqtt-trust`).
    Mqtt2Trust,
    /// Forget the second broker's pinned certificate.
    Mqtt2Untrust,
    /// Set the web UI's Basic Auth username.
    WebUsername(&'a str),
    /// Set the web UI's Basic Auth password.
    WebPassword(&'a str),
    /// Serve the web UI over HTTPS on port 443 with port 80 redirecting
    /// (on, the default), or over plain HTTP on port 80 (off: the way back
    /// in if HTTPS doesn't work). Takes effect after a restart.
    #[set(one_of = ["on", "off"])]
    WebHttps(OnOff),
    /// Show the network: link, MAC, IPv4 and IPv6 addresses, gateways, DNS
    /// servers and the time.
    Network,
    /// Show the HTTPS certificate: mode, FQDN, validity, status.
    TlsStatus,
    /// Set the FQDN the HTTPS certificate is for (empty: the hostname). In
    /// self-signed mode a new certificate is made right away.
    TlsFqdn(&'a str),
    /// Make a new self-signed HTTPS certificate (and switch to that mode).
    TlsSelfSigned,
    /// Set the NTP server the time comes from; `auto`: the one DHCPv6
    /// names, else pool.ntp.org.
    NtpServer(&'a str),
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
            "heatingcontrol" => DeviceType::HeatingControl,
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
        Commands::EthClock(gpio) => respond(writer, &cmd_eth_clock(gpio).await).await,
        Commands::CustomString(s) => respond(writer, &cmd_custom_string(s).await).await,
        Commands::Bitrate(b) => respond(writer, &cmd_bitrate(b.0).await).await,
        Commands::Hostname(s) => {
            respond(
                writer,
                &cmd_str_field(s, config::Key::Hostname, "hostname").await,
            )
            .await
        }
        Commands::MqttUri(s) => respond(writer, &cmd_mqtt_uri(0, s).await).await,
        Commands::MqttUser(s) => {
            let keys = config::Key::broker(0);
            respond(writer, &cmd_str_field(s, keys.user, "mqtt_user").await).await
        }
        Commands::MqttPassword(s) => respond(writer, &cmd_mqtt_password(0, s).await).await,
        Commands::MqttEnabled(v) => {
            respond(writer, &cmd_mqtt_enabled(0, matches!(v, OnOff::On)).await).await
        }
        Commands::MqttInsecure(v) => {
            respond(writer, &cmd_mqtt_insecure(0, matches!(v, OnOff::On)).await).await
        }
        Commands::MqttTrust => respond(writer, &cmd_mqtt_trust(0).await).await,
        Commands::MqttUntrust => respond(writer, &cmd_mqtt_untrust(0).await).await,
        Commands::Mqtt2Trust => respond(writer, &cmd_mqtt_trust(1).await).await,
        Commands::Mqtt2Untrust => respond(writer, &cmd_mqtt_untrust(1).await).await,
        Commands::Mqtt2Uri(s) => respond(writer, &cmd_mqtt_uri(1, s).await).await,
        Commands::Mqtt2User(s) => {
            let keys = config::Key::broker(1);
            respond(writer, &cmd_str_field(s, keys.user, "mqtt2_user").await).await
        }
        Commands::Mqtt2Password(s) => respond(writer, &cmd_mqtt_password(1, s).await).await,
        Commands::Mqtt2Enabled(v) => {
            respond(writer, &cmd_mqtt_enabled(1, matches!(v, OnOff::On)).await).await
        }
        Commands::Mqtt2Insecure(v) => {
            respond(writer, &cmd_mqtt_insecure(1, matches!(v, OnOff::On)).await).await
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
        Commands::WebHttps(v) => {
            respond(writer, &cmd_web_https(matches!(v, OnOff::On)).await).await
        }
        Commands::Network => cmd_network(writer).await,
        // ~15 KB of state (the certificate status): on the heap while it
        // runs, rather than in this task's static memory, which comes out
        // of the main stack's room.
        Commands::TlsStatus => crate::heap::boxed(|| cmd_tls_status(writer)).await,
        Commands::TlsFqdn(s) => respond(writer, &cmd_tls_fqdn(s).await).await,
        Commands::TlsSelfSigned => respond(writer, &cmd_tls_self_signed().await).await,
        Commands::NtpServer(s) => respond(writer, &cmd_ntp_server(s).await).await,
        Commands::Ping => {
            send_can_message(CanMessageType::Available, &[], true).await;
            respond(writer, "ping sent").await
        }
        Commands::Restart => {
            respond(writer, "restarting...").await.ok();
            embassy_time::Timer::after(Duration::from_millis(50)).await;
            crate::dhcpv6::release().await;
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
        &fmt_msg(format_args!(
            "eth_clock: GPIO{}",
            config::eth_clock_gpio().await
        )),
    )
    .await?;
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
    respond(writer, &fmt_msg(format_args!("hostname: {}", cfg.hostname))).await?;
    for (slot, broker) in cfg.brokers.iter().enumerate() {
        let name = if slot == 0 { "mqtt" } else { "mqtt2" };
        respond(
            writer,
            &fmt_msg(format_args!("{name}_enabled: {}", broker.enabled)),
        )
        .await?;
        respond(writer, &fmt_msg(format_args!("{name}_uri: {}", broker.uri))).await?;
        respond(
            writer,
            &fmt_msg(format_args!("{name}_user: {}", broker.user)),
        )
        .await?;
        if broker.uri.starts_with("mqtts://") {
            let pinned = crate::config::pinned_certificate(slot).await;
            let line = if broker.tls_insecure {
                fmt_msg(format_args!("{name}_tls: insecure (any certificate)"))
            } else if let Some(certificate) = pinned {
                let digest = crate::tls::fingerprint(&certificate);
                fmt_msg(format_args!(
                    "{name}_tls: pinned, SHA-256 {:02x}{:02x}{:02x}{:02x}...",
                    digest[0], digest[1], digest[2], digest[3]
                ))
            } else {
                fmt_msg(format_args!(
                    "{name}_tls: NO certificate pinned (run {name}-trust)"
                ))
            };
            respond(writer, &line).await?;
        }
    }
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

async fn cmd_eth_clock(gpio: u8) -> String<LINE_CAP> {
    match config::set_eth_clock_gpio(gpio).await {
        Ok(()) => fmt_msg(format_args!(
            "eth_clock set to GPIO{gpio} (takes effect after restart)"
        )),
        Err(()) => str_msg("eth_clock must be 16 or 17, or storing failed"),
    }
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

async fn cmd_str_field(value: &str, key: config::Key, name: &str) -> String<LINE_CAP> {
    if config().await.set_str(key, value).await.is_err() {
        return fmt_msg(format_args!("failed to persist {name}"));
    }
    fmt_msg(format_args!("{name} set to \"{value}\""))
}

/// `mqtt` for the first broker slot, `mqtt2` for the second.
fn broker_name(slot: usize) -> &'static str {
    if slot == 0 {
        "mqtt"
    } else {
        "mqtt2"
    }
}

async fn cmd_mqtt_uri(slot: usize, value: &str) -> String<LINE_CAP> {
    if gateway_core::helper::parse_broker_uri(value).is_none() {
        return str_msg("uri must look like mqtt://host[:port] or mqtts://host[:port]");
    }
    let name = broker_name(slot);
    let key = config::Key::broker(slot).uri;
    let mut message = cmd_str_field(value, key, &format_name(name, "uri")).await;
    if value.starts_with("mqtts://") {
        let _ = message.push_str(" - run mqtt-trust once to pin its certificate");
    }
    message
}

fn format_name(name: &str, field: &str) -> String<16> {
    let mut out = String::new();
    let _ = write!(out, "{name}_{field}");
    out
}

async fn cmd_mqtt_password(slot: usize, value: &str) -> String<LINE_CAP> {
    if !gateway_core::command::should_apply_mqtt_password(value) {
        return str_msg("mqtt password rejected: must be at least 8 characters");
    }
    let key = config::Key::broker(slot).password;
    cmd_str_field(value, key, &format_name(broker_name(slot), "password")).await
}

async fn cmd_mqtt_enabled(slot: usize, enabled: bool) -> String<LINE_CAP> {
    let name = format_name(broker_name(slot), "enabled");
    let key = config::Key::broker(slot).enabled;
    if config().await.set_bool(key, enabled).await.is_err() {
        return fmt_msg(format_args!("failed to persist {name}"));
    }
    fmt_msg(format_args!("{name} set to {enabled}"))
}

async fn cmd_mqtt_trust(slot: usize) -> String<LINE_CAP> {
    crate::mqtt::request_trust(slot);
    fmt_msg(format_args!(
        "{}: the next connection pins the broker's certificate - watch the log for its fingerprint",
        broker_name(slot)
    ))
}

async fn cmd_mqtt_untrust(slot: usize) -> String<LINE_CAP> {
    let name = broker_name(slot);
    match config::set_pinned_certificate(slot, &[]).await {
        Ok(()) => fmt_msg(format_args!("{name}: pinned certificate forgotten")),
        Err(()) => fmt_msg(format_args!("{name}: failed to forget the certificate")),
    }
}

async fn cmd_mqtt_insecure(slot: usize, insecure: bool) -> String<LINE_CAP> {
    let name = format_name(broker_name(slot), "insecure");
    let key = config::Key::broker(slot).tls_insecure;
    if config().await.set_bool(key, insecure).await.is_err() {
        return fmt_msg(format_args!("failed to persist {name}"));
    }
    fmt_msg(format_args!("{name} set to {insecure}"))
}

async fn cmd_web_https(on: bool) -> String<LINE_CAP> {
    match crate::cert::set_https_enabled(on).await {
        Ok(()) if on => str_msg("web: HTTPS on port 443, port 80 redirects (after restart)"),
        Ok(()) => str_msg("web: plain HTTP on port 80 (after restart)"),
        Err(_) => str_msg("failed to persist web-https"),
    }
}

async fn cmd_tls_status(writer: &mut CommandWriter<UartTx>) -> Result<(), ()> {
    use gateway_core::x509::time::Rfc3339;
    let status = crate::cert::status().await;
    respond(
        writer,
        &fmt_msg(format_args!(
            "https: {}, mode {}, fqdn {}",
            if status.https_enabled { "on" } else { "off" },
            status.settings.mode.code(),
            status.effective_fqdn
        )),
    )
    .await?;
    match &status.certificate {
        Some((info, state, origin)) => {
            respond(
                writer,
                &fmt_msg(format_args!(
                    "certificate: {} ({}), issuer {}",
                    state.code(),
                    origin.code(),
                    info.issuer
                )),
            )
            .await?;
            respond(
                writer,
                &fmt_msg(format_args!(
                    "valid {} .. {}",
                    Rfc3339(info.not_before),
                    Rfc3339(info.not_after)
                )),
            )
            .await?;
            let f = &info.fingerprint;
            respond(
                writer,
                &fmt_msg(format_args!(
                    "SHA-256 {:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}...",
                    f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7]
                )),
            )
            .await?;
        }
        None => respond(writer, "certificate: none").await?,
    }
    respond(
        writer,
        if status.time_known {
            "time: known (SNTP)"
        } else {
            "time: unknown (no SNTP answer yet)"
        },
    )
    .await
}

async fn cmd_tls_fqdn(fqdn: &str) -> String<LINE_CAP> {
    let update = crate::cert::SettingsUpdate {
        fqdn: Some(fqdn),
        ..Default::default()
    };
    match crate::cert::update_settings(update).await {
        Ok(()) => fmt_msg(format_args!("tls fqdn: {fqdn}")),
        Err(e) => fmt_msg(format_args!("tls fqdn refused: {}", e.code())),
    }
}

async fn cmd_tls_self_signed() -> String<LINE_CAP> {
    // Switching the mode to self-signed makes the certificate by itself.
    let result = if crate::cert::settings().await.mode == crate::cert::Mode::SelfSigned {
        crate::cert::generate_self_signed(None).await
    } else {
        let update = crate::cert::SettingsUpdate {
            mode: Some(crate::cert::Mode::SelfSigned),
            ..Default::default()
        };
        crate::cert::update_settings(update).await
    };
    match result {
        Ok(()) => str_msg("tls: new self-signed certificate"),
        Err(e) => fmt_msg(format_args!("tls: failed: {}", e.code())),
    }
}

async fn cmd_ntp_server(server: &str) -> String<LINE_CAP> {
    let auto = server == "auto";
    let update = crate::cert::SettingsUpdate {
        ntp_server: Some(if auto { "" } else { server }),
        ..Default::default()
    };
    match crate::cert::update_settings(update).await {
        Ok(()) if auto => fmt_msg(format_args!(
            "ntp server: automatic, now {} (next sync)",
            crate::cert::ntp_server().await
        )),
        Ok(()) => fmt_msg(format_args!("ntp server: {server} (next sync)")),
        Err(e) => fmt_msg(format_args!("ntp server refused: {}", e.code())),
    }
}

/// The DHCPv6 client's state, and what it got beyond the address and DNS
/// servers (shown above with the others).
fn dhcpv6_lines() -> heapless::Vec<String<LINE_CAP>, 3> {
    use core::fmt::Write;
    use dhcpeav6_client::{Mode, State};

    let now = embassy_time::Instant::now().as_millis();
    let seconds_until = |at: dhcpeav6_client::Instant| at.as_millis().saturating_sub(now) / 1000;
    crate::dhcpv6::with_status(|status| {
        let mut lines = heapless::Vec::new();
        let state = match status.state {
            State::Idle => "idle",
            State::Soliciting => "looking for a server",
            State::Requesting => "requesting an address",
            State::Bound => "bound",
            State::Renewing => "renewing",
            State::Rebinding => "rebinding",
            State::InformationRequesting => "asking for configuration",
            State::Informed => "configured",
            State::Releasing => "releasing",
        };
        let mode = match status.mode {
            Mode::Off => "off",
            Mode::Stateless => "stateless",
            Mode::Stateful => "stateful",
        };
        let mut line = String::new();
        let _ = write!(line, "dhcpv6: {state} ({mode})");
        if let Some(config) = &status.configuration {
            let _ = write!(line, ", server {}", config.server_id);
            if config.renew_at != dhcpeav6_client::Instant::NEVER {
                let _ = write!(line, ", renew in {} s", seconds_until(config.renew_at));
            }
            if config.rebind_at != dhcpeav6_client::Instant::NEVER {
                let _ = write!(line, ", rebind in {} s", seconds_until(config.rebind_at));
            }
        }
        let _ = lines.push(line);
        let Some(config) = &status.configuration else {
            return lines;
        };
        if !config.domains.is_empty() {
            let mut line = String::new();
            let _ = write!(line, "dhcpv6 domains:");
            for domain in config.domains.iter() {
                let _ = write!(line, " {domain}");
            }
            let _ = lines.push(line);
        }
        if !config.ntp_servers.is_empty() || !config.ntp_names.is_empty() {
            let mut line = String::new();
            let _ = write!(line, "dhcpv6 ntp:");
            for server in &config.ntp_servers {
                let _ = write!(line, " {server}");
            }
            for name in config.ntp_names.iter() {
                let _ = write!(line, " {name}");
            }
            let _ = lines.push(line);
        }
        lines
    })
}

async fn cmd_network(writer: &mut CommandWriter<UartTx>) -> Result<(), ()> {
    let Some(net) = crate::ethernet::info().await else {
        return respond(writer, "network: not started (no Ethernet PHY)").await;
    };
    let m = net.mac;
    respond(
        writer,
        &fmt_msg(format_args!(
            "link: {}, MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            if net.link_up { "up" } else { "down" },
            m[0],
            m[1],
            m[2],
            m[3],
            m[4],
            m[5]
        )),
    )
    .await?;

    match &net.v4 {
        Some(v4) => {
            respond(
                writer,
                &fmt_msg(format_args!("ipv4: {} (DHCP)", v4.address)),
            )
            .await?;
            match v4.gateway {
                Some(gateway) => {
                    respond(writer, &fmt_msg(format_args!("ipv4 gateway: {gateway}"))).await?
                }
                None => respond(writer, "ipv4 gateway: none").await?,
            }
            for server in &v4.dns_servers {
                respond(writer, &fmt_msg(format_args!("ipv4 dns: {server}"))).await?;
            }
        }
        None => respond(writer, "ipv4: no DHCP lease").await?,
    }

    for cidr in &net.v6_addresses {
        let origin = net.v6_origin(cidr);
        respond(writer, &fmt_msg(format_args!("ipv6: {cidr} ({origin})"))).await?;
    }
    match net.v6.as_ref().and_then(|v6| v6.gateway) {
        Some(gateway) => respond(writer, &fmt_msg(format_args!("ipv6 gateway: {gateway}"))).await?,
        None => respond(writer, "ipv6 gateway: none").await?,
    }
    let rdnss = net.v6.iter().flat_map(|v6| v6.dns_servers.iter());
    for server in rdnss {
        respond(writer, &fmt_msg(format_args!("ipv6 dns: {server} (RDNSS)"))).await?;
    }
    for server in net.dhcpv6.iter().flat_map(|lease| lease.dns_servers.iter()) {
        respond(
            writer,
            &fmt_msg(format_args!("ipv6 dns: {server} (DHCPv6)")),
        )
        .await?;
    }
    let flags = net.router_flags;
    respond(
        writer,
        &fmt_msg(format_args!(
            "ipv6 routers: {}{}",
            if flags.managed {
                "M (DHCPv6 addresses) "
            } else {
                ""
            },
            if flags.other {
                "O (DHCPv6 configuration)"
            } else if flags.managed {
                ""
            } else {
                "no DHCPv6"
            }
        )),
    )
    .await?;
    for line in dhcpv6_lines() {
        respond(writer, &line).await?;
    }

    let ntp = crate::cert::ntp_server().await;
    match crate::wall_clock::now() {
        Some(now) => {
            respond(
                writer,
                &fmt_msg(format_args!(
                    "time: {} (SNTP from {ntp})",
                    gateway_core::x509::time::Rfc3339(now)
                )),
            )
            .await
        }
        None => {
            respond(
                writer,
                &fmt_msg(format_args!("time: unknown (no answer from {ntp} yet)")),
            )
            .await
        }
    }
}
