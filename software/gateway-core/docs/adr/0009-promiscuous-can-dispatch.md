# ADR 0009: Replace the dispatcher-list CAN fan-out with direct `msg_type` routing

**Status:** Accepted

## Context

The original C++ gateway routes incoming CAN frames through a runtime list
of `ICANDispatcher*` (`CAN::dispatch` iterates a `std::vector`, calling
each registered dispatcher's `dispatch()` until one returns `true`,
meaning "I've handled this frame, stop looking"). Each `Bridge*` class
registers itself in this list at construction time. In practice, every
live `msg_type` is handled by exactly one dispatcher — there's no real
contention where two dispatchers compete for the same message type, so
the "first `true` wins" consumption semantics never actually do anything
beyond what a plain `match` on `msg_type` would.

## Decision

`gateway-hardware`'s `can::dispatch` routes each received frame directly:
it always feeds the frame to the device registry, then does a single
`match` on `msg_type` calling straight into the relevant translation
function in `translate.rs`. There is no runtime-registered dispatcher
list and no "claims the frame" return value.

## Consequences

- Simpler, and the compiler checks the `match` is exhaustive over
  `CanMessageType` — a newly-added message type that needs handling can't
  be silently forgotten the way it could be with a mutable runtime list
  someone has to remember to register a new dispatcher into.
- If a future message type genuinely does need to be seen by more than
  one handler (unlike anything in the current protocol), this routing
  has to change to a real fan-out — that's a small, localized change in
  `can::dispatch`, not a structural one.
- The device registry (`gateway_core::device_list`) still effectively
  behaves like the original's always-non-consuming `DeviceList` dispatcher
  — it observes every frame unconditionally, before the `match` — so that
  part of the original's fan-out behavior is preserved exactly, just not
  via a runtime list.
