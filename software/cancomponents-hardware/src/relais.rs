//! Drives the relay outputs (and, in rollershutter modes, relay *pairs*)
//! of up to five relay boards, two I2C GPIO expanders each, and runs the
//! timed on/off scheduling for `Relais`/`Rollershutter` CAN commands. The
//! message's `bank` picks the board, see [`cancomponents_core::relais_board`]:
//! 0 is the node's own board, 1-4 are boards on the extension bus.

use crate::can::send_can_message;
use crate::config::{self, config};
use crate::console_log;
use crate::error::{report_error, Component, ErrorCode, Severity};
use crate::extension::{Bus, Device};
use cancomponents_core::can_id::CanId;
use cancomponents_core::can_message_type::CanMessageType;
use cancomponents_core::relais::{Message, Mode, State};
use cancomponents_core::relais_board::{self, Kind, BANKS, REG_CONFIG, REG_OUTPUT};
use cancomponents_core::relais_manager::RelayManager;
use core::sync::atomic::{AtomicU8, Ordering};
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};
use embedded_hal_async::i2c::I2c as _;
use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::i2c::master::{Config, I2c};
use static_cell::StaticCell;

const MAX_RELAIS: usize = 16;
/// Schedule slots: every channel of every bank (a power of two).
const SLOTS: usize = 128;

/// Queue of pending `Relais`/`Rollershutter` commands, consumed by
/// `relais_task`. Also used by the commissioning CLI (`cli::cmd_relais`) to
/// drive relays during bring-up without going through the CAN bus.
static RELAIS_CHANNEL: Channel<CriticalSectionRawMutex, Message, MAX_RELAIS> = Channel::new();

pub async fn relais_handler(_id: CanId, data: &[u8], _remote_request: bool) {
    match Message::from_bytes(data) {
        Ok(msg) => {
            // On a heating controller these relays are driven by its control
            // logic alone (the rod, valves, mixer and pump).
            if msg.bank == 0
                && cancomponents_core::heating::is_controlled_relay(msg.num)
                && crate::heating::is_heating_controller().await
            {
                console_log!(
                    "relais {}: owned by the heating controller, ignored",
                    msg.num
                );
                return;
            }
            RELAIS_CHANNEL.send(msg).await
        }
        Err(_) => {
            report_error(
                Component::Relais,
                ErrorCode::InvalidData,
                Severity::Warning,
                0,
                &[data.len() as u8],
            )
            .await
        }
    }
}

/// Queues a relay/rollershutter command for `relais_task`, the same way a
/// CAN `Relais`/`Rollershutter` frame would via [`relais_handler`]. Used by
/// the commissioning CLI for live bring-up testing.
pub async fn send_relais_command(msg: Message) {
    RELAIS_CHANNEL.send(msg).await;
}

/// One relay board: its two expanders and their output latches.
struct Board {
    i2c: Device,
    address: [u8; 2],
    kind: Kind,
    latch: [u8; 2],
}

impl Board {
    /// Latch first, all low (all relays off), then every pin an output.
    /// The latch powers up as `0xFF`, so turning the pins into outputs
    /// before clearing it would switch every relay on for a moment. Returns
    /// whether the first expander answered.
    async fn setup(&mut self) -> bool {
        let mut answered = [false; 2];
        for (n, &address) in self.address.iter().enumerate() {
            answered[n] = self.i2c.write(address, &[REG_OUTPUT, 0]).await.is_ok();
        }
        for &address in &self.address {
            self.i2c.write(address, &[REG_CONFIG, 0]).await.ok();
        }
        if answered[0] && !answered[1] {
            console_log!("relais: 0x{:02x} doesn't answer", self.address[1]);
        }
        answered[0]
    }

    /// Sets or clears one output and writes the expander's latch back out,
    /// then makes sure its pins are outputs: an expander that reset since
    /// the last write holds `0xFF` in its latch and has inputs.
    async fn set(&mut self, num: usize, on: bool) {
        let Some((expander, bit)) = self.kind.pin(num) else {
            return;
        };
        if on {
            self.latch[expander] |= 1 << bit;
        } else {
            self.latch[expander] &= !(1 << bit);
        }
        let address = self.address[expander];
        let latch = self.latch[expander];
        if let Err(e) = self.i2c.write(address, &[REG_OUTPUT, latch]).await {
            console_log!("relais: 0x{address:02x}: {e:?}");
        }
        self.i2c.write(address, &[REG_CONFIG, 0]).await.ok();
    }
}

/// Bit `n`: bank `n` is there, for `show`.
static PRESENT: AtomicU8 = AtomicU8::new(0);
/// Bit `n`: bank `n` is an SSR board, as read at start.
static SSR_BANKS: AtomicU8 = AtomicU8::new(0);
static ONBOARD_BUS: StaticCell<Bus> = StaticCell::new();

/// Sets up the board at `address`. Returns it when it answers, or always
/// for the node's own board, as before boards were looked for.
async fn board(bank: u8, bus: &'static Bus, address: [u8; 2], ssr_banks: u8) -> Option<Board> {
    let kind = Kind::from_mask(ssr_banks, bank);
    let mut board = Board {
        i2c: I2cDevice::new(bus),
        address,
        kind,
        latch: [0, 0],
    };
    let found = board.setup().await;
    if !found && bank != 0 {
        return None;
    }
    console_log!(
        "relais: bank {bank}: {} board, {} channels at 0x{:02x}/0x{:02x}{}",
        kind.name(),
        kind.channels(),
        address[0],
        address[1],
        if found { "" } else { ", not answering" }
    );
    PRESENT.fetch_or(1 << bank, Ordering::Relaxed);
    Some(board)
}

/// The first I2C bus, which the node's own board (bank 0) sits on, for
/// [`start`].
pub fn onboard_bus(
    i2c0: esp_hal::peripherals::I2C0<'static>,
    sda: impl PeripheralInput<'static> + PeripheralOutput<'static>,
    scl: impl PeripheralInput<'static> + PeripheralOutput<'static>,
) -> &'static Bus {
    let i2c = I2c::new(i2c0, Config::default())
        .unwrap()
        .with_sda(sda)
        .with_scl(scl)
        .into_async();
    ONBOARD_BUS.init(Mutex::new(i2c))
}

/// Sets up the node's own board on `onboard` (bank 0) and looks for boards
/// without a µC on the `extension` bus (banks 1-4), then starts carrying
/// out relay commands. Runs without boards too, so commands for them are
/// answered with an error instead of piling up.
pub async fn start(
    onboard: Option<&'static Bus>,
    extension: Option<&'static Bus>,
    spawner: &Spawner,
) {
    let mut cfg = config().await;
    let relais_mode = cfg
        .get_u8(config::Key::RelaisMode)
        .await
        .map(Mode::from)
        .unwrap_or(Mode::Off);
    let ssr_banks = cfg.get_u8(config::Key::RelaisBoards).await.unwrap_or(0);
    drop(cfg);
    console_log!("relais init: {relais_mode:?}");
    SSR_BANKS.store(ssr_banks, Ordering::Relaxed);

    let mut boards = [const { None }; BANKS];
    if let Some(bus) = onboard {
        boards[0] = board(0, bus, relais_board::ONBOARD_ADDRESSES, ssr_banks).await;
    }
    if let Some(bus) = extension {
        for bank in 1..BANKS as u8 {
            if let Some(address) = relais_board::extension_addresses(bank) {
                boards[bank as usize] = board(bank, bus, address, ssr_banks).await;
            }
        }
    }
    spawner.spawn(
        relais_task(Relais {
            boards,
            relais_mode,
        })
        .unwrap(),
    );
}

/// Banks with a board (bit `n` = bank `n`) and of those the SSR ones, for
/// `show`.
pub fn boards() -> (u8, u8) {
    let present = PRESENT.load(Ordering::Relaxed);
    (present, present & SSR_BANKS.load(Ordering::Relaxed))
}

pub struct Relais {
    boards: [Option<Board>; BANKS],
    relais_mode: Mode,
}

impl Relais {
    /// Whether `bank` has a board with channel `num` in the relay mode (a
    /// shutter index covers two relays in the rollershutter modes).
    fn accepts(&self, bank: u8, num: usize) -> bool {
        let Some(Some(board)) = self.boards.get(bank as usize) else {
            return false;
        };
        let last = match self.relais_mode {
            Mode::Relais => num,
            _ => num * 2 + 1,
        };
        last < board.kind.channels()
    }

    /// Applies a commanded state to logical relay `num` of `bank`,
    /// interpreting it according to `relais_mode`. In the rollershutter
    /// modes, `num` is a *shutter* index and actually drives the relay pair
    /// `(2*num, 2*num+1)`.
    pub async fn set(&mut self, bank: u8, num: usize, state: State) {
        console_log!("relais set: {bank}/{num} {state:?}");
        match self.relais_mode {
            Mode::Relais => {
                self.sethw(bank, num, state == State::On).await;
            }
            // Wiring variant where even relays switch power to the motor
            // and odd relays select direction, so Up and Down intentionally
            // produce the same pair here (unlike HardwareRollershutter,
            // whose relay pair encodes direction directly).
            Mode::SoftwareRollershutter => match state {
                State::Up | State::Down => {
                    self.sethw(bank, num * 2, true).await;
                    self.sethw(bank, num * 2 + 1, false).await;
                }
                _ => {
                    self.sethw(bank, num * 2, false).await;
                    self.sethw(bank, num * 2 + 1, false).await;
                }
            },
            Mode::HardwareRollershutter => match state {
                State::Up => {
                    self.sethw(bank, num * 2, true).await;
                    self.sethw(bank, num * 2 + 1, false).await;
                }
                State::Down => {
                    self.sethw(bank, num * 2, true).await;
                    self.sethw(bank, num * 2 + 1, true).await;
                }
                _ => {
                    self.sethw(bank, num * 2, false).await;
                    self.sethw(bank, num * 2 + 1, false).await;
                }
            },
            _ => {}
        }
    }

    async fn sethw(&mut self, bank: u8, num: usize, on: bool) {
        if let Some(Some(board)) = self.boards.get_mut(bank as usize) {
            board.set(num, on).await;
        }
    }
}

async fn send_state(bank: u8, num: usize, state: State) {
    let data = Message {
        num,
        state,
        duration: Duration::from_millis(0),
        bank,
    }
    .to_bytes();
    send_can_message(CanMessageType::RelaisState, &data, false).await;
}

/// Drives `relais` from CAN/CLI commands, and applies auto-off schedules
/// tracked by `RelayManager` — e.g. a "Relais on for 500ms" command that
/// must revert without a follow-up message.
#[embassy_executor::task]
async fn relais_task(mut relais: Relais) {
    let mut manager: RelayManager<SLOTS> = RelayManager::new();

    loop {
        let now = Instant::now();

        // 1. Apply any schedules that expired since the last iteration.
        for (slot, state) in manager.poll_expired(now).into_iter() {
            let (bank, num) = relais_board::split(slot);
            relais.set(bank, num, state).await;
            send_state(bank, num, state).await;
        }

        // 2. Wait for the next command or the next schedule to expire.
        let recv = RELAIS_CHANNEL.receive();
        let delay = Timer::after(manager.next_timeout(now));

        match select(recv, delay).await {
            Either::First(msg) => {
                if !relais.accepts(msg.bank, msg.num) {
                    console_log!("relais: no relay {}/{} here", msg.bank, msg.num);
                    report_error(
                        Component::Relais,
                        ErrorCode::InvalidData,
                        Severity::Warning,
                        msg.bank,
                        &[msg.num as u8],
                    )
                    .await;
                    continue;
                }
                let slot = relais_board::index(msg.bank, msg.num);
                let changed = manager.apply_command(slot, &msg.state, msg.duration, Instant::now());
                if changed {
                    relais.set(msg.bank, msg.num, msg.state).await;
                    send_state(msg.bank, msg.num, msg.state).await;
                }
            }
            Either::Second(_) => {}
        }
    }
}
