//! `ExtensionMode::Pwm`: PCA9685 PWM boards on the extension's I2C bus
//! ([`cancomponents_core::pca9685`]). `LampGroup` messages with bank 1-16
//! set the channels in their bitmask on the board of that bank to their
//! value (0-255); bank 0 (the board's own outputs) isn't driven.

use crate::console_log;
use cancomponents_core::pca9685::{self, BOARDS, CHANNELS};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Timer;
use esp_hal::i2c::master::I2c;
use esp_hal::Async;

/// A `LampGroup` command for [`pwm_task`].
struct Command {
    value: u8,
    bitmask: u32,
    bank: u8,
}

static COMMANDS: Channel<CriticalSectionRawMutex, Command, 8> = Channel::new();

pub struct Pwm;

impl Pwm {
    pub fn init(i2c: I2c<'static, Async>, spawner: &Spawner) {
        spawner.spawn(pwm_task(i2c).unwrap());
    }
}

/// Looks for boards at every bank's address, sets up those found, then
/// carries out the `LampGroup` commands for them.
#[embassy_executor::task]
async fn pwm_task(mut i2c: I2c<'static, Async>) {
    let mut present = 0u16;
    for bank in 1..=BOARDS {
        let Some(address) = pca9685::address(bank) else {
            continue;
        };
        if setup(&mut i2c, address).await.is_ok() {
            present |= 1 << (bank - 1);
            console_log!("pwm: PCA9685 for bank {bank} at 0x{address:02x}");
        }
    }
    if present == 0 {
        console_log!("pwm: no PCA9685 found");
        return;
    }
    loop {
        let Command {
            value,
            bitmask,
            bank,
        } = COMMANDS.receive().await;
        let Some(address) = pca9685::address(bank) else {
            continue;
        };
        if present & (1 << (bank - 1)) == 0 {
            continue;
        }
        let duty = pca9685::duty(value);
        for channel in (0..CHANNELS).filter(|c| bitmask & (1 << c) != 0) {
            if set_channel(&mut i2c, address, channel, duty).await.is_err() {
                console_log!("pwm: bank {bank} didn't answer");
                break;
            }
        }
    }
}

/// 1 kHz, open-drain outputs, all channels off.
async fn setup(i2c: &mut I2c<'static, Async>, address: u8) -> Result<(), ()> {
    let prescale = pca9685::prescale(pca9685::FREQUENCY_HZ);
    let auto = pca9685::MODE1_AUTO_INCREMENT;
    let steps: [&[u8]; 4] = [
        &[pca9685::REG_MODE1, pca9685::MODE1_SLEEP | auto],
        &[pca9685::REG_PRESCALE, prescale],
        &[pca9685::REG_MODE2, pca9685::MODE2_OPEN_DRAIN],
        &[pca9685::REG_MODE1, auto],
    ];
    for step in steps {
        i2c.write_async(address, step).await.map_err(|_| ())?;
    }
    // The oscillator needs 500 µs before the restart.
    Timer::after_millis(1).await;
    i2c.write_async(
        address,
        &[pca9685::REG_MODE1, pca9685::MODE1_RESTART | auto],
    )
    .await
    .map_err(|_| ())?;
    for channel in 0..CHANNELS {
        set_channel(i2c, address, channel, 0).await?;
    }
    Ok(())
}

async fn set_channel(
    i2c: &mut I2c<'static, Async>,
    address: u8,
    channel: usize,
    duty: u16,
) -> Result<(), ()> {
    let r = pca9685::channel_registers(duty);
    let register = pca9685::REG_LED0 + 4 * channel as u8;
    i2c.write_async(address, &[register, r[0], r[1], r[2], r[3]])
        .await
        .map_err(|_| ())
}

/// Handles `LampGroup`: `[value, bitmask (24 bits LE), bank]`.
pub async fn lamp_group(data: &[u8], remote: bool) {
    if remote || data.len() < 4 {
        return;
    }
    let command = Command {
        value: data[0],
        bitmask: u32::from_le_bytes([data[1], data[2], data[3], 0]),
        bank: data.get(4).copied().unwrap_or(0),
    };
    if command.bank != 0 && COMMANDS.try_send(command).is_err() {
        console_log!("pwm: too many lamp commands at once, one dropped");
    }
}
