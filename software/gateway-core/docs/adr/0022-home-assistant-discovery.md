# ADR 0022: Home Assistant MQTT discovery from the gateway

**Status:** Accepted

## Context

Every CAN device had to be set up in Home Assistant by hand: a sensor per
reading, with the gateway's topic and a value template, and an automation
per button event. The gateway already knows everything that takes: the
devices with their names and types (since ADR 0021 straight from their
announcement), every sensor that has sent a reading, every button and
presence sensor that has sent an event, and the topics it publishes them
on.

There are two ways to bring this into Home Assistant:

- **Home Assistant's MQTT discovery.** Retained configuration messages under
  `homeassistant/…/config` describe entities and devices. Home Assistant
  picks them up by itself and needs nothing installed.
- **A custom integration** (Python, in Home Assistant). This means more
  freedom (its own config flow, services), but it is a second piece of
  software to write, install and keep working with every Home Assistant
  release.

## Decision

**MQTT discovery, published by the gateway** (`gateway-core/src/ha_discovery.rs`
for the messages, `gateway-hardware/src/ha.rs` for when to send them),
switched on per gateway (console `ha-discovery on`, `PATCH /api/v1/mqtt`
`{"home_assistant": true}`, the MQTT page). It is off by default: it writes
retained messages into the broker.

- **One Home Assistant device per CAN device** (`canbus_<key>`), with its
  name, type (model) and firmware version.
- **Sensors:** every reading seen becomes a `sensor` with device class,
  unit and `state_class: measurement`, on the gateway's own state topic.
  Brightness is `illuminance`, pressure `atmospheric_pressure`. A node's
  DS18B20s are told apart by their name, else their id.
- **Presence:** a `binary_sensor` (motion), `ON` unless the body says
  `release`.
- **Buttons:** five device triggers per button seen: single, double, triple
  (`button_short/double/triple_press`), hold (`button_long_press`, fires
  every 500 ms while held) and the release after a hold
  (`button_long_release`). The value template drops the count, so
  automations trigger on button and event.
- **When:**
  - something is new or changed: a device, its name or version, a sensor,
    a sensor's name, a button, a presence sensor;
  - after every connection to the primary broker;
  - when Home Assistant reports `online` on `homeassistant/status`.

  What was published is remembered as a hash per topic, so an unchanged
  configuration isn't sent again.
- **Transport:** configurations are up to 768 bytes, larger than the
  publish queue's messages. They go one at a time through a channel of
  their own and are published retained.

Not described yet: relays, rollershutters, lamps and the heating controller.
Their state topics carry a raw state byte, or have no state at all (lamps).
They need a decision on how they map to switches, covers and lights first.

## Consequences

- With it on, new devices appear in Home Assistant as soon as the gateway
  has heard from them, named as on the bus. A renamed device or sensor gets
  its configuration again, with the new topic.
- Entity ids are stable (`canbus_<key>_<kind>_<sub_id>`), so Home
  Assistant keeps history and customisations across restarts and renames.
- Switching it off stops new configurations. What was published stays
  (retained) until it is deleted in Home Assistant or cleared on the
  broker.
- About 2 KB more static RAM (the published hashes, the seen buttons and
  presence sensors, one configuration in flight).
