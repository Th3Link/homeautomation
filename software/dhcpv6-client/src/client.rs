//! The client's state machine (RFC 8415 § 15, § 18).

use crate::config::{Address, Configuration};
use crate::rng::Rng;
use crate::time::Instant;
use crate::wire::{self, msg, opt, status, BufferTooSmall, IaNa, Message, Writer};
use crate::Duid;
use core::net::Ipv6Addr;
use heapless::Vec;

/// What the client does, from the routers' flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Nothing: no DHCPv6 on the link.
    #[default]
    Off,
    /// Other configuration only (Information-request).
    Stateless,
    /// An address and the other configuration (IA_NA).
    Stateful,
}

impl Mode {
    /// The mode the Managed and Other configuration flags of the Router
    /// Advertisements ask for (RFC 4861 § 4.2): `M` means addresses (and
    /// everything else), `O` only everything else.
    pub fn from_router_flags(managed: bool, other: bool) -> Self {
        match (managed, other) {
            (true, _) => Self::Stateful,
            (false, true) => Self::Stateless,
            (false, false) => Self::Off,
        }
    }
}

/// How the client identifies itself, and its randomness.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// The client's DUID, typically [`Duid::link_layer`].
    pub duid: Duid,
    /// The identity of its IA_NA: any number, the same across restarts.
    pub iaid: u32,
    /// Seeds transaction IDs and retransmission jitter: should differ
    /// between devices and boots (a hardware random number).
    pub seed: u64,
}

/// Where the client is, for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// [`Mode::Off`], or after a release.
    Idle,
    /// Looking for servers (Solicit).
    Soliciting,
    /// Asking the chosen server for the address (Request).
    Requesting,
    /// Has an address; renews at T1.
    Bound,
    /// Extending the lease with its server (Renew).
    Renewing,
    /// Extending the lease with any server (Rebind).
    Rebinding,
    /// Asking for the other configuration (Information-request).
    InformationRequesting,
    /// Has the other configuration; asks again after the refresh time.
    Informed,
    /// Giving the address back (Release).
    Releasing,
}

// Transmission and retransmission parameters (§ 7.6), in milliseconds.
const SOL_MAX_DELAY: u32 = 1_000;
const SOL_TIMEOUT: u64 = 1_000;
const SOL_MAX_RT: u64 = 3_600_000;
const REQ_TIMEOUT: u64 = 1_000;
const REQ_MAX_RT: u64 = 30_000;
const REQ_MAX_RC: u32 = 10;
const REN_TIMEOUT: u64 = 10_000;
const REN_MAX_RT: u64 = 600_000;
const REB_TIMEOUT: u64 = 10_000;
const REB_MAX_RT: u64 = 600_000;
const INF_MAX_DELAY: u32 = 1_000;
const INF_TIMEOUT: u64 = 1_000;
const INF_MAX_RT: u64 = 3_600_000;
const REL_TIMEOUT: u64 = 1_000;
const REL_MAX_RC: u32 = 4;
// The Information Refresh Time without the option, and its lower bound
// (§ 21.23), in seconds.
const IRT_DEFAULT: u32 = 86_400;
const IRT_MINIMUM: u32 = 600;
/// The valid range of the SOL_MAX_RT and INF_MAX_RT options, in seconds
/// (§ 21.24, § 21.25).
const MAX_RT_RANGE: core::ops::RangeInclusive<u32> = 60..=86_400;
/// The shortest T1 the client picks itself (when the server leaves T1 and
/// T2 to it), so that a short preferred lifetime doesn't make it renew in
/// a tight loop.
const MIN_OWN_T1: u32 = 60;

/// The options the client asks for in every message but Release.
const REQUESTED: [u16; 4] = [
    opt::DNS_SERVERS,
    opt::DOMAIN_LIST,
    opt::NTP_SERVER,
    opt::SNTP_SERVERS,
];

/// A server's offer while soliciting.
#[derive(Debug, Clone)]
struct Offer<const A: usize> {
    server_id: Duid,
    preference: u8,
    addresses: Vec<Ipv6Addr, A>,
}

#[derive(Debug, Clone)]
enum Step<const A: usize> {
    Idle,
    Soliciting {
        offer: Option<Offer<A>>,
    },
    Requesting {
        server_id: Duid,
        addresses: Vec<Ipv6Addr, A>,
    },
    Bound,
    Renewing,
    Rebinding,
    InformationRequesting,
    Informed,
    Releasing {
        server_id: Duid,
        addresses: Vec<Ipv6Addr, A>,
    },
}

/// One message exchange and its retransmissions (§ 15).
#[derive(Debug, Clone, Copy)]
struct Exchange {
    xid: u32,
    /// When the first message went out.
    started: Option<Instant>,
    /// When the next one is due.
    next: Instant,
    /// The current retransmission timeout (RT), 0 before the first message.
    rt: u64,
    /// Messages sent.
    sent: u32,
    /// The end of the maximum retransmission duration (MRD).
    end: Instant,
}

/// A DHCPv6 client for one interface. `ADDRESSES`, `SERVERS` and
/// `NAME_BYTES` size its [`Configuration`].
///
/// Without explicit parameters (`let client: Client = ...`) it holds two
/// addresses, three servers of each kind and 128 bytes of names per list,
/// about 700 bytes in all.
#[derive(Debug, Clone)]
pub struct Client<
    const ADDRESSES: usize = 2,
    const SERVERS: usize = 3,
    const NAME_BYTES: usize = 128,
> {
    duid: Duid,
    iaid: u32,
    rng: Rng,
    mode: Mode,
    step: Step<ADDRESSES>,
    exchange: Exchange,
    config: Option<Configuration<ADDRESSES, SERVERS, NAME_BYTES>>,
    changed: bool,
    sol_max_rt: u64,
    inf_max_rt: u64,
}

impl<const ADDRESSES: usize, const SERVERS: usize, const NAME_BYTES: usize>
    Client<ADDRESSES, SERVERS, NAME_BYTES>
{
    /// A client in [`Mode::Off`].
    pub fn new(config: ClientConfig) -> Self {
        Self {
            duid: config.duid,
            iaid: config.iaid,
            rng: Rng::new(config.seed),
            mode: Mode::Off,
            step: Step::Idle,
            exchange: Exchange {
                xid: 0,
                started: None,
                next: Instant::NEVER,
                rt: 0,
                sent: 0,
                end: Instant::NEVER,
            },
            config: None,
            changed: false,
            sol_max_rt: SOL_MAX_RT,
            inf_max_rt: INF_MAX_RT,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn state(&self) -> State {
        match self.step {
            Step::Idle => State::Idle,
            Step::Soliciting { .. } => State::Soliciting,
            Step::Requesting { .. } => State::Requesting,
            Step::Bound => State::Bound,
            Step::Renewing => State::Renewing,
            Step::Rebinding => State::Rebinding,
            Step::InformationRequesting => State::InformationRequesting,
            Step::Informed => State::Informed,
            Step::Releasing { .. } => State::Releasing,
        }
    }

    /// What the client has obtained, if anything.
    pub fn configuration(&self) -> Option<&Configuration<ADDRESSES, SERVERS, NAME_BYTES>> {
        self.config.as_ref()
    }

    /// Has [`Self::configuration`] changed (beyond its lifetimes) since the
    /// last call?
    pub fn configuration_changed(&mut self) -> bool {
        core::mem::take(&mut self.changed)
    }

    /// Switches to `mode`. A change drops what the client had and starts
    /// over in the new mode (without a Release: the server lets the lease
    /// run out).
    pub fn set_mode(&mut self, mode: Mode, now: Instant) {
        if mode == self.mode {
            return;
        }
        self.mode = mode;
        self.drop_config();
        match mode {
            Mode::Off => self.step = Step::Idle,
            Mode::Stateless => self.start_information_request(now),
            Mode::Stateful => self.start_solicit(now),
        }
    }

    /// Gives the leased addresses back (Release) and stops: the mode is
    /// [`Mode::Off`] afterwards. The addresses must not be used from now
    /// on (§ 18.2.7); the Release goes out with the next [`Self::poll`].
    pub fn release(&mut self, now: Instant) {
        self.mode = Mode::Off;
        let addresses: Vec<Ipv6Addr, ADDRESSES> = self
            .config
            .iter()
            .flat_map(|config| config.addresses.iter().map(|a| a.address))
            .collect();
        let server_id = self.config.as_ref().map(|c| c.server_id.clone());
        self.drop_config();
        match server_id {
            Some(server_id) if !addresses.is_empty() => {
                self.step = Step::Releasing {
                    server_id,
                    addresses,
                };
                self.start_exchange(now, 0, Instant::NEVER);
            }
            _ => self.step = Step::Idle,
        }
    }

    /// The link went down and came up again (perhaps on another network):
    /// checks what the client has with the servers on the link. A stateful
    /// lease is rebound (any server answers, with the lifetimes or that the
    /// addresses are not valid there); stateless configuration is asked for
    /// again.
    pub fn link_reconnected(&mut self, now: Instant) {
        match self.step {
            Step::Bound | Step::Renewing | Step::Rebinding => self.start_rebind(now),
            Step::Informed | Step::InformationRequesting => self.start_information_request(now),
            Step::Soliciting { .. } | Step::Requesting { .. } => self.start_solicit(now),
            Step::Idle | Step::Releasing { .. } => {}
        }
    }

    /// When [`Self::poll`] has something to do next; `None` if nothing is
    /// pending.
    pub fn poll_at(&self) -> Option<Instant> {
        let expiry = self
            .config
            .iter()
            .flat_map(|config| config.addresses.iter().map(|a| a.valid_until))
            .min()
            .unwrap_or(Instant::NEVER);
        let at = match (&self.step, &self.config) {
            (Step::Idle, _) => return None,
            (Step::Bound, Some(config)) => config.renew_at.min(config.rebind_at),
            (Step::Informed, Some(config)) => config.renew_at,
            (Step::Bound | Step::Informed, None) => Instant::NEVER,
            _ => self.exchange.next,
        };
        let at = at.min(expiry);
        (at != Instant::NEVER).then_some(at)
    }

    /// Runs the timers and writes the message due now, if there is one, into
    /// `buf`: it goes to [`wire::ALL_DHCP_RELAY_AGENTS_AND_SERVERS`] port
    /// [`wire::SERVER_PORT`]. Returns its length. 1 KiB is plenty for a
    /// buffer.
    pub fn poll(&mut self, now: Instant, buf: &mut [u8]) -> Result<Option<usize>, BufferTooSmall> {
        self.run_timers(now);
        // A failed exchange starts the next one, which may be due at once.
        for _ in 0..3 {
            if !self.in_exchange() || now < self.exchange.next {
                return Ok(None);
            }
            // The first RT of a Solicit is over: the best offer wins.
            if let Step::Soliciting { offer: Some(_) } = &self.step {
                if let Step::Soliciting { offer: Some(offer) } =
                    core::mem::replace(&mut self.step, Step::Idle)
                {
                    self.start_request(now, offer.server_id, offer.addresses);
                }
                continue;
            }
            let (_, _, mrc) = self.parameters();
            if (mrc != 0 && self.exchange.sent >= mrc) || now >= self.exchange.end {
                self.exchange_failed(now);
                continue;
            }
            let len = self.write(now, buf)?;
            self.sent(now);
            return Ok(Some(len));
        }
        Ok(None)
    }

    /// Takes a datagram that arrived on [`wire::CLIENT_PORT`].
    pub fn receive(&mut self, now: Instant, data: &[u8]) {
        self.run_timers(now);
        let Some(message) = Message::parse(data) else {
            return;
        };
        if !self.in_exchange() || self.exchange.sent == 0 || message.xid != self.exchange.xid {
            return;
        }
        // Only answers to this client, from an identified server (§ 16).
        if message.option(opt::CLIENT_ID) != Some(self.duid.as_bytes()) {
            return;
        }
        let Some(server_id) = message.option(opt::SERVER_ID).and_then(Duid::from_bytes) else {
            return;
        };
        self.take_max_rt(&message);
        match (&self.step, message.kind) {
            (Step::Soliciting { .. }, msg::ADVERTISE) => self.advertised(now, &message, server_id),
            (Step::Requesting { .. } | Step::Renewing | Step::Rebinding, msg::REPLY) => {
                self.replied(now, &message, server_id)
            }
            (Step::InformationRequesting, msg::REPLY) => self.informed(now, &message, server_id),
            (Step::Releasing { .. }, msg::REPLY) => self.step = Step::Idle,
            _ => {}
        }
    }

    fn in_exchange(&self) -> bool {
        !matches!(self.step, Step::Idle | Step::Bound | Step::Informed)
    }

    /// IRT, MRT and MRC of the current exchange.
    fn parameters(&self) -> (u64, u64, u32) {
        match self.step {
            Step::Soliciting { .. } => (SOL_TIMEOUT, self.sol_max_rt, 0),
            Step::Requesting { .. } => (REQ_TIMEOUT, REQ_MAX_RT, REQ_MAX_RC),
            Step::Renewing => (REN_TIMEOUT, REN_MAX_RT, 0),
            Step::Rebinding => (REB_TIMEOUT, REB_MAX_RT, 0),
            Step::InformationRequesting => (INF_TIMEOUT, self.inf_max_rt, 0),
            Step::Releasing { .. } => (REL_TIMEOUT, 0, REL_MAX_RC),
            Step::Idle | Step::Bound | Step::Informed => (0, 0, 0),
        }
    }

    /// Expired addresses go, T1 and T2 start a Renew and a Rebind, the
    /// refresh time a new Information-request.
    fn run_timers(&mut self, now: Instant) {
        let Some(config) = &mut self.config else {
            return;
        };
        let expired = config.expire(now);
        let (empty, renew_at, rebind_at) = (
            config.addresses.is_empty(),
            config.renew_at,
            config.rebind_at,
        );
        let leased = matches!(self.step, Step::Bound | Step::Renewing | Step::Rebinding);
        if expired {
            self.changed = true;
            // Nothing left: stateless configuration has no addresses to
            // expire, so this was a lease.
            if empty {
                self.drop_config();
                if leased {
                    self.start_solicit(now);
                }
                return;
            }
        }
        match self.step {
            Step::Bound if now >= rebind_at => self.start_rebind(now),
            Step::Bound if now >= renew_at => {
                self.step = Step::Renewing;
                self.start_exchange(now, 0, rebind_at);
            }
            Step::Informed if now >= renew_at => self.start_information_request(now),
            _ => {}
        }
    }

    /// The exchange ran out of retransmissions or time.
    fn exchange_failed(&mut self, now: Instant) {
        match self.step {
            Step::Requesting { .. } => self.start_solicit(now),
            Step::Renewing => self.start_rebind(now),
            Step::Rebinding => {
                self.drop_config();
                self.start_solicit(now);
            }
            Step::Releasing { .. } => self.step = Step::Idle,
            // Solicit and Information-request go on for ever.
            _ => {}
        }
    }

    fn start_exchange(&mut self, now: Instant, max_delay: u32, end: Instant) {
        let delay = if max_delay == 0 {
            0
        } else {
            self.rng.below(max_delay)
        };
        self.exchange = Exchange {
            xid: self.rng.next_u32() & 0x00FF_FFFF,
            started: None,
            next: now.plus_millis(u64::from(delay)),
            rt: 0,
            sent: 0,
            end,
        };
    }

    fn start_solicit(&mut self, now: Instant) {
        self.step = Step::Soliciting { offer: None };
        self.start_exchange(now, SOL_MAX_DELAY, Instant::NEVER);
    }

    fn start_request(
        &mut self,
        now: Instant,
        server_id: Duid,
        addresses: Vec<Ipv6Addr, ADDRESSES>,
    ) {
        self.step = Step::Requesting {
            server_id,
            addresses,
        };
        self.start_exchange(now, 0, Instant::NEVER);
    }

    /// Rebind until the last address expires.
    fn start_rebind(&mut self, now: Instant) {
        let end = self
            .config
            .as_ref()
            .and_then(|config| config.valid_until())
            .unwrap_or(now);
        self.step = Step::Rebinding;
        self.start_exchange(now, 0, end);
    }

    fn start_information_request(&mut self, now: Instant) {
        self.step = Step::InformationRequesting;
        self.start_exchange(now, INF_MAX_DELAY, Instant::NEVER);
    }

    fn drop_config(&mut self) {
        if self.config.take().is_some() {
            self.changed = true;
        }
    }

    /// A message went out: the next retransmission timeout (§ 15).
    fn sent(&mut self, now: Instant) {
        let (irt, mrt, _) = self.parameters();
        let exchange = &mut self.exchange;
        if exchange.started.is_none() {
            exchange.started = Some(now);
        }
        let mut rt = if exchange.rt == 0 {
            // The first RT of a Solicit is longer than IRT, so that the
            // first Advertise can be waited for in full (§ 18.2.1).
            let jitter = if matches!(self.step, Step::Soliciting { .. }) {
                jitter_positive(&mut self.rng, irt)
            } else {
                jitter(&mut self.rng, irt)
            };
            irt.saturating_add_signed(jitter)
        } else {
            (2 * exchange.rt).saturating_add_signed(jitter(&mut self.rng, exchange.rt))
        };
        if mrt != 0 && rt > mrt {
            rt = mrt.saturating_add_signed(jitter(&mut self.rng, mrt));
        }
        exchange.rt = rt;
        exchange.sent += 1;
        exchange.next = now.plus_millis(rt).min(exchange.end);
    }

    /// Takes SOL_MAX_RT and INF_MAX_RT from a server (§ 18.2.9, § 18.2.10).
    fn take_max_rt(&mut self, message: &Message<'_>) {
        let valid = |code| {
            message
                .option(code)
                .and_then(wire::u32_option)
                .filter(|secs| MAX_RT_RANGE.contains(secs))
                .map(|secs| u64::from(secs) * 1000)
        };
        if let Some(rt) = valid(opt::SOL_MAX_RT) {
            self.sol_max_rt = rt;
        }
        if let Some(rt) = valid(opt::INF_MAX_RT) {
            self.inf_max_rt = rt;
        }
    }

    /// This client's IA_NA in `message`, if it is valid (§ 21.4: T1 must not
    /// be beyond T2).
    fn own_ia<'a>(&self, message: &Message<'a>) -> Option<IaNa<'a>> {
        message
            .options()
            .filter(|(code, _)| *code == opt::IA_NA)
            .filter_map(|(_, data)| IaNa::parse(data))
            .find(|ia| ia.iaid == self.iaid)
            .filter(|ia| ia.t1 == 0 || ia.t2 == 0 || ia.t1 <= ia.t2)
    }

    fn advertised(&mut self, now: Instant, message: &Message<'_>, server_id: Duid) {
        // An Advertise without addresses is ignored (§ 18.2.9).
        if message.status() != status::SUCCESS {
            return;
        }
        let Some(ia) = self.own_ia(message) else {
            return;
        };
        if ia.options.status() != status::SUCCESS {
            return;
        }
        let addresses: Vec<Ipv6Addr, ADDRESSES> = ia
            .addresses()
            .filter(|a| a.status == status::SUCCESS && a.valid != 0 && a.preferred <= a.valid)
            .map(|a| a.address)
            .take(ADDRESSES)
            .collect();
        if addresses.is_empty() {
            return;
        }
        let preference = message
            .option(opt::PREFERENCE)
            .and_then(|data| data.first().copied())
            .unwrap_or(0);
        let offer = Offer {
            server_id,
            preference,
            addresses,
        };
        let Step::Soliciting { offer: best } = &mut self.step else {
            return;
        };
        if best
            .as_ref()
            .is_none_or(|best| offer.preference > best.preference)
        {
            *best = Some(offer);
        }
        // The highest preference is taken at once, and after the first RT
        // the first offer (§ 18.2.1).
        if preference == u8::MAX || self.exchange.sent > 1 {
            if let Step::Soliciting { offer: Some(offer) } =
                core::mem::replace(&mut self.step, Step::Idle)
            {
                self.start_request(now, offer.server_id, offer.addresses);
            }
        }
    }

    /// A Reply to Request, Renew or Rebind (§ 18.2.10).
    fn replied(&mut self, now: Instant, message: &Message<'_>, server_id: Duid) {
        let requesting = matches!(self.step, Step::Requesting { .. });
        match message.status() {
            status::SUCCESS => {}
            status::NOT_ON_LINK if requesting => return self.start_solicit(now),
            // UnspecFail, UseMulticast, ...: the retransmissions go on.
            _ => return,
        }
        let Some(ia) = self.own_ia(message) else {
            return;
        };
        match ia.options.status() {
            status::SUCCESS => {}
            // The server lost the lease: ask it for the addresses again.
            status::NO_BINDING if !requesting => {
                let addresses = self
                    .config
                    .iter()
                    .flat_map(|config| config.addresses.iter().map(|a| a.address))
                    .collect();
                return self.start_request(now, server_id, addresses);
            }
            status::NOT_ON_LINK | status::NO_ADDRS_AVAIL | status::NO_BINDING if requesting => {
                return self.start_solicit(now);
            }
            status::NOT_ON_LINK => {
                self.drop_config();
                return self.start_solicit(now);
            }
            // The addresses stay until they expire.
            _ => return,
        }

        let previous = self.config.take();
        let mut config = match &previous {
            Some(config) if !requesting => config.clone(),
            _ => Configuration::new(server_id.clone()),
        };
        config.server_id = server_id;
        let mut shortest_preferred = u32::MAX;
        for offered in ia.addresses() {
            if offered.status != status::SUCCESS || offered.preferred > offered.valid {
                continue;
            }
            let known = config
                .addresses
                .iter()
                .position(|a| a.address == offered.address);
            if offered.valid == 0 {
                if let Some(i) = known {
                    config.addresses.swap_remove(i);
                }
                continue;
            }
            shortest_preferred = shortest_preferred.min(offered.preferred);
            let address = Address {
                address: offered.address,
                preferred_until: now.plus_lifetime(offered.preferred),
                valid_until: now.plus_lifetime(offered.valid),
            };
            match known {
                Some(i) => config.addresses[i] = address,
                None => {
                    let _ = config.addresses.push(address);
                }
            }
        }
        if config.addresses.is_empty() {
            self.changed |= previous.is_some();
            return self.start_solicit(now);
        }

        // T1 and T2 of 0 leave the times to the client (§ 21.4).
        let own = |fraction: fn(u32) -> u32| match shortest_preferred {
            u32::MAX => u32::MAX,
            preferred => fraction(preferred).max(MIN_OWN_T1),
        };
        let t1 = if ia.t1 == 0 { own(|p| p / 2) } else { ia.t1 };
        let t2 = if ia.t2 == 0 {
            own(|p| p / 5 * 4)
        } else {
            ia.t2
        };
        config.renew_at = now.plus_lifetime(t1.min(t2));
        config.rebind_at = now.plus_lifetime(t2);
        config.take_options(message);

        self.changed |= previous.is_none_or(|previous| !same_content(&previous, &config));
        self.config = Some(config);
        self.step = Step::Bound;
    }

    /// A Reply to Information-request (§ 18.2.6).
    fn informed(&mut self, now: Instant, message: &Message<'_>, server_id: Duid) {
        if message.status() != status::SUCCESS {
            return;
        }
        let mut config = Configuration::new(server_id);
        config.take_options(message);
        let refresh = message
            .option(opt::INFORMATION_REFRESH_TIME)
            .and_then(wire::u32_option)
            .unwrap_or(IRT_DEFAULT)
            .max(IRT_MINIMUM);
        config.renew_at = now.plus_lifetime(refresh);
        self.changed |= self
            .config
            .as_ref()
            .is_none_or(|before| !same_content(before, &config));
        self.config = Some(config);
        self.step = Step::Informed;
    }

    /// Writes the message of the current exchange.
    fn write(&self, now: Instant, buf: &mut [u8]) -> Result<usize, BufferTooSmall> {
        let mut w = Writer::new(buf);
        let config = self.config.as_ref();
        let config_addresses = || {
            config
                .into_iter()
                .flat_map(|c| c.addresses.iter().map(|a| a.address))
        };
        let (kind, server_id) = match &self.step {
            Step::Soliciting { .. } => (msg::SOLICIT, None),
            Step::Requesting { server_id, .. } => (msg::REQUEST, Some(server_id)),
            Step::Renewing => (msg::RENEW, config.map(|c| &c.server_id)),
            Step::Rebinding => (msg::REBIND, None),
            Step::InformationRequesting => (msg::INFORMATION_REQUEST, None),
            Step::Releasing { server_id, .. } => (msg::RELEASE, Some(server_id)),
            Step::Idle | Step::Bound | Step::Informed => return Ok(0),
        };
        w.header(kind, self.exchange.xid)?;
        w.option(opt::CLIENT_ID, |w| w.bytes(self.duid.as_bytes()))?;
        if let Some(server_id) = server_id {
            w.option(opt::SERVER_ID, |w| w.bytes(server_id.as_bytes()))?;
        }
        // In hundredths of a second since the first message (§ 21.9).
        let elapsed = self
            .exchange
            .started
            .map_or(0, |started| now.millis_since(started) / 10);
        w.option(opt::ELAPSED_TIME, |w| {
            w.u16(u16::try_from(elapsed).unwrap_or(u16::MAX))
        })?;

        // The IA_NA, with the addresses the client wants or has; the
        // lifetimes are the server's to set (§ 21.6).
        let ia_addresses = |w: &mut Writer<'_>, addresses: &mut dyn Iterator<Item = Ipv6Addr>| {
            w.option(opt::IA_NA, |w| {
                w.u32(self.iaid)?;
                w.u32(0)?;
                w.u32(0)?;
                for address in addresses {
                    w.option(opt::IA_ADDR, |w| {
                        w.bytes(&address.octets())?;
                        w.u32(0)?;
                        w.u32(0)
                    })?;
                }
                Ok(())
            })
        };
        match &self.step {
            Step::Soliciting { .. } => ia_addresses(&mut w, &mut core::iter::empty())?,
            Step::Requesting { addresses, .. } | Step::Releasing { addresses, .. } => {
                ia_addresses(&mut w, &mut addresses.iter().copied())?
            }
            Step::Renewing | Step::Rebinding => ia_addresses(&mut w, &mut config_addresses())?,
            _ => {}
        }

        if kind != msg::RELEASE {
            w.option(opt::ORO, |w| {
                for code in REQUESTED {
                    w.u16(code)?;
                }
                if kind == msg::INFORMATION_REQUEST {
                    w.u16(opt::INFORMATION_REFRESH_TIME)?;
                    w.u16(opt::INF_MAX_RT)
                } else {
                    w.u16(opt::SOL_MAX_RT)
                }
            })?;
        }
        Ok(w.len())
    }
}

/// Do `a` and `b` hold the same addresses, servers and names (ignoring
/// lifetimes and times)?
fn same_content<const A: usize, const S: usize, const N: usize>(
    a: &Configuration<A, S, N>,
    b: &Configuration<A, S, N>,
) -> bool {
    a.server_id == b.server_id
        && a.addresses.len() == b.addresses.len()
        && a.addresses
            .iter()
            .all(|x| b.addresses.iter().any(|y| y.address == x.address))
        && a.dns_servers == b.dns_servers
        && a.domains == b.domains
        && a.ntp_servers == b.ntp_servers
        && a.ntp_names == b.ntp_names
}

/// RAND × `value` with RAND in [-0.1, 0.1] (§ 15).
fn jitter(rng: &mut Rng, value: u64) -> i64 {
    let factor = i64::from(rng.below(201)) - 100;
    i64::try_from(value).unwrap_or(i64::MAX) / 1000 * factor
}

/// RAND × `value` with RAND in (0, 0.1].
fn jitter_positive(rng: &mut Rng, value: u64) -> i64 {
    let factor = i64::from(rng.below(100)) + 1;
    i64::try_from(value).unwrap_or(i64::MAX) / 1000 * factor
}
