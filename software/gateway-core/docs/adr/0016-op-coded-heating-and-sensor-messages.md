# ADR 0016: Op-coded heating and sensor messages, state by R frame

**Status:** Accepted

## Context

The heating controller had grown to five CAN message types (target, energy,
emergency, manual mode, manual state), the last one pushed by the node every
few seconds, and its temperature sensors could only be assigned by hex id on
the node's console. Message types and bus time are worth saving on a 50 kbit/s
bus, and sensors had to be nameable and assignable from the web UI and over
CAN, which needs a way to call a sensor that fits next to other fields (the
48-bit serial does not).

## Decision

- **One command message.** `HeatingCommand` (165) carries every input to the
  controller: an operation code in bits 0-2 of the first byte, the operation's
  flags above it, the values after it, all in one frame of at most 5 bytes.
- **State by R frame.** `HeatingState` (166) is *read*: an R frame on it is
  answered with a data frame, the same pattern as `Uptime` and the UIDs. The
  node does not send it by itself, there is no heartbeat; the gateway asks
  when somebody wants to know (the web UI polls while a page is open).
- **Sensor management in one message.** `SensorConfig` (167) has the slot in
  bits 0-4 and the operation in bits 5-7 of byte 0, which leaves 7 bytes for
  a name exactly. Sensors are called by **slot**, their number in discovery
  order since the node started — local to the node and temporary. Names
  (7 characters, `A-Z a-z 0-9 - _`) stay with the sensor, kept by serial;
  roles stay stored as they were (the low 32 bits of the serial).
  A role assignment carries a **guard**, the first four serial bytes the sender
  believes the slot has, and the node refuses it if they differ, so a slot that
  moved can never assign the tank sensor of the heating to the wrong sensor.
- **New numbers.** 160-164 are retired and never reused. A node that was not
  updated yet would otherwise read a new frame as a flow setpoint; with new
  numbers a version mismatch only means that inputs are missing, which the
  controller already treats as the safe state (rod off, circuit direct).
- **Names in MQTT topics** the way custom strings are: the name instead of
  `0x<id>` when there is one, the hex id otherwise.

## Consequences

- Gateway and node have to be updated, but in either order and without a
  dangerous intermediate state.
- The gateway asks for the sensor list by itself the first time it sees a
  sensor of a node, so names reach the MQTT topics without anyone doing
  anything; the first readings after a restart may still carry the hex id.
- Reading the state costs a round trip (a few milliseconds on the bus); a
  controller that does not answer (an old firmware) shows as `504` in the web
  API and its commands are sent anyway.
- The command encoding rejects values it can't hold (a rod stage above 3,
  a manual relay outside 6-10) instead of truncating them.
