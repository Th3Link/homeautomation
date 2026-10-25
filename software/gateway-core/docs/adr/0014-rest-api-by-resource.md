# ADR 0014: A REST API by resource instead of `/state.json` and `/control.json`

**Status:** Accepted

## Context

The original web server had three endpoints that did everything:
`GET /state.json` (every value of the gateway in one document, passwords
included), `POST /control.json` (one RPC: a `command` string, a `unit` and a
`commandId` selecting the target, and loose fields, some of them numbers
sent as strings) and `PUT /bridge_config.json`. Each verb was found by
string comparison inside one handler, the target selection was its own mini
protocol, and a change to anything meant reading all of it. The browser UI
grew the same way and was hard to debug and extend.

## Decision

The gateway exposes resources in the path and uses the HTTP verbs for the
intent, under `/api/v1`: `/system`, `/config`, `/mqtt/{primary,secondary}`,
`/devices`, `/devices/:device/...` (relays, rollershutters, lamps, heating,
ping, ...), `/device-types/:type/...`, `/bus/...` and `/firmware/...`.
[`api-rest.md`](../api-rest.md) lists them. In `gateway-hardware` every
route is one small handler registered with picoserve's router, with typed
path parameters (`:device` is the name or `0x…` key as in the MQTT topics,
`:slot` is `primary` or `secondary`).

- Bodies are small flat JSON objects with typed fields, errors are
  `{"error","message"}` with a fitting status, and unknown fields or
  values are rejected, not ignored.
- Passwords are write-only: `GET` never returns them
  (`password_set: true`), unlike `/state.json`.
- The firmware image is the raw request body of a `PUT`
  (`?crc32=` and `Content-Length` up front), not multipart or a
  prepare/data/complete sequence.
- The `can_selected` unit (a list of devices in one request) is dropped:
  the client sends one request per device.
- Authentication stays as ADR 0008 says: Basic Auth on every route and
  the UI files too.
- The browser UI (`software/gateway-web`) is a client of this API and is
  routed by path as well, so that the same paths work as its page URLs.

## Consequences

- The old `state.json`/`control.json`/`bridge_config.json` clients do not
  work against the new firmware. The only client was the bundled UI.
- JSON responses are rendered into one shared buffer before they are
  sent (`gateway-hardware/src/web/json.rs`), so the largest response is
  bounded by that buffer (3 KiB; the device list is paged for that).
- Which connection serves a request is limited by the number of web
  tasks (3): a fourth simultaneous connection is refused, so the UI
  retries refused requests.
