# Plan: on-gateway automation rules

Implements [`spec-automation-rules.md`](spec-automation-rules.md) /
[ADR 0014](adr/0014-on-gateway-automation-rules.md).

Each phase ends in a mergeable PR with CI green. Phases 1–3 are pure
`gateway-core` logic and fully testable on the host, so most of the work
needs no hardware. Phases 4–5 wire it into the firmware. Phase 6 needs a
working gateway on a real bus.

## Phase 0 — Prerequisites (small, blocks relay state)

Relay state predicates and `toggle` need per-channel relay state, which
the gateway can't currently decode.

- [ ] Pin down the `RelaisState` wire format the nodes actually send.
      `cancomponents-hardware` sends `[channel, state, duration u24,
      bank]` with state `Off=0 Up=1 Down=2 On=3`, while
      `gateway-core::relais::decode_relais_state` reads only byte 0
      (legacy behavior — which today publishes the *channel number* as the
      state on `canbus/relais_state/…`). Check against the legacy C++ nodes
      still deployed, if any, and decide whether to support both formats.
- [ ] Same for `RollershutterState` (`number` + `bank`) and its mapping
      to spec channel numbers (0–79 including extension boards).
- [ ] Map the spec's semantic values (`on`/`off`, `up`/`down`/`stop`) onto
      the command bytes the nodes accept (the MQTT command path currently
      passes the raw byte through, so `1` reaches a relay node as `Up`).
- [ ] Fix the decoder and the `canbus/relais_state` topic accordingly —
      a behavior change on the MQTT side, needs its own note in
      `protocol-mqtt.md` and, if it breaks the hub config, an ADR.

**Done when**: decoder and encoder have tests pinning the real byte
layouts, and the open point in spec §10.1 is closed.

## Phase 1 — Rule model, JSON, validation (`gateway-core`)

New module `gateway_core::automation`.

- [ ] Dependencies: `serde` (derive, no default features),
      `serde-json-core` for the API format, `postcard` (with the
      `heapless` feature, no allocation) for flash. Licenses fit the
      existing `deny.toml` allow-list (MIT/Apache-2.0) — confirm with
      `cargo deny`.
- [ ] Types: `Rule`, `DeviceRef`, `Condition` (combinators + leaf
      predicates), `Action`, all `heapless`. The condition tree is stored
      as a flat array of nodes with child indices (max 16), not as
      recursive boxes, so there's no allocation and the size is fixed.
- [ ] JSON decode into the API shape (externally tagged, as in the spec),
      then convert to the flat internal form; JSON encode back for `GET`.
- [ ] Validation with error `code` + JSON `path`, covering every row of
      spec §6.3, including `unsatisfiable` (§3.3) and `unknown_field`.
- [ ] `postcard` encode/decode with a leading format version byte.

**Tests**: every example in the spec parses and round-trips JSON →
internal → JSON and internal → postcard → internal; one test per error
code with the expected `path`; size of `Rule` printed/asserted so the RAM
budget (≤ 16 KiB for 32 rules + cache) is tracked from the start.

## Phase 2 — Engine (`gateway-core`)

- [ ] `Input` type and a pure `fn decode_input(CanId, &[u8]) -> Option<Input>`
      reusing the existing decoders (button, PIR, relay/rollershutter
      state, fixed-point sensors, ambient light).
- [ ] `StateCache`: last value per (device, channel) and per (device,
      sensor kind, sub id), fixed capacity, oldest entry evicted when
      full.
- [ ] `Engine::on_input(&mut self, input, now) -> Vec<Action, N>`:
      update the cache, find affected rules (§5.2), evaluate on the
      snapshot, rising edge (§5.3), hysteresis (§3.2), cooldown and storm
      guard (§5.5), actions ordered by rule name (§5.4). `now` is passed
      in — the engine never reads a clock, so it stays deterministic and
      testable like the rest of `gateway-core`.
- [ ] Action → CAN frame encoding via the existing encoders (`relais`,
      `lamp`, …); `toggle` resolved from the cache.

**Tests**, table-driven, one scenario per paragraph of spec §5:
button fires exactly once per event; state rule fires once per crossing;
hysteresis suppresses flapping; unknown state is false; `not` of unknown
state; cooldown drops but doesn't defer; storm guard suspends after 10
firings in 1 s and `reset` clears it; two rules firing on the same input
execute in name order on the same snapshot; a rule chain via the
resulting state report; boot with a condition already true fires on the
first report.

## Phase 3 — Rule store abstraction (`gateway-core`)

- [ ] `RuleSet`: the in-RAM set of up to 32 rules plus their runtime
      status, with `put`/`get`/`delete`/`iter`, implementing the atomic
      `PUT` semantics against a storage trait (so the flash part can be
      faked in tests).
- [ ] Load from storage at boot: skip and log undecodable entries (§8).

**Tests**: create/replace/delete status codes (201/200/204/404), table
full (507), failed write leaves the old rule active, reload after a
simulated reboot.

## Phase 4 — Firmware integration (`gateway-hardware`)

- [ ] Rule storage on the `storage` partition via `sequential-storage`
      (map keyed by rule name). `common-hardware::config_store` is keyed
      by `u8` — either generalize it over the key type or add a sibling
      `MapStore`; decide when writing it.
- [ ] Load rules in `main` before the network comes up; run the engine
      from the CAN receive dispatch (`translate::dispatch` already sees
      every decoded frame), push action frames onto the existing CAN send
      queue, `mqtt` actions and `canbus/automation/<name>` notifications
      onto the existing publish queue.
- [ ] Serial console: read-only `rules` and `rule <name>` commands —
      useful without network, cheap.
- [ ] Check the CI memory report: rule table + cache within budget,
      stack budget still healthy.

**Done when**: rules defined in code (temporary, until phase 5) fire on a
real or simulated CAN input and produce the expected frames.

## Phase 5 — HTTP server and rule API (`gateway-hardware`)

No HTTP server is running yet; `picoserve` is only a dependency.

- [ ] Bring up `picoserve` on the `embassy-net` stack: one listener task,
      fixed buffers sized from the 4 KiB body limit, Basic Auth
      middleware (ADR 0008) shared with everything that comes later
      (`/control.json`, web UI).
- [ ] Routes from spec §6.1; `application/problem+json` errors from the
      phase-1 validation result; `413` on oversized bodies.
- [ ] A small shell or Python script in `software/tools/` exercising the
      API with `curl` (create, read, replace, delete, every error path),
      used for the hardware test in phase 6.
- [ ] API reference in `docs` (or the spec itself, if it stays accurate)
      and a changelog note in `requirements.md`.

## Phase 6 — Hardware validation

Blocked until a working gateway is available again.

- [ ] Button → relay toggle: latency under 20 ms (§5.6), measured on the
      bus (CAN analyzer or a second node's timestamps).
- [ ] Rules keep working with the Ethernet cable unplugged and with the
      MQTT broker down.
- [ ] Sensor threshold with hysteresis on a real sensor.
- [ ] Storm guard: two rules toggling each other get suspended, and
      `canbus/automation/<name>` reports it.
- [ ] Reboot: rules survive, a state rule already true fires once after
      boot.
- [ ] Flash wear sanity: many `PUT`s in a row don't corrupt the
      `storage` partition (sequential-storage handles wear leveling;
      verify recovery after a power cut mid-write).

## Order and dependencies

```text
Phase 0 ──────────────────────────┐
Phase 1 ──> Phase 2 ──> Phase 3 ──┼──> Phase 4 ──> Phase 5 ──> Phase 6 (hardware)
                                  │
     (1–3 need no hardware; 0 can run in parallel)
```

## Risks

| Risk | Mitigation |
|---|---|
| `serde-json-core` error positions are coarse | Decode in two steps (JSON → API shape, then validation on typed data), so most errors get a precise `path` from our own validation, and only syntax errors fall back to `invalid_json` without a path |
| `picoserve` RAM and stack use unknown on this target | Measure in phase 5 with the CI memory report; the stack budget is now 138 KiB, so there is room |
| Relay state format turns out inconsistent across node generations | Phase 0 decides before any rule depends on it; worst case, relay state predicates are limited to Rust nodes |
| Rules in the hub and on the gateway fight each other | Operational, not technical: document it; MQTT notifications make firings visible to the hub |
| Plain HTTP + Basic Auth on the LAN | Accepted (ADR 0008/0014); revisit with ADR 0011's trust model |
