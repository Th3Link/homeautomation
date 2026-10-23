# ADR 0014: On-gateway automation rules — event-driven, JSON over a resource-oriented REST API

**Status:** Proposed — specified in
[`spec-automation-rules.md`](../spec-automation-rules.md), implementation
planned in [`plan-automation-rules.md`](../plan-automation-rules.md).

This is the first, single-gateway half of
[ADR 0011](../../../../docs/adr/0011-gateway-mesh-planned.md): automation
running on the gateway itself, independent of MQTT and the hub. Rules that
span several gateways (ADR 0011's mesh half) are out of scope here, but the
rule format is designed so they can be added later without breaking it.

## Context

Today every automation ("button pressed → light on") runs in the hub
(openHAB/Home Assistant): CAN event → gateway → MQTT → broker → hub →
broker → gateway → CAN. If any link in that chain is down — network,
broker, hub — automations between two devices on the same CAN segment stop
working. That is the main driver behind ADR 0011.

Three things need deciding before building it:

1. **How rules are expressed.** Requirement: a rule has a name and is "if
   *this* then *that*", where *this* can be combined with AND/OR and
   *that* may be several actions.
2. **How rules get onto the gateway**: created, read and deleted over a
   REST API.
3. **How rules execute**: when a rule is evaluated, when it fires.

### Rule format

| Option | For | Against |
|---|---|---|
| **JSON** | Hand-readable and -writable, works from `curl`, Home Assistant/openHAB, a future web UI; `serde` + `serde-json-core` are `no_std` without allocation; `picoserve` already has JSON support | Verbose; text parsing on the device |
| CBOR / MessagePack | Compact, fast to parse | Not hand-readable/-writable, every client needs a library — the wrong trade-off for a configuration API |
| YAML / TOML | Nicer to write by hand | No usable `no_std`, allocation-free parser; TOML nests condition trees poorly |
| Custom text DSL (`when button(...) and ... then ...`) | Most concise to write | A hand-written on-device parser with good error messages; every client has to generate text |

JSON wins for the API. A text DSL remains possible later as a client-side
or web-UI convenience that compiles to the same JSON — with no firmware
change.

On flash the gateway does **not** keep the JSON text but the parsed,
validated rule as `postcard` (`serde`'s compact binary format for embedded
use, `no_std`, no allocation) behind a format-version byte: smaller, no
JSON parsing at boot, and only rules that already passed validation ever
reach flash.

### Execution

A purely state-based model ("while the condition holds…") cannot express
button presses; a purely event-based one cannot express "…and it's dark".
Real rules need both ("button 2 pressed **and** brightness below 40").

## Decision

1. **Rule model** (details in the spec): a rule has a `name` (which is
   also its key), `enabled`, `when` and `then`. `when` is a tree of
   `all` (AND), `any` (OR), `not` and leaf predicates. Predicates are
   either **events** (button, presence) or **states** (relay channel,
   sensor value with threshold and hysteresis). `then` is an ordered list
   of 1–8 actions (relay, rollershutter, lamp, nightlight, MQTT publish,
   raw CAN frame).
2. **Format**: JSON on the API; `postcard` with a version byte on flash.
3. **API**: resource-oriented under `/api/v1/rules` — `GET` on the
   collection, `GET`/`PUT`/`DELETE` on `/api/v1/rules/{name}`. `PUT`
   creates or replaces (idempotent; no `POST`). Errors are RFC 9457
   `application/problem+json` carrying the JSON path of the offending
   field. Basic Auth on every endpoint (ADR 0008). This is deliberately
   the opposite of `/control.json` (one verb-dispatch endpoint, errors
   silently dropped — see `technical-debt.md`) and is meant as the
   template for that endpoint's eventual replacement.
4. **Execution is event-driven and edge-triggered**: every CAN frame the
   engine understands updates a state cache and triggers evaluation of
   the rules that reference it. Event predicates are only true during the
   evaluation of their own event. A rule **fires when its expression goes
   from false to true** — which gives exactly one firing per button press
   and exactly one per threshold crossing, with no extra mechanism. All
   rules are evaluated against the same snapshot first; the collected
   actions are executed afterwards.
5. **Loop/storm protection**: an optional per-rule `cooldown_ms`, plus a
   hard limit — a rule firing more than 10 times within one second is
   suspended until re-enabled through the API.
6. **Unsatisfiable rules are rejected at validation**, not at runtime
   (e.g. AND of two events, which never happen at the same instant).
7. **Storage**: the `storage` partition (100 KiB, kept unused since the
   original partition layout was restored) holds the rules — no new
   partition, so OTA compatibility with the original layout is
   unaffected.
8. Rules run **regardless of MQTT/network**: they're loaded at boot,
   before any network is up.

## Consequences

- Basic automations ("button → light", "too warm → fan") keep working
  without hub, broker or network — ADR 0011's WAF gain, for one gateway.
- The gateway needs an HTTP server. It would anyway (web UI,
  `/control.json`), but none is wired up yet (`picoserve` is only a
  dependency) — it becomes a prerequisite and is shared later.
- Device references in rules are `{type, id}`, i.e. the CAN address:
  stable without a device list, but less readable than custom names.
  Accepting names is a later convenience (spec, open points).
- No clock, so v1 has no time-of-day conditions (sunset, "at night"). That
  needs SNTP or a CAN time source — a separate decision.
- Relay-state conditions and `toggle` depend on correct per-channel
  `RelaisState` reports. `gateway-core` currently decodes only byte 0,
  while the Rust nodes send `[channel, state, …]` — this has to be fixed
  first (plan, phase 0).
- No TLS: like the rest of the web UI, the rule API is plain HTTP with
  Basic Auth on the LAN. Anyone able to sniff the LAN can read credentials
  and change rules. Accepted for now as in ADR 0008; revisit together
  with ADR 0011's mesh trust model.
- Two places for automations: hub *and* gateway. Conflicts (the hub
  switches off what the gateway switched on) are the operator's
  responsibility; the gateway publishes every firing over MQTT so the hub
  can at least see them.
