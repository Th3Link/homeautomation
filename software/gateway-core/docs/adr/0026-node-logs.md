# ADR 0026: The nodes' logs over CAN, and all log lines on MQTT

**Status:** Accepted.

## Context

The gateway keeps its latest console lines and shows them in the web UI
(`/logs`); that is how most problems were found. The nodes print to their
serial console only: to see what a node did, someone had to go to it with
a cable. The CAN protocol had a message type for it, `LogDownload` (155),
that nothing used. And none of the log lines reached MQTT, where they could
be kept and searched.

## Decision

**On the node:** every `console_log!` line also goes into a ring of 4 KiB
(`cancomponents-hardware/src/node_log.rs`), `[uptime] text` like the
gateway's. When the ring is full the oldest lines go; the node counts the
bytes lost that way.

**Over CAN** (`LogDownload`, `common_core::log_download`,
[`protocol-can.md`](../../../../docs/protocol-can.md#node-logs-logdownload)):

- The gateway asks with an **R frame**. The node sends **blocks** of whole
  lines, up to 224 bytes: up to 32 data frames of 7 bytes, then a block end
  with the block's number, length and CRC-16 and how much is still waiting.
- The gateway **acknowledges** a block that arrived whole. Only then does
  the node **drop it** from the ring and send the next. A block that isn't
  acknowledged goes again after 500 ms, five times, then the node stops and
  keeps it. A block whose ack was lost comes again with the same number;
  the gateway acknowledges it again and doesn't take it twice. So every line
  is downloaded **once**, none lost on the way.
- At the end the node says so, with the bytes overwritten since the last
  download.
- The pace is the gateway's acks: between two blocks other frames go
  through. 4 KiB take about 2-3 s at 50 kbit/s.

**On the gateway** (`gateway-hardware/src/node_log.rs`):

- Every 5 minutes (`node-log-interval`, 0: never) it downloads the logs of
  the nodes in the device list, one after the other, and publishes their
  lines on `canbus/device_log/<device>`, one line a message. Only while the
  primary broker is connected: without it the lines stay on the node.
  A node that doesn't answer (an old one) is left out for a day.
- Its own lines go to `canbus/gateway/log` every 10 s, those since the
  last time. Not each line at once: publishing logs lines itself.
- `log-mqtt off` (console, or the log page) stops both.
- The web UI downloads one node's log on the device page
  (`POST`/`GET /devices/:device/log`); the lines go to MQTT too.

## Consequences

- 4 KiB of RAM on each node; on the gateway a 40-frame queue and, while a
  download runs, its state on the heap.
- Since a node drops what was downloaded, the device page shows only what
  came since the last download, at most 5 minutes with MQTT on. The broker is
  the archive; with MQTT off, the device page is the only reader.
- The STM32 nodes don't know the message; they don't answer and are asked
  again a day later.
- A node with a full ring between two downloads loses its oldest lines;
  the gateway publishes how many bytes that were.
