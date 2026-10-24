//! MQTT client: connect/reconnect loop, resubscribing every topic on
//! (re)connect (mirrors `MQTT::dispatch`'s `connected_event()` fan-out —
//! each bridge resubscribes on every reconnect, not just once), and an
//! outgoing publish queue any other module can push to via [`publish`].
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
use embassy_futures::select::{select, Either};
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use heapless::String;
use rust_mqtt::buffer::BumpBuffer;
use rust_mqtt::client::event::Event;
use rust_mqtt::client::options::{
    ConnectOptions, PublicationOptions, SubscriptionOptions, TopicReference,
};
use rust_mqtt::client::Client;
use rust_mqtt::types::{MqttBinary, MqttString, TopicFilter, TopicName};

/// The gateway's client instantiation: a little headroom (4) for
/// in-flight un-acked SUBSCRIBE/incoming-QoS/outgoing-QoS packets, and
/// none of subscription identifiers, user properties, or topic aliases
/// (we don't use any of those MQTT 5 features).
type MqttClient<'a, 'c> = Client<'a, 'c, TcpSocket<'a>, BumpBuffer<'c>, 4, 4, 4, 0, 0, 0, 0>;

/// Topic + body length caps generous enough for every topic/body this
/// crate builds (see [`gateway_core::topics`]).
const TOPIC_CAP: usize = 96;
const BODY_CAP: usize = 200;

pub struct Publication {
    pub topic: String<TOPIC_CAP>,
    pub body: String<BODY_CAP>,
}

/// Outgoing publishes queued by any module via [`publish`], drained by
/// [`mqtt_task`].
pub static PUBLISH_QUEUE: Channel<CriticalSectionRawMutex, Publication, 16> = Channel::new();

/// Queues `topic`/`body` for publication once connected. Silently dropped
/// if the queue is full — matches the original's `IMQTT::publish` being a
/// silent no-op while disconnected (not queued either).
pub async fn publish(topic: &str, body: &str) {
    let mut p = Publication {
        topic: String::new(),
        body: String::new(),
    };
    let _ = p.topic.push_str(topic);
    let _ = p.body.push_str(body);
    let _ = PUBLISH_QUEUE.try_send(p);
}

/// Every topic the gateway subscribes to on (re)connect — one entry per
/// `Bridge*::connected_event()` in the original.
const SUBSCRIBE_TOPICS: &[&str] = &[
    "canbus/relais_command/#",
    "canbus/rollershutter_command/#",
    "canbus/lamp_command/#",
    "canbus/nightlight_command/#",
    "canbus/debug/#",
];

#[embassy_executor::task]
pub async fn mqtt_task(stack: Stack<'static>) {
    loop {
        let cfg = config().await.load_or_init().await;
        if !cfg.mqtt_enabled {
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }

        let Some((host, port)) = gateway_core::helper::parse_broker_uri(&cfg.mqtt_uri) else {
            console_log!("mqtt: invalid broker uri \"{}\"", cfg.mqtt_uri);
            Timer::after(Duration::from_secs(10)).await;
            continue;
        };

        // IPv4 first, IPv6 (AAAA) as the fallback; IP literals of either
        // family resolve to themselves.
        let addr = match stack.dns_query(host, DnsQueryType::A).await {
            Ok(addrs) if !addrs.is_empty() => addrs[0],
            _ => match stack.dns_query(host, DnsQueryType::Aaaa).await {
                Ok(addrs) if !addrs.is_empty() => addrs[0],
                _ => {
                    console_log!("mqtt: could not resolve \"{host}\"");
                    Timer::after(Duration::from_secs(10)).await;
                    continue;
                }
            },
        };

        let mut rx_buf = [0u8; 2048];
        let mut tx_buf = [0u8; 2048];
        let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
        if let Err(e) = socket.connect((addr, port)).await {
            console_log!("mqtt: tcp connect failed: {e:?}");
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }

        let mut bump_storage = [0u8; 2048];
        let mut buffer = BumpBuffer::new(&mut bump_storage);
        // Const generics: SUBSCRIBE/RECEIVE/SEND_MAXIMUM bound in-flight
        // un-acked packets of each kind (we only ever have one outstanding
        // at a time, but leave a little headroom); the gateway uses none
        // of subscription identifiers, user properties, or topic aliases.
        let mut client: MqttClient<'_, '_> = Client::new(&mut buffer);

        let mut connect_options = ConnectOptions::new().clean_start();
        if !cfg.mqtt_user.is_empty() {
            if let Ok(user) = MqttString::from_str(cfg.mqtt_user.as_str()) {
                connect_options = connect_options.user_name(user);
            }
        }
        if !cfg.mqtt_password.is_empty() {
            if let Ok(pw) = MqttBinary::from_slice(cfg.mqtt_password.as_bytes()) {
                connect_options = connect_options.password(pw);
            }
        }

        let client_id = MqttString::from_str("gateway-hardware").ok();
        if let Err(e) = client.connect(socket, &connect_options, client_id).await {
            console_log!("mqtt: connect failed: {e:?}");
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }
        console_log!("mqtt: connected to {host}:{port}");

        for topic in SUBSCRIBE_TOPICS {
            let Ok(filter_string) = MqttString::from_str(topic) else {
                continue;
            };
            let Some(filter) = TopicFilter::new(filter_string) else {
                continue;
            };
            let _ = client.subscribe(filter, &SubscriptionOptions::new()).await;
        }

        crate::translate::publish_self_available().await;

        // Drains the outgoing publish queue while polling for incoming
        // messages, dispatching them into `crate::translate`. Returns to
        // the outer reconnect loop once the connection fails.
        loop {
            match select(PUBLISH_QUEUE.receive(), client.poll()).await {
                Either::First(publication) => {
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
                Either::Second(Ok(Event::Publish(msg))) => {
                    if let TopicReference::Name(name) = &msg.topic {
                        if let Ok(body) = core::str::from_utf8(msg.message.as_bytes()) {
                            crate::translate::dispatch_mqtt(name.as_ref().as_str(), body).await;
                        }
                    }
                }
                Either::Second(Ok(_)) => {}
                Either::Second(Err(_)) => break,
            }
        }

        console_log!("mqtt: disconnected, reconnecting");
        Timer::after(Duration::from_secs(2)).await;
    }
}
