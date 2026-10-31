# REST API

The gateway's web server offers this API under `/api/v1` and, at the other
paths, the browser UI that uses it (`software/gateway-web`). It is served
over HTTPS on port 443; port 80 answers `308 Permanent Redirect` to the
same URL on `https://` (the method and body are kept). `web-https off` on
the console serves everything over plain HTTP on port 80 instead
([ADR 0018](adr/0018-https-certificate-management.md)). Why
it looks like this: [ADR 0014](adr/0014-rest-api-by-resource.md). It replaces
the original's `/state.json`, `/control.json` and `/bridge_config.json`.

## Conventions

- **Authentication:** HTTP Basic Auth on every route, the UI files included
  ([ADR 0008](adr/0008-mandatory-web-authentication.md)). The credentials
  are the `web_username`/`web_password` settings
  (`CAN2MQTTSETUP`/`Can2MqttPass` until changed). Without them: `401`.
- **Bodies** are JSON objects with typed fields (numbers are numbers), sent
  with `Content-Type: application/json`. Fields that are not part of a
  request are rejected, not ignored. In a `PATCH` every field is optional;
  only the given ones change.
- **Errors:** the status code says what kind, the body is
  `{"error": "<code>", "message": "<text>"}`: `400 invalid` for a value that
  isn't acceptable, `404 not_found`, `409 busy`, `413 size`, `422 verify`,
  `500` for storage/update failures.
- **Secrets are write-only.** Passwords are accepted, never returned; a
  `GET` says `password_set` instead.
- **Commands** (everything that puts frames on the CAN bus) answer
  `202 Accepted` with `{"sent": <frames>}`. The bus has no
  acknowledgement; the result shows in the device's state.
- **`:device`** is the name of a device (its custom string, e.g.
  `HEIZUNG1`) or its key `0x<hex>`, as in the MQTT topics. A device that
  has not been seen on the bus is `404`.
- **`:slot`** is `primary` or `secondary`, see
  [`protocol-mqtt.md`](protocol-mqtt.md).
- **Capacity:** the server holds two HTTPS connections at a time and serves
  them one after the other (one TLS session at a time); more are refused,
  a client retries.

## System and bus

| Route | |
|---|---|
| `GET /system` | `firmware_version`, `uptime_s`, `heap_used`, `heap_free`, `stack_size`, `stack_untouched` (main stack bytes never used since boot), `network` (`null` without Ethernet): `link_up`, `mac`, `ipv4` (with prefix length) and `ipv4_gateway` or `null`, `ipv6` (`address` with prefix length and `origin`: `link-local`/`SLAAC`/`DHCPv6`; link-local first), `ipv6_gateway` or `null`, `dns_servers` (`address`, `origin`: `DHCPv4`/`RDNSS`/`DHCPv6`), `ipv6_managed`/`ipv6_other` (the routers' M/O flags), `dhcpv6_mode` (`off`/`stateless`/`stateful`) |
| `POST /system/restart` | restarts the gateway a moment after answering (`202`) |
| `POST /bus/ping`, `/bus/refresh`, `/bus/restart` | to every device |
| `PUT /bus/silence` `{"on": true}` | tells every device to be quiet / talk again |
| `POST /device-types/:type/ping`, `/refresh`, `/restart` | to every device of one type (`:type` is the number, e.g. `9` for the heating controller) |

## Configuration

| Route | |
|---|---|
| `GET /config` | `hostname`, `can_bitrate` (`b22_222`, `b25`, `b50`, `b100`), `update_delay_ms`, `web_username`, `web_password_set` |
| `PATCH /config` | any of those (+ `web_password`, at least 8 characters). Answers `{"restart_required": true}` when the hostname or bitrate changed: they are read at boot |

| Route | |
|---|---|
| `GET /mqtt` | `{"primary": {...}, "secondary": {...}}` |
| `GET /mqtt/:slot` | `slot`, `enabled`, `uri`, `user`, `password_set`, `tls_insecure`, `certificate_pinned`, `connected` |
| `PATCH /mqtt/:slot` | `enabled`, `uri` (`mqtt://host[:1883]` or `mqtts://host[:8883]`), `user`, `password` (8+ characters, or `""`), `tls_insecure`. The broker reconnects with the new settings; the answer is the new state |
| `POST /mqtt/:slot/trust` | the next connection pins the certificate the broker presents (`202`) |
| `GET /mqtt/:slot/certificate` | `{"pinned": true, "sha256": "<hex>"}`, the fingerprint `openssl x509 -noout -fingerprint -sha256` shows |
| `DELETE /mqtt/:slot/certificate` | forgets the pinned certificate (`204`) |

## Devices

| Route | |
|---|---|
| `GET /devices` | `{"total", "offset", "devices": [...]}`, eight per page: `?offset=` continues, `?device_type=` filters |
| `GET /devices/:device` | `key`, `name`, `device_type`, `type_name`, `device_id`, `version`, `last_seen_minutes_ago`, `uptime`, `hwrev`, `baudrate`, `extension_mode`, `relais_mode`, `state`, `error`, `uid0`, `uid1` |
| `PATCH /devices/:device` | `custom_string` (8 chars), `hwrev`, `baudrate`, `extension_mode`, `relais_mode` (`OFF`, `RELAIS`, `SWROLLERSHUTTER`, `HWROLLERSHUTTER`; `extension_mode` also `BUTTONS`, `PWM`, `SENSORS`, `LEGACY_SENSORS`), `device_id` + `device_type` (only together), `uid0`/`uid1` (`0x…`, with those) |
| `POST /devices/:device/ping`, `/refresh`, `/restart`, `/legacy-mode` | `legacy-mode` restarts it into update mode, for old STM32 nodes |
| `PUT /devices/:device/silence` `{"on": true}` | |
| `PUT /devices/:device/relays/:num` | `{"state": "on"\|"off"\|"up"\|"down", "duration_ms": 0, "bank": 0}`; after `duration_ms` (up to 24 bits, `0` = stay) it switches back |
| `PUT /devices/:device/rollershutters/:num` | the same body |
| `PUT /devices/:device/lamps/:bank` | `{"value": 0-255, "bitmask": <24 bits>}` (always the full 8-byte form with the bank) |
| `PUT /devices/:device/heating/target` | `{"flow_c": 45.5, "tank": true}` |
| `PUT /devices/:device/heating/energy` | `{"power_w": 6200, "rod_enabled": true}` |
| `PUT /devices/:device/heating/emergency` | `{"stage": 0-3, "tank_c": 55}` |
| `PUT /devices/:device/heating/manual` | commissioning: `{"enabled": true}` takes the relays 6-10 (valves, mixer, pump) from the controller (they go off), `{"enabled": true, "relay": 8, "on": true}` switches one (6-10; the mixer directions exclude each other), `{"enabled": false}` hands them back. There is no time limit: it stays on until it is ended (or the controller restarts); while it is, the controller drops `target`/`energy`/`emergency` commands, from MQTT too, and those routes answer `409 manual_mode`. The rod can't be reached. `GET` of the same path asks the controller (R frame) and answers `{"enabled": true, "relays": [8, 10]}`, or `504 no_answer` if it doesn't answer in time. Sent as a `HeatingCommand`, [`protocol-can.md`](../../../docs/protocol-can.md) |

The heating bodies mean what the MQTT topics' do
([`protocol-mqtt.md`](protocol-mqtt.md)); the controller counts each as gone
30 s after it last arrived.

## Sensors

| Route | |
|---|---|
| `GET /sensors` | every sensor seen on the bus, its latest reading and what its node says about it, sixteen per page: `{"total", "offset", "sensors": [{"device", "device_key", "kind", "unit", "sensor_id", "named_by_node", "slot", "name", "role", "lost", "value", "age_s"}]}`. `kind` is `temperature`, `humidity`, `pressure`, `co2`, `voc`, `air_quality` or `brightness` (lux, one per device, `sensor_id` 0); `sensor_id` is the sensor's own id (the 1-Wire address of a DS18B20, `0x…680000xx` for a light switch's BME680). `named_by_node` says whether the node has a slot and name for it (only DS18B20s; the others are known by their device). `slot`, `name` and `role` (`none`, `tank0` … `tank6`, `flow`) are `null` until the node has told (the gateway asks for them the first time it sees such a sensor). `?offset=` continues, `?device=` (name or `0x…` key) filters |
| `PUT /devices/:device/sensors/:slot` | `{"name": "TANK-O1"}` (1-7 characters of `A-Z a-z 0-9 - _`, unique on the node) and/or `{"role": "tank2"}` (`none`, `tank0` … `tank6`, `flow`; the heating controller only). The slot has to be one the node reported (`409 unknown_slot` otherwise — refresh first); the gateway sends the sensor's serial along as a guard, so that a slot that moved is refused by the node instead of renaming or assigning the wrong sensor |
| `POST /devices/:device/sensors/refresh` | asks the node for its list again (R frame); the answer shows in `GET /sensors` a moment later |
| `POST /devices/:device/sensors/forget` | the node forgets the sensors that are lost |

Slots are the node's own numbers, temporary: they change with a restart or
when sensors come and go. Names stay with the sensor. The sensor lists are kept
in RAM (48 sensors; a full list drops the one that reported longest ago), so
they are empty after a restart until the sensors report again.

## Firmware

The image is the raw request body (`Content-Type: application/octet-stream`),
`Content-Length` its size, `?crc32=<decimal>` its CRC-32. Only one update
runs at a time (`409 busy`). A browser UI computes the CRC with the `crc-32`
package before sending.

| Route | |
|---|---|
| `PUT /firmware/gateway?crc32=…` | the gateway's own firmware (up to 7.75 MiB, the size of an OTA slot). Answers `{"bytes", "restarting": true}` once the image verified, then restarts into it; `422 verify` if size or CRC are wrong |
| `PUT /devices/:device/firmware?crc32=…` | a CAN node (up to 1 MiB), in acknowledged blocks (ADR 0017); a node that does not know the block protocol yet is sent the old way once. Takes minutes, the bus sets the pace |
| `PUT /device-types/:type/firmware?crc32=…` | every node of a type at once, with the old streaming protocol (no acknowledgement). The web UI sends an image to the devices of a type one after the other through the per-device route instead |
| `GET /firmware/job` | `state` (`idle`, `running`, `done`, `failed`), `target`, `total`, `sent` (for a device: the bytes it has acknowledged), `protocol` (`blocks`, `legacy`), `retries` (blocks sent again), `error` |

## HTTPS certificate

One device key (ECDSA P-256, never exported) and the configured FQDN; the
certificate is self-signed, uploaded (from the gateway's CSR) or enrolled
over EST. A new certificate replaces the active one only if it parses,
carries the device key, names the FQDN and isn't expired; without a usable
certificate the gateway makes a self-signed one. Certificates and keys are
uploaded as the raw request body (PEM, or DER for a single certificate),
up to 10 KiB (a stored chain holds at most 6 KiB of DER); `503` with
`no_memory` if the heap is short right then. What an EST server has to
support:
[`est-server.md`](est-server.md).

| Route | |
|---|---|
| `GET /tls` | `fqdn` (as set; empty: the hostname is used), `effective_fqdn`, `mode` (`self_signed`, `manual`, `est`), `validity_years`, `https`, `time_known`, `ntp_server` (as set; empty: automatic), `ntp_server_in_use` (the set one, else the one DHCPv6 names, else `pool.ntp.org`), `certificate` (`subject`, `issuer`, `not_before`, `not_after`, `dns_names`, `fingerprint_sha256`, `status`: `valid`/`expires_soon`/`expired`/`not_yet_valid`/`fqdn_mismatch`/`time_unknown`, `origin`: `self_signed`/`self_signed_fallback`/`manual`/`est`, `count`: certificates in the chain), `trust_anchor` (the same facts, `count` CA certificates) or `null`, `est` (`url`, `auto_renew`, `identity`, `last_result`, `last_status`, `last_attempt`, `failures`, `next_attempt_in_s`) |
| `PATCH /tls` | `fqdn`, `mode`, `validity_years` (1-30), `est_url` (`https://host[:port][/path]`), `est_auto_renew`, `ntp_server` (empty: automatic). A new FQDN or mode `self_signed` makes a self-signed certificate right away (in that mode); mode `est` starts an enrollment. `204` |
| `POST /tls/self-signed` | `{"validity_years"}` (optional): a new self-signed certificate. `204` |
| `GET /tls/csr` | the PKCS#10 request for the device key and the FQDN, `application/pkcs10` PEM |
| `PUT /tls/certificate` | the signed certificate with its chain (switches the mode to `manual`); `422` with `key_mismatch`, `fqdn_mismatch`, `expired`, `not_yet_valid`, `unparsable` or `chain_too_long` if it isn't usable. `204` |
| `PUT /tls/trust-anchor`, `DELETE` | the CA certificate(s) the EST server must chain to. `204` |
| `PUT /tls/est/identity`, `DELETE` | the EST device identity: one PEM body with the certificate(s) and the private key; the key can't be read back. `204` |
| `POST /tls/est/enroll` | enroll (or renew) now; `409 not_est_mode` outside EST mode. `202` |

## The browser UI

`GET /`, `/sensors`, `/mqtt`, `/gateway`, `/https`, `/devices/:device` answer with `index.html`,
`/assets/app.js` and `/assets/style.css` with the rest, all gzip'd and part
of the firmware (`gateway-hardware/src/web/assets.rs`). Edit the sources in
`software/gateway-web/src` and run `npm run build` there: the result in
`dist/` is checked in, so building the firmware needs no node.
`npm run dev` serves the sources on `localhost:8080` and forwards `/api` to a
gateway (`GATEWAY=<address>`).
