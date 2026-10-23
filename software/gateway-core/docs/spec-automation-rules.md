# Spec: on-gateway automation rules

**Status:** Draft — design decided in
[ADR 0014](adr/0014-on-gateway-automation-rules.md), implementation
planned in [`plan-automation-rules.md`](plan-automation-rules.md).

The gateway stores a set of named rules of the form *if this, then that*
and executes them itself, directly on the CAN bus — with or without MQTT,
broker or hub. Rules are created, read and deleted over a REST API.

"Must", "must not", "should" and "may" are used in the RFC 2119 sense.

## 1. Rule

```json
{
  "name": "hallway-light",
  "enabled": true,
  "cooldown_ms": 0,
  "when": { "button": { "device": { "type": "button", "id": 12 }, "button": 2, "event": "single" } },
  "then": [
    { "relais": { "device": { "type": "relais", "id": 5 }, "channel": 3, "set": "toggle" } }
  ]
}
```

| Field | Type | Required | Meaning |
|---|---|---|---|
| `name` | string | yes | Unique key. `^[a-z0-9][a-z0-9_-]{0,31}$` — URL-safe, used verbatim in the API path. |
| `enabled` | bool | no, default `true` | Disabled rules are stored but never evaluated. |
| `cooldown_ms` | integer 0–3 600 000 | no, default `0` | Minimum time between two firings of this rule; firings inside the window are dropped (not deferred). |
| `when` | condition | yes | See §3. |
| `then` | array of actions, 1–8 entries | yes | Executed in order, see §4. |

Unknown fields must be rejected (catches typos such as `"enabeld"`).
The read-only `status` object returned by the API (§6.2) is the one
exception: it is ignored on input, so a `GET`→edit→`PUT` round trip works.

## 2. Device references

Every predicate and action addresses a CAN device by its CAN address:

```json
{ "type": "relais", "id": 5 }
```

- `type`: one of `relais`, `ssr`, `rollershutter`, `button`, `gateway`,
  `legacy_relais`, `legacy_lamps` (the `DeviceType` names, lower snake
  case).
- `id`: the device id, 1–255. `0` (broadcast) is rejected.

The reference is resolved to a CAN id directly, so a rule works even if
the device has not been seen on the bus since boot. The device's type is
not cross-checked against the predicate or action kind: nodes are
multi-function.

## 3. Conditions (`when`)

A condition is exactly one of the following objects.

### 3.1 Combinators

| Object | Meaning |
|---|---|
| `{ "all": [c, …] }` | AND — true if every child is true. 1–8 children. |
| `{ "any": [c, …] }` | OR — true if at least one child is true. 1–8 children. |
| `{ "not": c }` | Negation of a **state** condition (see §3.3). |

Combinators nest to a depth of at most 4. A rule's whole condition tree
has at most 16 nodes (combinators and leaves together).

### 3.2 Leaf predicates

There are two kinds of predicate: **events** happen at a single instant,
**states** hold over time.

| Predicate | Kind | Example | True when |
|---|---|---|---|
| `button` | event | `{"button": {"device": D, "button": 2, "event": "single"}}` | A `ButtonEvent` for that device and button with that event is being evaluated. `event` ∈ `pressed`, `released`, `hold`, `single`, `double`, `triple`. `button` 0–255. |
| `presence` | state | `{"presence": {"device": D, "is": "detected"}}` | The last `PirSensor` report of the device was *detected* (`is: "detected"`) or *clear* (`is: "clear"`). `Released` counts as clear, every other event kind as detected. |
| `relais` | state | `{"relais": {"device": D, "channel": 3, "is": "on"}}` | The last reported state of that relay channel is `on`/`off`. |
| `rollershutter` | state | `{"rollershutter": {"device": D, "channel": 1, "is": "up"}}` | The last reported state of that rollershutter channel is `up`/`down`/`stopped`. |
| `sensor` | state | `{"sensor": {"device": D, "kind": "temperature", "above": 25.0, "hysteresis": 0.5}}` | The last reading of that sensor is above/below the threshold (see below). |

`sensor` details:

- `kind` ∈ `temperature` (°C), `pressure` (hPa), `humidity` (%RH), `co2`,
  `voc`, `air_quality`, `ambient_light` (raw count). Units are as
  reported by the node; fixed-point kinds already decoded (raw ÷ 16).
- Exactly one of `above` or `below` (number).
- `hysteresis` (number ≥ 0, default 0): once true, `above: T` only turns
  false again below `T − hysteresis`; once true, `below: T` only turns
  false again above `T + hysteresis`. That stops a noisy reading around
  the threshold from firing the rule over and over.
- `sub_id` (optional, hex string, e.g. `"0x28ff641e8016"`): selects one
  sensor when a device reports several of the same kind (the 48-bit
  sensor id from the CAN frame). Without it, any sensor of that kind on
  the device matches, using whichever reported last.

A state predicate whose input has **never been reported** since boot is
false. (`not` of it is therefore true.)

### 3.3 Validity

A rule is rejected with `unsatisfiable` (§6.3) if its condition can never
become true or would fire on nothing:

- An `all` with two or more children that each contain an event
  predicate. Two events never occur at the same instant, so the AND is
  always false. (Two events under `any` are fine.)
- `not` with an event predicate anywhere beneath it. "Not this button
  press" is true at every moment except the press itself.

## 4. Actions (`then`)

Actions are executed in the listed order. CAN actions are queued on the
bus immediately, without waiting for any confirmation from the node.

| Action | Example | Effect |
|---|---|---|
| `relais` | `{"relais": {"device": D, "channel": 3, "set": "on", "for_ms": 600000}}` | Switches a relay channel. `set` ∈ `on`, `off`, `toggle`. `toggle` uses the last reported state of that channel (unknown counts as off, so the first toggle switches on). Optional `for_ms` (1–16 777 215): the node reverts the channel after that time, by itself — survives a gateway reboot. |
| `rollershutter` | `{"rollershutter": {"device": D, "channel": 1, "move": "down", "for_ms": 30000}}` | `move` ∈ `up`, `down`, `stop`; optional `for_ms` as above. |
| `lamp` | `{"lamp": {"device": D, "value": 200, "mask": 16777215, "bank": 0}}` | Sets the LEDs selected by `mask` (24-bit, integer) to `value` (0–255). Optional `bank` (0–255). |
| `nightlight` | `{"nightlight": {"device": D, "value": 40}}` | Sets the nightlight to `value` (0–255). |
| `mqtt` | `{"mqtt": {"topic": "home/alarm", "payload": "on", "retain": false}}` | Publishes one message (QoS 0) if MQTT is connected; dropped otherwise, never queued or retried. `topic` ≤ 96 bytes, `payload` ≤ 200 bytes. |
| `can` | `{"can": {"device": D, "msg_type": 130, "data": "0301"}}` | Raw escape hatch: sends `data` (hex, 0–8 bytes) as message type `msg_type` (0–255) to the device. Not validated beyond length — for message types the other actions don't cover. |

Lamps report no state, so there is no `lamp` predicate and no lamp toggle.

## 5. Evaluation

### 5.1 Inputs and the state cache

The gateway already decodes every CAN frame for MQTT. The engine sees the
same decoded frames:

- `ButtonEvent` → a **button event**, not stored.
- `PirSensor` → presence **state** of that device.
- `RelaisState`, `RollershutterState` → channel **state**.
- Fixed-point sensor frames, `AmbientLightSensor` → sensor **state**
  (last value per device, kind and sub id).

Every state input is stored in a cache in RAM, which starts empty at boot.

### 5.2 When a rule is evaluated

For each decoded input, after updating the cache, the gateway evaluates
every enabled, non-suspended rule whose condition references that input
(same device, and for channels and sensors the same channel or
kind/sub id). Other rules are not evaluated.

### 5.3 When a rule fires — rising edge

Each rule remembers the result of its last evaluation, initially false.
**A rule fires when its condition evaluates to true and the previous
result was false.** Consequences:

- Event rules fire once per matching event: the event predicate is true
  only while its own event is being evaluated, so the condition drops
  back to false before the next event.
- State rules fire once per crossing ("temperature rises above 25"), not
  on every reading while it stays above. Crossing back and forth fires
  again each time it becomes true.
- A state rule whose condition is already true when its inputs are first
  reported after boot fires then (false → true). Example: after a
  reboot in a warm room, "above 25 → fan on" switches the fan on.
- Creating, replacing or enabling a rule through the API resets its
  previous result to false and clears its suspension.

### 5.4 Snapshot, then execute

For one input, all affected rules are evaluated against the same cache
state first. The actions of every rule that fires are then executed, in
rule name order (byte-wise ascending), each rule's actions in their
listed order. The outcome therefore doesn't depend on rule order.

The state reports caused by those actions (e.g. the `RelaisState` after
switching) are ordinary inputs again and may fire further rules. Chains
like this are intended.

### 5.5 Cooldown and storm protection

- **`cooldown_ms`**: a rule that would fire within `cooldown_ms` of its
  last firing does not fire. Its previous result is still updated, so it
  fires on the next rising edge after the window, not as soon as the
  window ends.
- **Storm guard**: a rule that fires more than **10 times within 1 s** is
  suspended. It is no longer evaluated until a `PUT` of the rule clears
  the suspension, and the suspension is reported in its `status` (§6.2)
  and on MQTT (§7). This catches loops, e.g. two rules toggling each
  other.

### 5.6 Timing

The time from the triggering CAN frame being received to the first action
frame being queued should be under 20 ms.

## 6. REST API

Base path `/api/v1`. Every endpoint requires HTTP Basic Auth with the web
credentials (ADR 0008); without it the response is `401`. Request and
response bodies are JSON (`application/json`), errors use
`application/problem+json` (§6.3). Request bodies over 4 KiB get `413`.

### 6.1 Endpoints

| Method and path | Request body | Success response |
|---|---|---|
| `GET /api/v1/rules` | – | `200`, `{"rules": [rule, …]}` sorted by name, each with `status` |
| `GET /api/v1/rules/{name}` | – | `200`, the rule with `status`; `404` if it doesn't exist |
| `PUT /api/v1/rules/{name}` | the rule (`name` optional; if present it must equal `{name}`) | `201` if created, `200` if replaced; body is the stored rule with `status` |
| `DELETE /api/v1/rules/{name}` | – | `204`; `404` if it doesn't exist |

`PUT` is atomic: either the new rule is validated, persisted to flash and
active, or nothing changes. A `2xx` is only sent after the rule is
persisted. There is deliberately no `POST`: the client picks the name, so
`PUT` is idempotent and a retried request can't create a duplicate.

### 6.2 `status` (read-only)

```json
"status": { "suspended": false, "fired_count": 17, "last_fired_ms": 8734121 }
```

`fired_count` counts since boot. `last_fired_ms` is milliseconds of
gateway uptime, or `null` if the rule hasn't fired since boot; there is
no wall clock (§9).

### 6.3 Errors

```http
HTTP/1.1 422 Unprocessable Content
Content-Type: application/problem+json

{
  "type": "about:blank",
  "title": "Invalid rule",
  "status": 422,
  "code": "out_of_range",
  "detail": "value must be between 0 and 255",
  "path": "then[0].lamp.value"
}
```

`path` points to the offending field in the request body. Codes:

| Status | `code` | When |
|---|---|---|
| 400 | `invalid_json` | Body isn't valid JSON, or a field has the wrong type |
| 400 | `invalid_name` | `{name}` doesn't match the name pattern |
| 400 | `name_mismatch` | `name` in the body differs from `{name}` |
| 422 | `unknown_field` | A field not in this spec |
| 422 | `missing_field` | A required field is missing |
| 422 | `out_of_range` | A number, string length or list length is outside its limit |
| 422 | `invalid_value` | An enum value isn't one of the allowed ones (e.g. `event: "tripple"`) |
| 422 | `too_complex` | More than 16 condition nodes, depth over 4, or more than 8 actions |
| 422 | `unsatisfiable` | §3.3 |
| 404 | `not_found` | `GET`/`DELETE` of a rule that doesn't exist |
| 413 | `body_too_large` | Body over 4 KiB |
| 507 | `rule_table_full` | Creating a rule when 32 already exist |
| 500 | `storage_failed` | Flash write failed; nothing changed |

## 7. MQTT notifications

When MQTT is connected, the gateway publishes (QoS 0, not retained):

- `canbus/automation/<name>` with payload `fired` on every firing.
- `canbus/automation/<name>` with payload `suspended` when the storm
  guard suspends the rule.

These are notifications only. Rules can't be managed over MQTT, and
nothing is published while MQTT is down (no queueing).

## 8. Persistence and limits

Rules are stored in the `storage` partition (ADR 0014, decision 7) as
`postcard`, behind a format version byte, keyed by name. They're loaded
at boot before any network comes up, so rules run from the first second
without network. A stored rule that fails to decode (e.g. a format
version this firmware doesn't know) is skipped with a log message and is
not deleted.

| Limit | Value |
|---|---|
| Rules | 32 |
| Condition nodes per rule (leaves + combinators) | 16 |
| Combinator depth | 4 |
| Children per `all`/`any` | 8 |
| Actions per rule | 8 |
| Name | 32 bytes |
| Request body | 4 KiB |
| Target RAM for the loaded rule table + state cache | ≤ 16 KiB (checked by the CI memory report) |

## 9. Out of scope for v1

- **Time**: time-of-day conditions, schedules, delays between actions.
  `for_ms` on relay/rollershutter covers "on for 10 minutes". Needs a
  clock (SNTP or a CAN time source) first.
- **Rules across gateways** (ADR 0011's mesh): device references would
  gain an optional `gateway` field; the format leaves room for it.
- **Device references by custom name** instead of `{type, id}`.
- **Managing rules over MQTT or the serial console** (the console may get
  read-only `rules`/`rule <name>` commands).
- **Lamp state** (nodes don't report it).
- **A text DSL**: possible later in a client, compiling to this JSON.

## 10. Open points

1. **Relay channel numbering and state values on the wire.** The spec
   uses semantic values (`on`/`off`, `up`/`down`/`stopped`) and channel
   numbers as in `canbus/relais_command` (0–79, extension boards
   included). How these map onto `RelaisState` (`[channel, state, …]` from
   the Rust nodes, state enum `Off=0 Up=1 Down=2 On=3`) and onto
   `RollershutterState` (`number` + `bank`) has to be pinned down in plan
   phase 0, together with fixing `decode_relais_state` (which currently
   reads only byte 0).
2. **Storm guard thresholds** (10 / 1 s) and **limits** (§8) are first
   guesses, to be checked against real installations.
