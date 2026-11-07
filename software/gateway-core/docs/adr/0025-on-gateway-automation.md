# ADR 0025: Automation on the gateway, from fixed building blocks

**Status:** Accepted. Replaces the generic if-then draft that was on the
branch `docs/automation-rules` (never merged).

## Context

Every automation ran in openHAB: a button press went CAN → gateway → MQTT →
hub → MQTT → gateway → CAN. With the network, the broker or the hub down,
no light switch worked. [ADR 0011](../../../../docs/adr/0011-gateway-mesh-planned.md)
already pointed the way: automation on the gateway, independent of MQTT.

What the house needs (`openhab_rules.js`, `smartlight.js`) is a handful of
patterns:
- buttons switching lights in levels, relays and roller shutters;
- motion lights with a timeout, a lockout and a manual mode;
- outdoor lights by the clock, the sun and the brightness.

A general rule language would need timers, a clock, dimming and light state
in its expressions. A draft without them couldn't express these patterns,
and one with them would be large for the gateway's RAM.

## Decision

**Fixed building blocks, configured with forms, not a rule language**
(`gateway_core::automation`). All of it is pure: callers pass in the time
and the inputs, and get back the CAN commands to send. It is host-tested,
including the openHAB lookups replayed.

- **Outputs**, named once and used by everything else:
  - a room *light* in levels: lamp groups across the lamp controllers,
    with a level table and the last level remembered;
  - a *dimmer*;
  - a *relay*;
  - a *shutter*: up/down/stop, hold, travel time, inverted wiring;
  - a *group* of outputs.
- **Switch configurations:** per wall switch, or several alike, what each
  button does on single, double and triple press, hold and release, with up
  to 4 actions. An action commands an output, enables or disables a motion
  light or schedule, or sets a night light.
- **Motion lights:** detectors and an output, a timeout and a lockout, and
  levels by the time of day. A light switched on by hand puts the motion
  light into a manual mode until the light is off again. Conditions are lux,
  a time window and the sun.
- **Schedules:** an output and up to 4 windows. A window starts at a clock
  time or an astro event and ends at the earliest of several ends; brightness
  can be a condition. Local time and sun times come from the location
  settings (`tz`, `sun`).
- **A master switch:** with it off, nothing switches by itself.

**Runtime** (`gateway-hardware/src/automation`):
- one task takes the CAN events, MQTT commands and web requests one after
  the other, ticks the timers and sends what the engine answers;
- the lamp controllers get at most one level change a second, the newest
  winning (`lamp_pacer`).

**Storage:**
- the configuration lives in its own `sequential-storage` map on the
  `storage` partition, one postcard item per output, switch configuration,
  button, motion light and schedule, behind a format version byte (2);
- RAM holds only what every event needs: the engine stays under 8 KiB
  (a test checks this) at the limits of 48 outputs, 32 switch configurations,
  12 motion lights and 8 schedules;
- level tables and button actions are read from flash when needed.

**Interfaces:**
- REST: `/api/v1/automation`, one JSON object per item
  ([`api-rest.md`](../api-rest.md#automation));
- MQTT: `canbus/output_state|output_command/<name>` and
  `canbus/automation_state|automation_command/<name>`
  ([`protocol-mqtt.md`](../protocol-mqtt.md));
- web UI: the page `/automation`, with forms, YAML editing, learning a
  button by pressing it, and export/import as a file assembled in the
  browser. The UI's requirements are in
  [`automation-ui-requirements.md`](../../../gateway-web/docs/automation-ui-requirements.md).

Everything house-specific (lamp masks, level tables, which button does
what) is configuration, built from openHAB's files by an import script. The
firmware knows no rooms.

## Consequences

- Lights, shutters and motion lights work without the network, the broker
  or a hub.
- Only what the blocks offer can be automated; a new pattern means a new
  block (or an option on one), not a rule.
- A changed item is loaded at once and the states of the others are kept.
  An item stored in an older format is dropped at boot.
- Not done yet:
  - Home Assistant entities for the outputs and the automation switches.
    Only the buttons are discovered (ADR 0022).
  - A guard against a block that fires in a loop.
  - An import script that writes format version 2 (it writes version 1).
