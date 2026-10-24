# REST API

The gateway's web server (port 80) offers this API under `/api/v1` and, at
the other paths, the browser UI that uses it (`software/gateway-web`). Why
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
- **Capacity:** the server handles three connections at a time and refuses
  more; a client retries.

## System and bus

| Route | |
|---|---|
| `GET /system` | `firmware_version`, `uptime_s`, `heap_used`, `heap_free` |
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
| `PUT /devices/:device/heating/manual` | commissioning: `{"enabled": true}` takes the relays 6-10 (valves, mixer, pump) from the controller (they go off), `{"enabled": true, "relay": 8, "on": true}` switches one (6-10; the mixer directions exclude each other), `{"enabled": false}` hands them back. Ends by itself after 10 minutes without a switch. The rod can't be reached. Sent as `HeatingManual`, [`protocol-can.md`](../../../docs/protocol-can.md) |

The heating bodies mean what the MQTT topics' do
([`protocol-mqtt.md`](protocol-mqtt.md)); the controller counts each as gone
30 s after it last arrived.

## Sensors

| Route | |
|---|---|
| `GET /sensors` | every sensor seen on the bus and its latest reading, sixteen per page: `{"total", "offset", "sensors": [{"device", "device_key", "kind", "unit", "sensor_id", "value", "age_s"}]}`. `kind` is `temperature`, `humidity`, `pressure`, `co2`, `voc` or `air_quality`; `sensor_id` is the sensor's own id (the 1-Wire address of a DS18B20). `?offset=` continues, `?device=` (name or `0x…` key) filters |

The list is kept in RAM (48 sensors; a full list drops the one that reported longest ago), so it is empty after a restart until the sensors report again.

## Firmware

The image is the raw request body (`Content-Type: application/octet-stream`),
`Content-Length` its size, `?crc32=<decimal>` its CRC-32. Only one update
runs at a time (`409 busy`). A browser UI computes the CRC with the `crc-32`
package before sending.

| Route | |
|---|---|
| `PUT /firmware/gateway?crc32=…` | the gateway's own firmware (up to 7.75 MiB, the size of an OTA slot). Answers `{"bytes", "restarting": true}` once the image verified, then restarts into it; `422 verify` if size or CRC are wrong |
| `PUT /devices/:device/firmware?crc32=…` | a CAN node, over CAN-OTA (up to 1 MiB); takes minutes, the bus sets the pace |
| `PUT /device-types/:type/firmware?crc32=…` | every node of a type |
| `GET /firmware/job` | `state` (`idle`, `running`, `done`, `failed`), `target`, `total`, `sent`, `error` |

## The browser UI

`GET /`, `/sensors`, `/mqtt`, `/gateway`, `/devices/:device` answer with `index.html`,
`/assets/app.js` and `/assets/style.css` with the rest, all gzip'd and part
of the firmware (`gateway-hardware/src/web/assets.rs`). Edit the sources in
`software/gateway-web/src` and run `npm run build` there: the result in
`dist/` is checked in, so building the firmware needs no node.
`npm run dev` serves the sources on `localhost:8080` and forwards `/api` to a
gateway (`GATEWAY=<address>`).
