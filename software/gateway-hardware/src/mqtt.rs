//! MQTT client: connect/reconnect loop, resubscribing every topic on
//! (re)connect (mirrors `MQTT::dispatch`'s `connected_event()` fan-out —
//! each bridge resubscribes on every reconnect, not just once), and an
//! outgoing publish queue any other module can push to via [`publish`].
//!
//! The gateway serves up to [`MQTT_BROKERS`] brokers at the same time (e.g.
//! Home Assistant's and a Cerbo's): one [`mqtt_task`] per configured broker
//! slot, each with its own connection and publish queue. Commands are
//! accepted from every broker and go onto the CAN bus the same way.
//!
//! The brokers aren't equal, though: slot [`PRIMARY`] is the one the gateway
//! reports to — everything it publishes goes there ([`publish`]) — while
//! slot [`SECONDARY`] only listens. Publishing to the secondary broker is
//! an exception that has to be asked for explicitly ([`publish_secondary`]).
//!
//! Wire protocol note: the original used ESP-IDF's `esp-mqtt`, which
//! speaks MQTT 3.1.1. The `rust-mqtt` crate used here only implements
//! MQTT 5.0 (its `v3` feature is a currently-unused placeholder upstream)
//! — there is no maintained `no_std`/`embedded-io-async` MQTT 3.1.1 client
//! to reach for instead. Any broker modern enough to matter (Mosquitto,
//! EMQX, HiveMQ, ...) speaks both, so this is a wire-version change with
//! no expected behavioral difference for pub/sub against a real broker.

use crate::config::{config, LoadOrInit};
use crate::console_log;
use crate::tls::{self, Trust};
use core::num::NonZero;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_futures::select::{select3, Either3};
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use gateway_core::config::BrokerConfig;
use gateway_core::config::MQTT_BROKERS;
use gateway_core::helper::BrokerAddress;
use heapless::String;
use heapless::Vec;
use rust_mqtt::buffer::BumpBuffer;
use rust_mqtt::client::event::{Event, Publish};
use rust_mqtt::client::options::{
    ConnectOptions, PublicationOptions, SubscriptionOptions, TopicReference,
};
use rust_mqtt::client::Client;
use rust_mqtt::io::Transport;
use rust_mqtt::types::{MqttBinary, MqttString, PacketIdentifier, TopicFilter, TopicName};
use static_cell::ConstStaticCell;

/// The gateway's client instantiation: a little headroom (4) for
/// in-flight un-acked SUBSCRIBE/incoming-QoS/outgoing-QoS packets, and
/// none of subscription identifiers, user properties, or topic aliases
/// (we don't use any of those MQTT 5 features).
type MqttClient<'a, 'c, N> = Client<'a, 'c, N, BumpBuffer<'c>, 4, 4, 4, 0, 0, 0, 0>;

/// Topic + body length caps generous enough for every topic/body this
/// crate builds (see [`gateway_core::topics`]).
const TOPIC_CAP: usize = 96;
const BODY_CAP: usize = 200;

/// The client's buffer for what it receives, and so the largest packet it
/// takes ([`forget_received`]). The broker is told (MQTT 5's maximum packet
/// size) and drops a larger message for this client instead of sending it:
/// it would end the connection, and a retained one every one after it.
const RECEIVE_BYTES: usize = 2048;
const MAXIMUM_PACKET_SIZE: NonZero<u32> = NonZero::new(RECEIVE_BYTES as u32).unwrap();

pub struct Publication {
    pub topic: String<TOPIC_CAP>,
    pub body: String<BODY_CAP>,
}

/// The broker slot everything is published to.
pub const PRIMARY: usize = 0;
/// The broker slot that only receives, apart from explicit exceptions.
pub const SECONDARY: usize = 1;

/// Outgoing publishes, one queue per broker slot, drained by that slot's
/// [`mqtt_task`].
static PUBLISH_QUEUES: [Channel<CriticalSectionRawMutex, Publication, 16>; MQTT_BROKERS] =
    [Channel::new(), Channel::new()];

/// Set to make a slot's task drop its connection and connect again (with
/// whatever configuration is stored by then), see [`reconnect`].
static RECONNECT: [Signal<CriticalSectionRawMutex, ()>; MQTT_BROKERS] =
    [Signal::new(), Signal::new()];

/// Makes broker `slot`'s task reconnect, so that changed settings (or a
/// requested certificate capture) take effect right away.
pub fn reconnect(slot: usize) {
    RECONNECT[slot].signal(());
}

/// Whether broker `slot` is connected and subscribed.
pub fn is_connected(slot: usize) -> bool {
    CONNECTED[slot].load(Ordering::Relaxed)
}

/// Whether each broker slot is connected and subscribed, i.e. worth queueing
/// publishes for.
static CONNECTED: [AtomicBool; MQTT_BROKERS] = [AtomicBool::new(false), AtomicBool::new(false)];

fn publication(topic: &str, body: &str) -> Publication {
    let mut p = Publication {
        topic: String::new(),
        body: String::new(),
    };
    let _ = p.topic.push_str(topic);
    let _ = p.body.push_str(body);
    p
}

/// Queues `topic`/`body` for publication on the primary broker. Silently
/// dropped if its queue is full, and not queued at all while it is
/// disconnected — matches the original's `IMQTT::publish` being a silent
/// no-op while disconnected.
pub async fn publish(topic: &str, body: &str) {
    publish_if_connected(PRIMARY, topic, body);
}

/// Queues `topic`/`body` for publication on the secondary broker, which
/// otherwise only receives. Nothing uses this yet; it is the one place to
/// start from when a message has to reach the secondary broker too.
#[allow(dead_code)]
pub async fn publish_secondary(topic: &str, body: &str) {
    publish_if_connected(SECONDARY, topic, body);
}

fn publish_if_connected(slot: usize, topic: &str, body: &str) {
    if CONNECTED[slot].load(Ordering::Relaxed) {
        let _ = PUBLISH_QUEUES[slot].try_send(publication(topic, body));
    }
}

/// Queues `topic`/`body` for a broker that is just being connected to (it
/// isn't marked as connected yet).
pub async fn publish_on_connect(slot: usize, topic: &str, body: &str) {
    let _ = PUBLISH_QUEUES[slot].try_send(publication(topic, body));
}

/// Every topic the gateway subscribes to on (re)connect — one entry per
/// `Bridge*::connected_event()` in the original.
const SUBSCRIBE_TOPICS: &[&str] = &[
    "canbus/relais_command/#",
    "canbus/rollershutter_command/#",
    "canbus/lamp_command/#",
    "canbus/nightlight_command/#",
    "canbus/heating_target_command/#",
    "canbus/heating_energy_command/#",
    "canbus/heating_emergency_command/#",
    "canbus/debug/#",
    "canbus/gateway/restart",
];

/// Hands one received PUBLISH to the CAN/MQTT translation.
async fn dispatch_publish(msg: &Publish<'_, 0, 0>) {
    if let TopicReference::Name(name) = &msg.topic {
        if let Ok(body) = core::str::from_utf8(msg.message.as_bytes()) {
            crate::translate::dispatch_mqtt(name.as_ref().as_str(), body, msg.retain).await;
        }
    }
}

/// Gives the client's buffer back for the next packets. The client keeps
/// what it receives there (the topic and payload of every PUBLISH, reason
/// strings) and never frees it itself: without this, the next message after
/// [`RECEIVE_BYTES`] of them doesn't fit, and the connection fails with it.
///
/// # Safety
///
/// Nothing the client returned before may still be in use.
unsafe fn forget_received<N: Transport>(client: &mut MqttClient<'_, '_, N>) {
    // SAFETY: passed on to the caller.
    unsafe { client.buffer_mut().reset() };
}

/// Waits for the SUBACK of the SUBSCRIBE with packet identifier `pid`,
/// still dispatching any message that arrives meanwhile (e.g. a retained
/// one the broker sends right after subscribing). Returns false if the
/// connection failed or the broker never answered.
async fn wait_for_suback<N: Transport>(
    client: &mut MqttClient<'_, '_, N>,
    pid: PacketIdentifier,
    topic: &str,
) -> bool {
    let wait = async {
        loop {
            // SAFETY: the previous event was dropped at the end of the
            // last iteration.
            unsafe { forget_received(client) };
            match client.poll().await {
                Ok(Event::Suback(suback)) if suback.packet_identifier == pid => {
                    if !suback.reason_code.is_success() {
                        console_log!(
                            "mqtt: subscription to {topic} refused: {:?}",
                            suback.reason_code
                        );
                    }
                    return true;
                }
                Ok(Event::Publish(msg)) => dispatch_publish(&msg).await,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    };
    embassy_time::with_timeout(Duration::from_secs(5), wait)
        .await
        .unwrap_or(false)
}

/// Subscribes to every command topic (one at a time, each confirmed before
/// the next: the client only tracks `SUBSCRIBE_MAXIMUM` unconfirmed ones,
/// and anything beyond that is refused — and was once silently never
/// subscribed to), announces the gateway and then shuttles messages both
/// ways until the connection fails.
async fn serve<N: Transport>(client: &mut MqttClient<'_, '_, N>, slot: usize) {
    let broker = slot + 1;

    for topic in SUBSCRIBE_TOPICS {
        let Ok(filter_string) = MqttString::from_str(topic) else {
            continue;
        };
        let Some(filter) = TopicFilter::new(filter_string) else {
            continue;
        };
        match client.subscribe(filter, &SubscriptionOptions::new()).await {
            Ok(pid) => {
                if !wait_for_suback(client, pid, topic).await {
                    console_log!("mqtt{broker}: subscribing failed, reconnecting");
                    return;
                }
            }
            Err(e) => console_log!("mqtt{broker}: subscribe to {topic} failed: {e:?}"),
        }
    }

    // A reconnect requested while connecting is moot: the settings in use
    // were read after it.
    RECONNECT[slot].reset();
    // Anything queued while this broker wasn't connected is stale by now.
    while PUBLISH_QUEUES[slot].try_receive().is_ok() {}
    if slot == PRIMARY {
        crate::translate::publish_self_available(slot).await;
    }
    CONNECTED[slot].store(true, Ordering::Relaxed);

    // Drains this broker's publish queue while polling for incoming
    // messages, dispatching them into `crate::translate`.
    loop {
        // SAFETY: the previous event was dropped at the end of the last
        // iteration; the subscription loop above keeps nothing either.
        unsafe { forget_received(client) };
        match select3(
            PUBLISH_QUEUES[slot].receive(),
            client.poll(),
            RECONNECT[slot].wait(),
        )
        .await
        {
            Either3::First(publication) => {
                let Ok(topic_string) = MqttString::from_str(publication.topic.as_str()) else {
                    continue;
                };
                let Some(topic_name) = TopicName::new(topic_string) else {
                    continue;
                };
                let options = PublicationOptions::new(TopicReference::Name(topic_name));
                if client
                    .publish(&options, publication.body.as_str().into())
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Either3::Second(Ok(Event::Publish(msg))) => dispatch_publish(&msg).await,
            Either3::Second(Ok(_)) => {}
            Either3::Second(Err(_)) | Either3::Third(()) => break,
        }
    }
    CONNECTED[slot].store(false, Ordering::Relaxed);
}

/// How long a TLS handshake may take (the key exchange and, for RSA, the
/// signature check are slow on this CPU, so be generous).
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// Slots whose broker certificate should be captured and pinned on the next
/// connection (see [`request_trust`]).
static TRUST_REQUESTED: [AtomicBool; MQTT_BROKERS] =
    [AtomicBool::new(false), AtomicBool::new(false)];

/// Asks broker `slot`'s task to take the certificate its broker presents on
/// its next connection and pin it (`mqtt-trust`).
pub fn request_trust(slot: usize) {
    TRUST_REQUESTED[slot].store(true, Ordering::Relaxed);
}

/// Connects the MQTT client over `net` and serves the session until it ends.
async fn session<N: Transport>(
    net: N,
    settings: &BrokerConfig,
    slot: usize,
    address: &BrokerAddress<'_>,
) {
    let broker = slot + 1;
    let mut bump_storage = [0u8; RECEIVE_BYTES];
    let mut buffer = BumpBuffer::new(&mut bump_storage);
    // Const generics: SUBSCRIBE/RECEIVE/SEND_MAXIMUM bound in-flight
    // un-acked packets of each kind (we only ever have one outstanding
    // at a time, but leave a little headroom); the gateway uses none
    // of subscription identifiers, user properties, or topic aliases.
    let mut client: MqttClient<'_, '_, N> = Client::new(&mut buffer);

    let mut connect_options = ConnectOptions::new()
        .clean_start()
        .maximum_packet_size(MAXIMUM_PACKET_SIZE);
    if !settings.user.is_empty() {
        if let Ok(user) = MqttString::from_str(settings.user.as_str()) {
            connect_options = connect_options.user_name(user);
        }
    }
    if !settings.password.is_empty() {
        if let Ok(pw) = MqttBinary::from_slice(settings.password.as_bytes()) {
            connect_options = connect_options.password(pw);
        }
    }

    let client_id = MqttString::from_str("gateway-hardware").ok();
    if let Err(e) = client.connect(net, &connect_options, client_id).await {
        console_log!("mqtt{broker}: connect failed: {e:?}");
        Timer::after(Duration::from_secs(5)).await;
        return;
    }
    console_log!(
        "mqtt{broker}: connected to {}:{}{}",
        address.host,
        address.port,
        if address.tls { " (TLS)" } else { "" }
    );

    serve(&mut client, slot).await;

    console_log!("mqtt{broker}: disconnected, reconnecting");
    Timer::after(Duration::from_secs(2)).await;
}

/// The buffers of one broker connection. Static, in place from the start:
/// in the task's future they would make it ~16 KB larger, and a task's
/// future is built on the main stack when it is spawned.
struct Buffers {
    rx: [u8; 2048],
    tx: [u8; 2048],
    read_record: [u8; tls::READ_RECORD_BYTES],
    write_record: [u8; tls::WRITE_RECORD_BYTES],
    captured: Vec<u8, { tls::CERT_CAP }>,
}

impl Buffers {
    const fn new() -> Self {
        Self {
            rx: [0; 2048],
            tx: [0; 2048],
            read_record: [0; tls::READ_RECORD_BYTES],
            write_record: [0; tls::WRITE_RECORD_BYTES],
            captured: Vec::new(),
        }
    }
}

static BUFFERS: [ConstStaticCell<Buffers>; MQTT_BROKERS] =
    [const { ConstStaticCell::new(Buffers::new()) }; MQTT_BROKERS];

/// The connection to one broker (slot 0 or 1), kept up forever: connects,
/// serves, and reconnects after a pause when the connection is lost or the
/// configuration changes. A slot that isn't enabled just idles.
#[embassy_executor::task(pool_size = 2)]
pub async fn mqtt_task(stack: Stack<'static>, slot: usize) {
    let broker = slot + 1;
    let buffers = BUFFERS[slot].take();
    loop {
        let cfg = config().await.load_or_init().await;
        let settings = &cfg.brokers[slot];
        if !settings.enabled {
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }

        let Some(address) = gateway_core::helper::parse_broker_uri(&settings.uri) else {
            console_log!("mqtt{broker}: invalid broker uri \"{}\"", settings.uri);
            Timer::after(Duration::from_secs(10)).await;
            continue;
        };

        // What an `mqtts://` connection has to satisfy, decided before
        // connecting at all.
        let capturing = address.tls && TRUST_REQUESTED[slot].swap(false, Ordering::Relaxed);
        let pinned = if address.tls && !capturing && !settings.tls_insecure {
            let pinned = crate::config::pinned_certificate(slot).await;
            if pinned.is_none() {
                console_log!(
                    "mqtt{broker}: no certificate pinned for {} - run mqtt{}-trust \
                     (or turn mqtt{}-insecure on)",
                    address.host,
                    if slot == 0 { "" } else { "2" },
                    if slot == 0 { "" } else { "2" },
                );
                // Wake up early if the user asks to pin the certificate now.
                for _ in 0..30 {
                    if TRUST_REQUESTED[slot].load(Ordering::Relaxed) {
                        break;
                    }
                    Timer::after(Duration::from_secs(1)).await;
                }
                continue;
            }
            pinned
        } else {
            None
        };

        let addresses = crate::resolve::resolve(stack, address.host).await;
        if addresses.is_empty() {
            console_log!("mqtt{broker}: could not resolve \"{}\"", address.host);
            Timer::after(Duration::from_secs(10)).await;
            continue;
        }

        let mut socket = TcpSocket::new(stack, &mut buffers.rx, &mut buffers.tx);
        if let Err(e) = crate::resolve::connect(&mut socket, &addresses, address.port).await {
            console_log!("mqtt{broker}: tcp connect failed: {e:?}");
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }

        if !address.tls {
            session(socket, settings, slot, &address).await;
            continue;
        }

        buffers.captured.clear();
        let trust = if capturing {
            Trust::Capture(&mut buffers.captured)
        } else if let Some(certificate) = &pinned {
            Trust::Pinned(certificate)
        } else {
            Trust::AnyCertificate
        };
        let opened = embassy_time::with_timeout(
            TLS_HANDSHAKE_TIMEOUT,
            tls::open(
                socket,
                &mut buffers.read_record,
                &mut buffers.write_record,
                trust,
            ),
        )
        .await;
        match opened {
            Ok(Ok(connection)) if capturing => {
                drop(connection);
                pin_captured(slot, &buffers.captured).await;
                Timer::after(Duration::from_secs(1)).await;
            }
            Ok(Ok(connection)) => session(connection, settings, slot, &address).await,
            Ok(Err(e)) => {
                console_log!("mqtt{broker}: TLS handshake failed: {e:?}");
                if pinned.is_some() {
                    console_log!(
                        "mqtt{broker}: the broker's certificate doesn't match the pinned one \
                         (new key? run mqtt-trust again)"
                    );
                }
                Timer::after(Duration::from_secs(10)).await;
            }
            Err(_) => {
                console_log!("mqtt{broker}: TLS handshake timed out");
                Timer::after(Duration::from_secs(10)).await;
            }
        }
    }
}

/// Stores the certificate a capturing connection saw and reports its
/// fingerprint.
async fn pin_captured(slot: usize, certificate: &[u8]) {
    let broker = slot + 1;
    let digest = tls::fingerprint(certificate);
    let stored = crate::config::set_pinned_certificate(slot, certificate)
        .await
        .is_ok();
    console_log!(
        "mqtt{broker}: {} certificate ({} bytes), SHA-256 {:02x?}",
        if stored { "pinned" } else { "COULD NOT STORE" },
        certificate.len(),
        digest
    );
}
