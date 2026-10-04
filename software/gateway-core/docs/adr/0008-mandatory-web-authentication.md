# ADR 0008: Require Basic Auth on every web endpoint unconditionally

**Status:** Accepted

## Context

The original C++ web server has two auth gaps, found while porting
`Web.cpp`/`http_handler.cpp`:

- `/state.json` (full status dump: MQTT config including credential
  fields, device list, counters) and `/update/data` (the firmware-upload
  endpoint) have **no authentication check at all**, regardless of
  configuration.
- `/control.json` and the rest do check Basic Auth, but only when a web
  username happens to be configured (`username().length() > 0`) — an
  out-of-the-box or reset device with no username set silently serves
  everything, including accepting arbitrary firmware uploads, to anyone
  on the network.

## Decision

Every web endpoint requires Basic Auth unconditionally in the rewrite —
including `/state.json` and `/update/data`, and with no username-empty
bypass. This is a deliberate behavioral difference from the original,
made without asking for sign-off on this specific point because an
unauthenticated firmware-upload endpoint is a real vulnerability, not a
faithfully-reproducible quirk.

## Consequences

- A freshly-flashed or factory-reset device is unreachable over the web
  UI until its Basic Auth credentials are set — this differs from the
  original, where a device with no username configured yet was fully
  open. The default credentials
  (`CAN2MQTTSETUP`/`Can2MqttPass`, matching the original's own hardcoded
  setup values) still apply out of the box, so this doesn't
  actually block first-time setup, it just means setup always requires
  knowing those defaults rather than sometimes requiring no credentials
  at all.
- This is the one place in the port where "exact same functionality" was
  deliberately not the goal — everywhere else, quirks (even unflattering
  ones) are reproduced as-is rather than silently "fixed", precisely so
  behavior stays predictable relative to the original. This ADR exists to
  make the one exception explicit and easy to find.
