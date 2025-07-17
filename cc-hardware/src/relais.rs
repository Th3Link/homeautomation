use esp_println::println;

use crate::relais_manager::RelayManager;
use cancomponents_core::can_id::CanId;
use cancomponents_core::relais_message::{RelaisMessage, RelaisState};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Instant, Timer};
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::i2c::master::{Config, I2c};
use esp_hal::Async;

const BANK: [u8; 2] = [0x26, 0x27];
const MAX_RELAIS: usize = 16;
// hard coded for now until we have an nvs working
const RELAIS_MODE: RelaisMode = RelaisMode::HardwareRollershutter;

pub enum RelaisMode {
    Relais = 0,
    SoftwareRollershutter = 1,
    HardwareRollershutter = 2,
}
static RELAIS_CHANNEL: Channel<CriticalSectionRawMutex, RelaisMessage, MAX_RELAIS> = Channel::new();

pub async fn relais_handler(_id: CanId, data: &[u8], _remote_request: bool) {
    if let Ok(msg) = RelaisMessage::from_bytes(data).await {
        RELAIS_CHANNEL.send(msg).await;
    }
    // silent error, already reportet is relais_message
}

pub async fn rollershutter_handler(_id: CanId, data: &[u8], _remote_request: bool) {
    if let Ok(msg) = RelaisMessage::from_bytes(data).await {
        RELAIS_CHANNEL.send(msg).await;
    }
    // silent error, already reportet is relais_message
}

pub struct Relais {
    i2c: I2c<'static, Async>,
    expanders: [u8; 2],
}

impl Relais {
    pub fn init(
        i2c0: esp_hal::peripherals::I2C0<'static>,
        sda: impl PeripheralOutput<'static>,
        scl: impl PeripheralOutput<'static>,
        spawner: &Spawner,
    ) {
        let mut i2c = I2c::new(i2c0, Config::default())
            .unwrap()
            .with_sda(sda)
            .with_scl(scl)
            .into_async();

        i2c.write(BANK[0], &[0x3, 0x0]).ok();
        i2c.write(BANK[1], &[0x3, 0x0]).ok();
        i2c.write(BANK[0], &[0x1, 0x0]).ok();
        i2c.write(BANK[1], &[0x1, 0x0]).ok();

        let relais = Relais {
            expanders: [0, 0],
            i2c,
        };

        spawner.spawn(relais_task(relais)).unwrap();
    }
    /// Each entry: (expander index, bit position)
    const MAPPING: [(usize, u8); MAX_RELAIS] = [
        (0, 3),
        (0, 2),
        (0, 1),
        (0, 7),
        (0, 6),
        (0, 5),
        (0, 4),
        (1, 11 - 8),
        (1, 10 - 8),
        (1, 9 - 8),
        (1, 15 - 8),
        (1, 14 - 8),
        (0, 0),
        (0, 0),
        (0, 0),
        (0, 0),
    ];

    pub fn set(&mut self, num: usize, state: RelaisState) {
        match RELAIS_MODE {
            RelaisMode::Relais => self.sethw(num, state),
            RelaisMode::SoftwareRollershutter => match state {
                RelaisState::Up => {
                    self.sethw(num * 2, RelaisState::On);
                    self.sethw(num * 2 + 1, RelaisState::Off);
                }
                RelaisState::Down => {
                    self.sethw(num * 2, RelaisState::On);
                    self.sethw(num * 2 + 1, RelaisState::Off);
                }
                _ => {
                    self.sethw(num * 2, RelaisState::Off);
                    self.sethw(num * 2 + 1, RelaisState::Off);
                }
            },
            RelaisMode::HardwareRollershutter => match state {
                RelaisState::Up => {
                    self.sethw(num * 2, RelaisState::On);
                    self.sethw(num * 2 + 1, RelaisState::Off);
                }
                RelaisState::Down => {
                    self.sethw(num * 2, RelaisState::On);
                    self.sethw(num * 2 + 1, RelaisState::On);
                }
                _ => {
                    self.sethw(num * 2, RelaisState::Off);
                    self.sethw(num * 2 + 1, RelaisState::Off);
                }
            },
        }
    }

    fn sethw(&mut self, num: usize, state: RelaisState) {
        if let Some(&(expander, bit)) = Self::MAPPING.get(num) {
            println!("expander {expander}, bit {bit}");
            let mask = 1 << bit;
            if state == RelaisState::On {
                self.expanders[expander] |= mask;
            } else {
                self.expanders[expander] &= !mask;
            }
            println!(
                "i2c write to {:#x?}: {:#x?}",
                BANK[expander], self.expanders[expander]
            );
            self.i2c
                .write(BANK[expander], &[0x1, self.expanders[expander]])
                .ok();
        }
    }
}

#[embassy_executor::task]
async fn relais_task(mut relais: Relais) {
    let mut manager: RelayManager<MAX_RELAIS> = RelayManager::new();

    loop {
        let now = Instant::now();

        // 1. Abgelaufene Zeitsteuerungen
        for (num, state) in manager.poll_expired(now).into_iter() {
            relais.set(num as usize, state);
        }

        // 2. Warte auf nächsten Befehl oder nächstes Timeout
        let recv = RELAIS_CHANNEL.receive();
        let delay = Timer::after(manager.next_timeout(now));

        match select(recv, delay).await {
            Either::First(msg) => {
                println!("relais future met");
                let changed =
                    manager.apply_command(msg.num, msg.state, msg.duration, Instant::now());
                if changed {
                    relais.set(msg.num, msg.state);
                    println!("set relais");
                }
            }
            Either::Second(_) => {}
        }
    }
}
