//! `Echo` loopback: mirrors `Echo.cpp` — on receiving an `Echo` message,
//! immediately resend the identical CAN id and payload back onto the bus
//! (always as a data frame, never RTR, and — since it goes through
//! [`crate::can::send_raw`], not [`crate::can::send_can_message`] — never
//! gated by [`crate::can::SILENCE`]). Used as a bus latency/connectivity
//! probe.

use gateway_core::can_id::CanId;
use gateway_core::can_message_type::CanMessageType;

pub async fn dispatch(id: CanId, data: &[u8]) {
    if id.msg_type == CanMessageType::Echo {
        crate::can::send_raw(id.into(), data, false).await;
    }
}
