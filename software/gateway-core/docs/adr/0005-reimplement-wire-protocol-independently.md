# ADR 0005: Reimplement the CAN/MQTT wire protocol independently rather than depend on `cancomponent-rs`

**Status:** Superseded by [ADR 0012](../../../../docs/adr/0012-shared-common-crates.md)

## Context

`cancomponent-rs` (the sibling CAN-node firmware project, kept in this
repository only as local reference and not otherwise part of it) already
implements the same 29-bit CAN ID layout, `CanMessageType` discriminants,
and `DeviceType` discriminants this gateway needs, in its own `cc-core`
crate. Depending on it directly (a git dependency on its published repo)
would avoid re-typing ~150 lines of wire-format enums and bit arithmetic,
and would keep the two projects automatically in sync if the protocol
changes.

Against that: `cancomponent-rs`'s presence in this workspace is explicitly
temporary/reference-only, and its own protocol surface is scoped to what
a *node* needs (things like `filter_code_mask` for a node's own hardware
acceptance filter), not what the gateway needs (promiscuous reception,
gateway-specific RPC/OTA-orchestration payload shapes that have no
node-side equivalent). A git dependency also couples this project's build
to another repository's availability and release cadence for a handful of
enums that are, in practice, a fixed wire contract that essentially never
changes shape.

## Decision

`gateway-core` reimplements the CAN ID/message-type/device-type wire
format from scratch, verified bit-for-bit against the original C++
gateway source during the port (not against `cancomponent-rs`, though the
two do turn out to match exactly — see
[`protocol-can.md`](../../../../docs/protocol-can.md)). No dependency, git
or otherwise, on `cancomponent-rs`.

## Consequences

- The two projects' protocol implementations can drift if the wire
  contract ever changes and only one side is updated — there's no
  compiler-enforced link keeping them in sync. This is an accepted
  trade-off for not coupling this project's build to another repository.
- If the two projects' protocols do need to change together going
  forward, that has to be done as a deliberate, coordinated update in
  both places, caught by each project's own test suite (or by
  interoperability testing on a real bus) rather than by a shared crate
  failing to compile.
- The duplication is small and stable (a handful of `#[repr(u8)]` enums
  and ~30 lines of bit-shift arithmetic), which is what makes this
  trade-off acceptable — this reasoning would not hold for a larger or
  faster-moving shared surface.
