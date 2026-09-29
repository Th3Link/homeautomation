# ADR 0010: Rework CAN-bus OTA into a chunked, acknowledged, retransmitting protocol

**Status:** Proposed — not yet implemented. The port currently reproduces
the original protocol as-is; see
[`../technical-debt.md`](../technical-debt.md#can-bus-ota-is-unreliable)
for the concrete failure mode driving this ADR.

## Context

The original CAN-bus OTA protocol (`CANUpdate.cpp`, faithfully reproduced
in `gateway_core::can_ota`/`gateway-hardware::can_update` — see
[`../protocol-can.md`](../protocol-can.md#ota-flashstartflashwriteflashcomplete)
for the exact frame sequence) streams firmware as a flat sequence of
unacknowledged `FlashWrite` frames, paced only by a fixed per-frame delay
and a periodic pause every 4096 bytes. There is no per-chunk checksum, no
sequence numbering, and no acknowledgement — a dropped or corrupted frame
anywhere in the stream is silently invisible to the sender, and the
target's flash simply receives whatever arrived (correct bytes, garbled
bytes, or a gap) with nothing to force a retry.

This is reported as unreliable in practice ("das update über can ist
mist. das schlägt oft fehl.") — expected, given CAN offers no built-in
retransmission for this usage pattern (data frames aren't individually
acknowledged at the application level here) and the bus is shared with
live traffic from other nodes during the update.

## Decision (proposed shape, not yet finalized in detail)

Replace the flat `FlashWrite` stream with a chunked protocol where each
chunk carries enough information for the receiver to detect corruption
and loss, and to actually correct it before the transfer is considered
complete:

- Each chunk gets a **sequence number**, so the receiver can detect a
  missing or out-of-order chunk instead of silently accepting whatever
  arrives next.
- Each chunk carries a **checksum** over its payload, so the receiver can
  detect in-flight corruption CAN's own frame-level CRC didn't catch (or
  that got past it due to a higher-level bug) before committing the chunk
  to flash.
- The receiver **writes each chunk to flash before acknowledging it**,
  not just buffers it — so an ACK is a real claim that the data is safely
  persisted, not just "I received some bytes."
- The receiver **ACKs each chunk** (or NAKs / times out), and the sender
  **retransmits** on a missing ACK or a NAK, instead of blindly streaming
  forward regardless of what actually landed.

This changes the wire protocol for `FlashWrite` (or introduces new
message types alongside it) — the exact frame layout, ACK message type,
and retry/backoff policy still need to be designed and are intentionally
left open here; this ADR records the decision to do this rework and the
properties the new protocol must have, not the finished design.

## Consequences

- The transfer gets slower per byte (round-trip per chunk instead of a
  fire-and-forget stream) — acceptable, since reliability is the actual
  problem being solved, not throughput; firmware images for these nodes
  are small.
- `gateway_core::can_ota` (currently a pure step-sequence generator with
  no notion of acknowledgement) needs a real state machine: per-chunk
  wait-for-ack, retry-with-limit, and failure reporting all need to be
  modeled and unit-tested the same way the rest of `gateway-core` is.
- The receiving node's firmware (out of scope for this repository, e.g.
  `cancomponent-rs`'s `cc-hardware::update`) has to implement the matching
  half of whatever protocol is designed — this is a two-sided protocol
  change, not something the gateway can improve unilaterally.
