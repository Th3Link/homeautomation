//! Drives up to 4 physical buttons: polls their levels every
//! [`POLL_MS`] and reports what `cancomponents_core::button_fsm::ButtonFsm`
//! (pure, unit-tested, the original firmware's behaviour) decides over CAN.
//!
//! Polling instead of edge interrupts is what the original's button driver
//! did too; it keeps the timing (50 ms dead time, 250 ms click window, hold
//! from 2 s every 500 ms) exactly the original's.

use crate::can::send_can_message;
use cancomponents_core::button_fsm::{ButtonFsm, POLL_MS};
use cancomponents_core::can_message_type::CanMessageType;
use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker};
use esp_hal::gpio::{Input, InputConfig, InputPin, Pull};

pub struct Button;

impl Button {
    /// Configures 4 GPIOs as pulled-up inputs (pressed = low) and spawns a
    /// task polling them, reported as buttons `first`..`first + 3` (0 on
    /// the board, 4 on the extension).
    pub fn init(
        button0: impl InputPin + 'static,
        button1: impl InputPin + 'static,
        button2: impl InputPin + 'static,
        button3: impl InputPin + 'static,
        first: u8,
        spawner: &Spawner,
    ) {
        let config = InputConfig::default().with_pull(Pull::Up);
        let inputs = [
            Input::new(button0, config),
            Input::new(button1, config),
            Input::new(button2, config),
            Input::new(button3, config),
        ];
        spawner.spawn(run(inputs, first).unwrap());
    }
}

/// One per set of 4 buttons: the board's and the extension's.
#[embassy_executor::task(pool_size = 2)]
async fn run(inputs: [Input<'static>; 4], first: u8) {
    let mut buttons = [0, 1, 2, 3].map(|i| ButtonFsm::new(first + i));
    let mut ticker = Ticker::every(Duration::from_millis(u64::from(POLL_MS)));
    loop {
        ticker.next().await;
        for (input, fsm) in inputs.iter().zip(buttons.iter_mut()) {
            for msg in fsm.poll(input.is_low()) {
                send_can_message(CanMessageType::ButtonEvent, &msg.to_bytes(), false).await;
            }
        }
    }
}
