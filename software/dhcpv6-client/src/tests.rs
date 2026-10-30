//! The client against a scripted server.

use crate::wire::{msg, ntp, opt, status, IaNa, Message, Writer};
use crate::{Client, ClientConfig, Duid, Instant, Mode, State};
use core::net::Ipv6Addr;
use std::string::ToString;
use std::vec::Vec;

const MAC: [u8; 6] = [0x58, 0xbf, 0x25, 0xee, 0x55, 0xfc];
const IAID: u32 = 7;
const SERVER: &[u8] = &[0, 3, 0, 1, 2, 0, 0, 0, 0, 1];
const OTHER_SERVER: &[u8] = &[0, 3, 0, 1, 2, 0, 0, 0, 0, 2];
const LEASED: Ipv6Addr = Ipv6Addr::new(0xfd77, 0x77, 0x77, 0, 0, 0, 0, 0x100);
const DNS: Ipv6Addr = Ipv6Addr::new(0xfd77, 0x77, 0x77, 0, 0, 0, 0, 1);

type TestClient = Client<2, 3, 64>;

fn client() -> TestClient {
    Client::new(ClientConfig {
        duid: Duid::link_layer(MAC),
        iaid: IAID,
        seed: 1,
    })
}

fn at(millis: u64) -> Instant {
    Instant::from_millis(millis)
}

/// A message the client sent.
struct Sent {
    bytes: Vec<u8>,
}

impl Sent {
    fn message(&self) -> Message<'_> {
        Message::parse(&self.bytes).unwrap()
    }

    fn kind(&self) -> u8 {
        self.message().kind
    }

    fn xid(&self) -> u32 {
        self.message().xid
    }

    fn option(&self, code: u16) -> Option<&[u8]> {
        self.message().option(code)
    }

    fn requested(&self) -> Vec<u16> {
        self.option(opt::ORO)
            .unwrap_or_default()
            .chunks(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect()
    }

    fn elapsed(&self) -> u16 {
        let data = self.option(opt::ELAPSED_TIME).unwrap();
        u16::from_be_bytes([data[0], data[1]])
    }

    fn ia_addresses(&self) -> Vec<Ipv6Addr> {
        let ia = IaNa::parse(self.option(opt::IA_NA).unwrap()).unwrap();
        assert_eq!(ia.iaid, IAID);
        ia.addresses().map(|a| a.address).collect()
    }
}

/// Polls at `now`, expecting a message.
fn send(client: &mut TestClient, now: Instant) -> Sent {
    let mut buf = [0u8; 1024];
    let len = client
        .poll(now, &mut buf)
        .unwrap()
        .expect("a message is due");
    Sent {
        bytes: buf[..len].to_vec(),
    }
}

fn nothing_to_send(client: &mut TestClient, now: Instant) {
    let mut buf = [0u8; 1024];
    assert_eq!(client.poll(now, &mut buf), Ok(None));
}

/// Polls at `poll_at` until a message goes out (a timer may first start an
/// exchange, delayed).
fn send_when_due(client: &mut TestClient) -> (Instant, Sent) {
    for _ in 0..3 {
        let now = client.poll_at().expect("something pending");
        let mut buf = [0u8; 1024];
        if let Some(len) = client.poll(now, &mut buf).unwrap() {
            let bytes = buf[..len].to_vec();
            return (now, Sent { bytes });
        }
    }
    panic!("no message due");
}

/// A server message answering `to`; `body` adds options after the client
/// and server IDs.
fn answer(
    kind: u8,
    to: &Sent,
    server: &[u8],
    body: impl FnOnce(&mut Writer<'_>) -> Result<(), crate::BufferTooSmall>,
) -> Vec<u8> {
    let mut buf = [0u8; 1024];
    let mut w = Writer::new(&mut buf);
    w.header(kind, to.xid()).unwrap();
    w.option(opt::CLIENT_ID, |w| {
        w.bytes(to.option(opt::CLIENT_ID).unwrap())
    })
    .unwrap();
    w.option(opt::SERVER_ID, |w| w.bytes(server)).unwrap();
    body(&mut w).unwrap();
    let len = w.len();
    buf[..len].to_vec()
}

/// An IA_NA option with one address.
fn ia_na(
    w: &mut Writer<'_>,
    t1: u32,
    t2: u32,
    address: Ipv6Addr,
    preferred: u32,
    valid: u32,
) -> Result<(), crate::BufferTooSmall> {
    w.option(opt::IA_NA, |w| {
        w.u32(IAID)?;
        w.u32(t1)?;
        w.u32(t2)?;
        w.option(opt::IA_ADDR, |w| {
            w.bytes(&address.octets())?;
            w.u32(preferred)?;
            w.u32(valid)
        })
    })
}

fn ia_na_status(w: &mut Writer<'_>, code: u16) -> Result<(), crate::BufferTooSmall> {
    w.option(opt::IA_NA, |w| {
        w.u32(IAID)?;
        w.u32(0)?;
        w.u32(0)?;
        w.option(opt::STATUS_CODE, |w| w.u16(code))
    })
}

fn dns(w: &mut Writer<'_>) -> Result<(), crate::BufferTooSmall> {
    w.option(opt::DNS_SERVERS, |w| w.bytes(&DNS.octets()))
}

/// A client that has leased [`LEASED`] (T1 100 s, T2 160 s, lifetimes
/// 200/300 s) at the returned time.
fn bound() -> (TestClient, Instant) {
    let mut client = client();
    client.set_mode(Mode::Stateful, at(0));
    let (_, solicit) = send_when_due(&mut client);
    let advertise = answer(msg::ADVERTISE, &solicit, SERVER, |w| {
        w.option(opt::PREFERENCE, |w| w.bytes(&[255]))?;
        ia_na(w, 0, 0, LEASED, 200, 300)
    });
    client.receive(at(1100), &advertise);
    let request = send(&mut client, at(1100));
    assert_eq!(request.kind(), msg::REQUEST);
    let reply = answer(msg::REPLY, &request, SERVER, |w| {
        ia_na(w, 100, 160, LEASED, 200, 300)?;
        dns(w)
    });
    client.receive(at(1200), &reply);
    assert_eq!(client.state(), State::Bound);
    assert!(client.configuration_changed());
    (client, at(1200))
}

#[test]
fn off_does_nothing() {
    let mut client = client();
    assert_eq!(client.poll_at(), None);
    nothing_to_send(&mut client, at(5000));
    assert_eq!(client.state(), State::Idle);
}

#[test]
fn stateless_configuration() {
    let mut client = client();
    client.set_mode(Mode::Stateless, at(0));
    assert_eq!(client.state(), State::InformationRequesting);
    // Delayed by up to INF_MAX_DELAY.
    let (now, request) = send_when_due(&mut client);
    assert!(now < at(1000));
    assert_eq!(request.kind(), msg::INFORMATION_REQUEST);
    assert_eq!(
        request.option(opt::CLIENT_ID),
        Some(Duid::link_layer(MAC).as_bytes())
    );
    assert_eq!(request.option(opt::IA_NA), None);
    assert_eq!(request.option(opt::SERVER_ID), None);
    assert_eq!(request.elapsed(), 0);
    let requested = request.requested();
    for code in [
        opt::DNS_SERVERS,
        opt::DOMAIN_LIST,
        opt::NTP_SERVER,
        opt::SNTP_SERVERS,
        opt::INFORMATION_REFRESH_TIME,
        opt::INF_MAX_RT,
    ] {
        assert!(requested.contains(&code), "{code} requested");
    }

    let ntp_server = Ipv6Addr::new(0xfd77, 0x77, 0x77, 0, 0, 0, 0, 0x123);
    let sntp_server = Ipv6Addr::new(0xfd77, 0x77, 0x77, 0, 0, 0, 0, 0x124);
    let reply = answer(msg::REPLY, &request, SERVER, |w| {
        dns(w)?;
        w.option(opt::DOMAIN_LIST, |w| {
            w.bytes(b"\x03lan\x00\x07example\x03com\x00")
        })?;
        w.option(opt::NTP_SERVER, |w| {
            w.option(ntp::SERVER_ADDRESS, |w| w.bytes(&ntp_server.octets()))?;
            w.option(ntp::SERVER_FQDN, |w| w.bytes(b"\x04time\x03lan\x00"))
        })?;
        w.option(opt::SNTP_SERVERS, |w| {
            w.bytes(&sntp_server.octets())?;
            w.bytes(&ntp_server.octets())
        })?;
        w.option(opt::INFORMATION_REFRESH_TIME, |w| w.u32(3600))
    });
    client.receive(now.plus_millis(5), &reply);
    assert_eq!(client.state(), State::Informed);
    assert!(client.configuration_changed());
    assert!(!client.configuration_changed());

    let config = client.configuration().unwrap();
    assert_eq!(config.server_id.as_bytes(), SERVER);
    assert!(config.addresses.is_empty());
    assert_eq!(config.dns_servers, [DNS]);
    let domains: Vec<_> = config.domains.iter().map(|n| n.to_string()).collect();
    assert_eq!(domains, ["lan", "example.com"]);
    assert_eq!(config.ntp_servers, [ntp_server, sntp_server]);
    let ntp_names: Vec<_> = config.ntp_names.iter().map(|n| n.to_string()).collect();
    assert_eq!(ntp_names, ["time.lan"]);

    // Refreshed after the Information Refresh Time.
    let refresh = now.plus_millis(5 + 3_600_000);
    assert_eq!(client.poll_at(), Some(refresh));
    nothing_to_send(&mut client, at(refresh.as_millis() - 1));
    assert_eq!(client.state(), State::Informed);
    let (_, again) = send_when_due(&mut client);
    assert_eq!(client.state(), State::InformationRequesting);
    assert_eq!(again.kind(), msg::INFORMATION_REQUEST);
    assert_ne!(again.xid(), request.xid());
    // The configuration stays meanwhile.
    assert!(client.configuration().is_some());

    // The same answer again is no change.
    let reply = answer(msg::REPLY, &again, SERVER, |w| {
        dns(w)?;
        w.option(opt::DOMAIN_LIST, |w| {
            w.bytes(b"\x03lan\x00\x07example\x03com\x00")
        })?;
        w.option(opt::NTP_SERVER, |w| {
            w.option(ntp::SERVER_ADDRESS, |w| w.bytes(&ntp_server.octets()))?;
            w.option(ntp::SERVER_FQDN, |w| w.bytes(b"\x04time\x03lan\x00"))
        })?;
        w.option(opt::SNTP_SERVERS, |w| w.bytes(&sntp_server.octets()))
    });
    client.receive(refresh.plus_millis(2000), &reply);
    assert_eq!(client.state(), State::Informed);
    assert!(!client.configuration_changed());
}

#[test]
fn information_refresh_time_has_a_minimum_and_a_default() {
    for (option, expected_secs) in [(Some(10), 600), (None, 86_400)] {
        let mut client = client();
        client.set_mode(Mode::Stateless, at(0));
        let (now, request) = send_when_due(&mut client);
        let reply = answer(msg::REPLY, &request, SERVER, |w| match option {
            Some(secs) => w.option(opt::INFORMATION_REFRESH_TIME, |w| w.u32(secs)),
            None => Ok(()),
        });
        client.receive(now, &reply);
        assert_eq!(
            client.poll_at(),
            Some(now.plus_millis(expected_secs * 1000))
        );
    }
}

#[test]
fn foreign_answers_are_ignored() {
    let mut client = client();
    client.set_mode(Mode::Stateless, at(0));
    let (now, request) = send_when_due(&mut client);

    // Another transaction.
    let mut reply = answer(msg::REPLY, &request, SERVER, dns);
    reply[3] ^= 1;
    client.receive(now, &reply);
    // Another client.
    let mut buf = [0u8; 256];
    let mut w = Writer::new(&mut buf);
    w.header(msg::REPLY, request.xid()).unwrap();
    w.option(opt::CLIENT_ID, |w| w.bytes(&[0, 3, 0, 1, 1, 2, 3, 4, 5, 6]))
        .unwrap();
    w.option(opt::SERVER_ID, |w| w.bytes(SERVER)).unwrap();
    let len = w.len();
    client.receive(now, &buf[..len]);
    // No server ID.
    let mut w = Writer::new(&mut buf);
    w.header(msg::REPLY, request.xid()).unwrap();
    w.option(opt::CLIENT_ID, |w| {
        w.bytes(Duid::link_layer(MAC).as_bytes())
    })
    .unwrap();
    let len = w.len();
    client.receive(now, &buf[..len]);
    // An Advertise is no answer to an Information-request.
    client.receive(now, &answer(msg::ADVERTISE, &request, SERVER, dns));
    // Truncated.
    let reply = answer(msg::REPLY, &request, SERVER, dns);
    client.receive(now, &reply[..reply.len() - 1]);

    assert_eq!(client.state(), State::InformationRequesting);
    assert!(client.configuration().is_none());
    assert!(!client.configuration_changed());
}

#[test]
fn stateful_lease() {
    let mut client = client();
    client.set_mode(Mode::Stateful, at(0));
    assert_eq!(client.state(), State::Soliciting);
    let (now, solicit) = send_when_due(&mut client);
    assert!(now < at(1000));
    assert_eq!(solicit.kind(), msg::SOLICIT);
    assert!(solicit.ia_addresses().is_empty());
    assert_eq!(solicit.option(opt::SERVER_ID), None);
    assert!(solicit.requested().contains(&opt::SOL_MAX_RT));
    assert!(!solicit.requested().contains(&opt::INF_MAX_RT));

    // Two offers within the first RT: the preferred one is taken when it
    // is over.
    let first_rt_end = client.poll_at().unwrap();
    let offer = |server, preference: u8| {
        answer(msg::ADVERTISE, &solicit, server, |w| {
            w.option(opt::PREFERENCE, |w| w.bytes(&[preference]))?;
            ia_na(w, 0, 0, LEASED, 200, 300)
        })
    };
    client.receive(now.plus_millis(10), &offer(OTHER_SERVER, 1));
    client.receive(now.plus_millis(20), &offer(SERVER, 5));
    client.receive(now.plus_millis(30), &offer(OTHER_SERVER, 2));
    assert_eq!(client.state(), State::Soliciting);
    nothing_to_send(&mut client, now.plus_millis(40));

    let request = send(&mut client, first_rt_end);
    assert_eq!(request.kind(), msg::REQUEST);
    assert_eq!(request.option(opt::SERVER_ID), Some(SERVER));
    assert_eq!(request.ia_addresses(), [LEASED]);
    assert_eq!(request.elapsed(), 0);

    let reply = answer(msg::REPLY, &request, SERVER, |w| {
        ia_na(w, 100, 160, LEASED, 200, 300)?;
        dns(w)
    });
    let now = first_rt_end.plus_millis(50);
    client.receive(now, &reply);
    assert_eq!(client.state(), State::Bound);
    assert!(client.configuration_changed());
    let config = client.configuration().unwrap();
    assert_eq!(config.addresses.len(), 1);
    let address = config.addresses[0];
    assert_eq!(address.address, LEASED);
    assert_eq!(address.preferred_until, now.plus_millis(200_000));
    assert_eq!(address.valid_until, now.plus_millis(300_000));
    assert_eq!(config.dns_servers, [DNS]);
    assert_eq!(config.renew_at, now.plus_millis(100_000));
    assert_eq!(config.rebind_at, now.plus_millis(160_000));

    // Renew at T1, with the server.
    assert_eq!(client.poll_at(), Some(now.plus_millis(100_000)));
    let (renew_time, renew) = send_when_due(&mut client);
    assert_eq!(client.state(), State::Renewing);
    assert_eq!(renew.kind(), msg::RENEW);
    assert_eq!(renew.option(opt::SERVER_ID), Some(SERVER));
    assert_eq!(renew.ia_addresses(), [LEASED]);

    // Extended: no change but the times.
    let reply = answer(msg::REPLY, &renew, SERVER, |w| {
        ia_na(w, 100, 160, LEASED, 200, 300)?;
        dns(w)
    });
    client.receive(renew_time.plus_millis(10), &reply);
    assert_eq!(client.state(), State::Bound);
    assert!(!client.configuration_changed());
    let config = client.configuration().unwrap();
    assert_eq!(config.renew_at, renew_time.plus_millis(100_010));
    assert_eq!(
        config.addresses[0].valid_until,
        renew_time.plus_millis(300_010)
    );
}

#[test]
fn renew_then_rebind_then_lose_the_lease() {
    let (mut client, bound_at) = bound();
    let (t1, renew) = send_when_due(&mut client);
    assert_eq!(t1, bound_at.plus_millis(100_000));
    assert_eq!(renew.kind(), msg::RENEW);

    // Renew is retransmitted until T2, about every 10, 20, 40 s.
    loop {
        let (now, sent) = send_when_due(&mut client);
        if sent.kind() != msg::RENEW {
            assert_eq!(now, bound_at.plus_millis(160_000));
            assert_eq!(sent.kind(), msg::REBIND);
            assert_eq!(sent.option(opt::SERVER_ID), None);
            assert_eq!(sent.ia_addresses(), [LEASED]);
            assert_eq!(sent.elapsed(), 0);
            break;
        }
        assert_eq!(sent.xid(), renew.xid());
        assert!(sent.elapsed() > 0);
    }
    assert_eq!(client.state(), State::Rebinding);

    // Rebinding until the valid lifetime ends, then the lease is gone.
    let valid_end = bound_at.plus_millis(300_000);
    let mut now;
    loop {
        now = client.poll_at().unwrap();
        assert!(now <= valid_end);
        let mut buf = [0u8; 1024];
        let sent = client.poll(now, &mut buf).unwrap();
        if client.state() != State::Rebinding {
            break;
        }
        assert!(sent.is_some());
    }
    assert_eq!(now, valid_end);
    assert_eq!(client.state(), State::Soliciting);
    assert!(client.configuration().is_none());
    assert!(client.configuration_changed());
}

#[test]
fn rebind_reply_from_another_server() {
    let (mut client, bound_at) = bound();
    client
        .poll(bound_at.plus_millis(160_000), &mut [0u8; 1024])
        .unwrap();
    assert_eq!(client.state(), State::Rebinding);
    let (now, rebind) = send_when_due(&mut client);
    let reply = answer(msg::REPLY, &rebind, OTHER_SERVER, |w| {
        ia_na(w, 100, 160, LEASED, 200, 300)
    });
    client.receive(now, &reply);
    assert_eq!(client.state(), State::Bound);
    // The server changed, and the DNS servers are gone.
    assert!(client.configuration_changed());
    let config = client.configuration().unwrap();
    assert_eq!(config.server_id.as_bytes(), OTHER_SERVER);
    assert!(config.dns_servers.is_empty());
}

#[test]
fn highest_preference_is_taken_at_once() {
    let (client, _) = bound();
    assert_eq!(client.configuration().unwrap().addresses[0].address, LEASED);
}

#[test]
fn first_offer_after_the_first_rt_is_taken_at_once() {
    let mut client = client();
    client.set_mode(Mode::Stateful, at(0));
    let (_, first) = send_when_due(&mut client);
    let (now, second) = send_when_due(&mut client);
    assert_eq!(second.kind(), msg::SOLICIT);
    assert_eq!(second.xid(), first.xid());
    let offer = answer(msg::ADVERTISE, &second, SERVER, |w| {
        ia_na(w, 0, 0, LEASED, 200, 300)
    });
    client.receive(now.plus_millis(5), &offer);
    assert_eq!(client.state(), State::Requesting);
    let request = send(&mut client, now.plus_millis(5));
    assert_eq!(request.kind(), msg::REQUEST);
}

#[test]
fn offers_without_addresses_are_ignored() {
    let mut client = client();
    client.set_mode(Mode::Stateful, at(0));
    let (now, solicit) = send_when_due(&mut client);
    let offers = [
        answer(msg::ADVERTISE, &solicit, SERVER, |w| {
            w.option(opt::STATUS_CODE, |w| w.u16(status::NO_ADDRS_AVAIL))
        }),
        answer(msg::ADVERTISE, &solicit, SERVER, |w| {
            ia_na_status(w, status::NO_ADDRS_AVAIL)
        }),
        // Valid lifetime 0, preferred beyond valid.
        answer(msg::ADVERTISE, &solicit, SERVER, |w| {
            ia_na(w, 0, 0, LEASED, 0, 0)
        }),
        answer(msg::ADVERTISE, &solicit, SERVER, |w| {
            ia_na(w, 0, 0, LEASED, 400, 300)
        }),
        // T1 beyond T2.
        answer(msg::ADVERTISE, &solicit, SERVER, |w| {
            ia_na(w, 200, 100, LEASED, 400, 500)
        }),
        // Another IAID.
        answer(msg::ADVERTISE, &solicit, SERVER, |w| {
            w.option(opt::IA_NA, |w| {
                w.u32(IAID + 1)?;
                w.u32(0)?;
                w.u32(0)?;
                w.option(opt::IA_ADDR, |w| {
                    w.bytes(&LEASED.octets())?;
                    w.u32(200)?;
                    w.u32(300)
                })
            })
        }),
    ];
    for offer in offers {
        client.receive(now, &offer);
    }
    let (_, sent) = send_when_due(&mut client);
    assert_eq!(sent.kind(), msg::SOLICIT);
}

#[test]
fn solicit_retransmissions() {
    let mut client = client();
    client.set_mode(Mode::Stateful, at(0));
    let (first, _) = send_when_due(&mut client);
    let mut previous_rt = 0;
    let mut previous = first;
    for i in 0..20 {
        let (now, sent) = send_when_due(&mut client);
        let rt = now.as_millis() - previous.as_millis();
        if i == 0 {
            // The first RT: IRT plus a positive RAND.
            assert!((1001..=1100).contains(&rt), "first RT {rt}");
        } else if rt < 3_240_000 {
            assert!(rt >= previous_rt * 19 / 10 && rt <= previous_rt * 21 / 10 + 1);
        } else {
            // Around SOL_MAX_RT.
            assert!((3_240_000..=3_960_000).contains(&rt), "RT {rt}");
        }
        assert_eq!(
            u64::from(sent.elapsed()),
            ((now.as_millis() - first.as_millis()) / 10).min(0xffff)
        );
        previous_rt = rt;
        previous = now;
    }
}

#[test]
fn sol_max_rt_from_a_server() {
    let mut client = client();
    client.set_mode(Mode::Stateful, at(0));
    let (_, solicit) = send_when_due(&mut client);
    // An Advertise without addresses still sets SOL_MAX_RT.
    let advertise = answer(msg::ADVERTISE, &solicit, SERVER, |w| {
        w.option(opt::SOL_MAX_RT, |w| w.u32(60))?;
        w.option(opt::STATUS_CODE, |w| w.u16(status::NO_ADDRS_AVAIL))
    });
    client.receive(at(1000), &advertise);
    let mut previous = client.poll_at().unwrap();
    send(&mut client, previous);
    for _ in 0..10 {
        let (now, _) = send_when_due(&mut client);
        assert!(now.as_millis() - previous.as_millis() <= 66_000);
        previous = now;
    }
}

#[test]
fn request_gives_up_after_ten_tries() {
    let mut client = client();
    client.set_mode(Mode::Stateful, at(0));
    let (now, solicit) = send_when_due(&mut client);
    let offer = answer(msg::ADVERTISE, &solicit, SERVER, |w| {
        w.option(opt::PREFERENCE, |w| w.bytes(&[255]))?;
        ia_na(w, 0, 0, LEASED, 200, 300)
    });
    client.receive(now, &offer);
    for _ in 0..10 {
        let (_, sent) = send_when_due(&mut client);
        assert_eq!(sent.kind(), msg::REQUEST);
    }
    // After the tenth RT: soliciting again.
    let (_, sent) = send_when_due(&mut client);
    assert_eq!(sent.kind(), msg::SOLICIT);
}

#[test]
fn request_refused() {
    for refusal in [status::NO_ADDRS_AVAIL, status::NOT_ON_LINK] {
        let mut client = client();
        client.set_mode(Mode::Stateful, at(0));
        let (now, solicit) = send_when_due(&mut client);
        client.receive(
            now,
            &answer(msg::ADVERTISE, &solicit, SERVER, |w| {
                w.option(opt::PREFERENCE, |w| w.bytes(&[255]))?;
                ia_na(w, 0, 0, LEASED, 200, 300)
            }),
        );
        let request = send(&mut client, now);
        client.receive(
            now,
            &answer(msg::REPLY, &request, SERVER, |w| ia_na_status(w, refusal)),
        );
        assert_eq!(client.state(), State::Soliciting);
        assert!(!client.configuration_changed());
    }
}

#[test]
fn renew_without_binding_requests_again() {
    let (mut client, _) = bound();
    let (now, renew) = send_when_due(&mut client);
    client.receive(
        now,
        &answer(msg::REPLY, &renew, SERVER, |w| {
            ia_na_status(w, status::NO_BINDING)
        }),
    );
    assert_eq!(client.state(), State::Requesting);
    // The address stays meanwhile.
    assert!(client.configuration().is_some());
    let request = send(&mut client, now);
    assert_eq!(request.kind(), msg::REQUEST);
    assert_eq!(request.option(opt::SERVER_ID), Some(SERVER));
    assert_eq!(request.ia_addresses(), [LEASED]);
    client.receive(
        now,
        &answer(msg::REPLY, &request, SERVER, |w| {
            ia_na(w, 100, 160, LEASED, 200, 300)?;
            dns(w)
        }),
    );
    assert_eq!(client.state(), State::Bound);
    assert!(!client.configuration_changed());
}

#[test]
fn lease_lost_while_soliciting_is_dropped_when_it_expires() {
    let (mut client, bound_at) = bound();
    let (now, renew) = send_when_due(&mut client);
    client.receive(
        now,
        &answer(msg::REPLY, &renew, SERVER, |w| {
            ia_na_status(w, status::NO_BINDING)
        }),
    );
    // The Request goes unanswered: soliciting, with the old address still
    // valid.
    for _ in 0..11 {
        send_when_due(&mut client);
    }
    assert_eq!(client.state(), State::Soliciting);
    assert!(client.configuration().is_some());
    client
        .poll(bound_at.plus_millis(300_000), &mut [0u8; 1024])
        .unwrap();
    assert!(client.configuration().is_none());
    assert!(client.configuration_changed());
    assert_eq!(client.state(), State::Soliciting);
}

#[test]
fn renew_reply_moves_to_a_new_address() {
    let (mut client, _) = bound();
    let (now, renew) = send_when_due(&mut client);
    let new = Ipv6Addr::new(0xfd77, 0x77, 0x77, 0, 0, 0, 0, 0x101);
    let reply = answer(msg::REPLY, &renew, SERVER, |w| {
        w.option(opt::IA_NA, |w| {
            w.u32(IAID)?;
            w.u32(100)?;
            w.u32(160)?;
            // The old address ends, a new one comes.
            for (address, lifetime) in [(LEASED, 0), (new, 300)] {
                w.option(opt::IA_ADDR, |w| {
                    w.bytes(&address.octets())?;
                    w.u32(lifetime)?;
                    w.u32(lifetime)
                })?;
            }
            Ok(())
        })?;
        dns(w)
    });
    client.receive(now, &reply);
    assert!(client.configuration_changed());
    let addresses: Vec<_> = client
        .configuration()
        .unwrap()
        .addresses
        .iter()
        .map(|a| a.address)
        .collect();
    assert_eq!(addresses, [new]);
}

#[test]
fn t1_and_t2_of_zero_follow_the_preferred_lifetime() {
    let mut client = client();
    client.set_mode(Mode::Stateful, at(0));
    let (now, solicit) = send_when_due(&mut client);
    client.receive(
        now,
        &answer(msg::ADVERTISE, &solicit, SERVER, |w| {
            w.option(opt::PREFERENCE, |w| w.bytes(&[255]))?;
            ia_na(w, 0, 0, LEASED, 1000, 2000)
        }),
    );
    let request = send(&mut client, now);
    client.receive(
        now,
        &answer(msg::REPLY, &request, SERVER, |w| {
            ia_na(w, 0, 0, LEASED, 1000, 2000)
        }),
    );
    let config = client.configuration().unwrap();
    assert_eq!(config.renew_at, now.plus_millis(500_000));
    assert_eq!(config.rebind_at, now.plus_millis(800_000));
}

#[test]
fn infinite_lifetimes_never_renew() {
    let mut client = client();
    client.set_mode(Mode::Stateful, at(0));
    let (now, solicit) = send_when_due(&mut client);
    client.receive(
        now,
        &answer(msg::ADVERTISE, &solicit, SERVER, |w| {
            w.option(opt::PREFERENCE, |w| w.bytes(&[255]))?;
            ia_na(w, 0, 0, LEASED, u32::MAX, u32::MAX)
        }),
    );
    let request = send(&mut client, now);
    client.receive(
        now,
        &answer(msg::REPLY, &request, SERVER, |w| {
            ia_na(w, 0, 0, LEASED, u32::MAX, u32::MAX)
        }),
    );
    assert_eq!(client.state(), State::Bound);
    assert_eq!(client.poll_at(), None);
    assert_eq!(
        client.configuration().unwrap().addresses[0].valid_until,
        Instant::NEVER
    );
}

#[test]
fn release() {
    let (mut client, bound_at) = bound();
    client.release(bound_at);
    assert_eq!(client.mode(), Mode::Off);
    assert_eq!(client.state(), State::Releasing);
    assert!(client.configuration().is_none());
    assert!(client.configuration_changed());
    let release = send(&mut client, bound_at);
    assert_eq!(release.kind(), msg::RELEASE);
    assert_eq!(release.option(opt::SERVER_ID), Some(SERVER));
    assert_eq!(release.ia_addresses(), [LEASED]);
    assert_eq!(release.option(opt::ORO), None);
    client.receive(
        bound_at,
        &answer(msg::REPLY, &release, SERVER, |w| {
            w.option(opt::STATUS_CODE, |w| w.u16(status::SUCCESS))
        }),
    );
    assert_eq!(client.state(), State::Idle);
    assert_eq!(client.poll_at(), None);
}

#[test]
fn release_gives_up_after_four_tries() {
    let (mut client, bound_at) = bound();
    client.release(bound_at);
    for _ in 0..4 {
        let (_, sent) = send_when_due(&mut client);
        assert_eq!(sent.kind(), msg::RELEASE);
    }
    let now = client.poll_at().unwrap();
    nothing_to_send(&mut client, now);
    assert_eq!(client.state(), State::Idle);
}

#[test]
fn release_without_a_lease_just_stops() {
    let mut client = client();
    client.set_mode(Mode::Stateless, at(0));
    client.release(at(0));
    assert_eq!(client.state(), State::Idle);
    assert_eq!(client.poll_at(), None);
}

#[test]
fn mode_change_starts_over() {
    let (mut client, bound_at) = bound();
    client.set_mode(Mode::Stateful, bound_at);
    assert_eq!(client.state(), State::Bound);
    client.set_mode(Mode::Stateless, bound_at);
    assert_eq!(client.state(), State::InformationRequesting);
    assert!(client.configuration().is_none());
    assert!(client.configuration_changed());
    client.set_mode(Mode::Off, bound_at);
    assert_eq!(client.state(), State::Idle);
    assert_eq!(client.poll_at(), None);
}

#[test]
fn reconnecting_rebinds() {
    let (mut client, bound_at) = bound();
    client.link_reconnected(bound_at.plus_millis(1000));
    assert_eq!(client.state(), State::Rebinding);
    let rebind = send(&mut client, bound_at.plus_millis(1000));
    assert_eq!(rebind.kind(), msg::REBIND);
    assert!(client.configuration().is_some());
}

#[test]
fn mode_from_router_flags() {
    assert_eq!(Mode::from_router_flags(false, false), Mode::Off);
    assert_eq!(Mode::from_router_flags(false, true), Mode::Stateless);
    assert_eq!(Mode::from_router_flags(true, false), Mode::Stateful);
    assert_eq!(Mode::from_router_flags(true, true), Mode::Stateful);
}

#[test]
fn small_buffer() {
    let mut client = client();
    client.set_mode(Mode::Stateful, at(0));
    let now = client.poll_at().unwrap();
    assert_eq!(client.poll(now, &mut [0u8; 20]), Err(crate::BufferTooSmall));
    // Nothing was used up: the message is still due.
    assert_eq!(client.poll_at(), Some(now));
    send(&mut client, now);
}
