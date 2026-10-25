# ADR 0017: Firmware update over CAN in acknowledged blocks

**Status:** Accepted. Supersedes [ADR 0010](../../../../docs/adr/0010-can-ota-rework-planned.md).

## Context

The original CAN update streams the image as 8-byte `FlashWrite` frames paced
by fixed delays. Nothing says whether a frame arrived. The delays were a guess
at how long the node needs: the node writes flash inline in its receive task
(`esp-hal-ota` writes a sector with read-modify-erase-write, tens of ms with
the CPU stalled), while the ESP32's CAN receive FIFO holds about five frames
(~13 ms at 50 kbit/s). Frames sent meanwhile are simply gone, and a gap in
the image is only noticed by the CRC-32 at the very end.

## Decision

A block protocol with two message types (numbers 168 and 169), see
[`protocol-can.md`](../../../../docs/protocol-can.md#firmware-update-in-blocks):

- The image goes in **blocks** of 1 KiB (a `const`, any multiple of 256 that
  divides the 4 KiB flash sector; the node learns it from `Begin`). A header
  frame carries the index, length and CRC-16 of the block, the data follows as
  `OtaData` frames of 8 bytes, back to back, **without delays**.
- The node collects a block, checks length and CRC and answers `Ack` (safe) or
  `Nak` (frames missing, bad CRC). The sender repeats a refused or unanswered
  block, up to 8 times. Flash is only written while the sender waits for an
  answer: blocks collect in a 4 KiB buffer and are written when a sector is
  full; the ack of the block that completes a sector comes after the write.
- A repeated block (its ack was lost) is acknowledged again and not taken
  twice; blocks are strictly in order.
- Inside a block all 8 bytes of a frame are data; a lost frame is noticed by
  the missing frame count and the CRC, and the whole block is repeated (no
  per-frame numbers, no selective retransmission).
- The image CRC-32 and size go in `Begin`; at `End` the node verifies the whole
  image, read back from flash, before it makes it the one to boot.
- The nodes speak **only** this protocol. The gateway speaks both: the old
  streaming sequence stays for the old STM32 nodes, for updating all devices
  of a type at once (the web UI instead sends the image to the devices of a
  type one after the other), and as a **fallback** for a node that does not
  answer `Begin` (its firmware is older), so that the first update to this
  protocol can still be done the old way.
- A new image is **pending** after the flash and is only kept when it has been
  *heard on the bus*: the node confirms it (`ota_mark_app_valid`) when its first
  `Echo` is answered, not at the start of the boot. A node whose echo guard may
  not restart it (a bench device without a gateway) confirms after two
  minutes. An image that boots but cannot talk is not kept; the bootloader goes
  back to the old one.

## Consequences

- Throughput is about 1 KiB per 0.35 s on a 50 kbit/s bus plus ~60 ms for every
  sector write: a 400 KB image takes about 2.5 minutes, the old way took about
  ten and was unreliable.
- The block repeat is whole-block: it rides out about one lost frame in 500
  (a 1 KiB block is 128 frames). A worse bus needs a smaller block size, which
  is one constant (tested: 256-byte blocks get through at one frame in 100).
- The protocol is a pair of pure state machines in `common-core`
  (`ota_sender`, `ota_receiver`) tested against each other over a simulated bus
  that loses and damages frames.
- `UpdateSilence` must not silence the update: the new message types are exempt
  on the node, which also silences itself for the duration of an update.
- A broadcast update by type is not possible with acknowledgements; the old
  stream remains for the (rare) case of a type update through the REST route.
