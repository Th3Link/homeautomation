# RAM on the gateway (ESP32)

For the shipping memory layout and workload measurements, see [MQTT, HTTPS and crypto storage](#mqtt-https-and-crypto-storage). The [baseline by bank](#baseline-by-bank) records the release before these changes.

## First reduction in HTTPS, logging and OTA (`a9e1b19`)

The gateway serves the UI and API on port 443. Removing the port 80 redirect eliminates a 7,688 B long-lived boxed future in the former default mode. A saved plain-HTTP setting from older firmware has no effect after upgrading. The five HTTPS sockets still share one active TLS session.

| Storage | Location | Allocated bytes |
|---|---|---:|
| Node log download text, shared by successive requests | RTC SLOW, `.rtc_slow.bss` | 8,192 |
| CAN log frame queue | RTC FAST, `.rtc_fast.data` | 672 |
| Gateway log batch sent over MQTT, including its lock | RTC FAST, `.rtc_fast.data` | 1,044 |
| Shared firmware upload chunk for gateway and node updates, including its lock | RTC FAST, `.rtc_fast.data` | 4,112 |
| Gateway log ring with cursor and lock | SRAM2, `.data` | 6,156 |

The two log workers use ordinary Embassy task storage in SRAM2. A download resets its length and status under the same lock used by readers. Only its valid UTF-8 prefix is exposed. At this step RTC FAST had 2,364 B left. Only one upload runs at a time, so all three upload routes use the same chunk. The flash driver still has its own sector-sized stack buffer. The bank figures and sizing experiment below describe the earlier build used to select these changes.

One local `memory-profile` release run repeated the baseline QEMU `combined` scenario with two gateways, active CAN nodes, Mosquitto and a concurrent HTTPS automation read. Gateway A reached 62,436 B peak heap and ended at 21,808 B. The baseline reached 72,724 B and ended at 32,000 B. The unchanged 96,256 B heap reservation therefore had 10,288 B more headroom at the measured peak. Stack margin in this workload was 23,620 B (baseline 26,316 B). A separate ten-second HTTPS `/system` capture completed on one boot with 15,508 B untouched. At this step the full automation-edit workload still hit the guard in `put_button` during output lookup.

Both HTTPS OTA scenarios completed their bad-CRC checks and the subsequent version switch from `qemu-v1` to `qemu-v2`. The gateway update preserved the configuration across reboot and reached 65,436 B peak heap with 15,508 B of stack untouched. The addressed CAN update left the other node on `qemu-v1` and reached 65,300 B peak gateway heap with 15,352 B of stack untouched. These are QEMU measurements.

## MQTT, HTTPS and crypto storage

| Storage in the release build | Placement | Bytes |
|---|---|---:|
| MQTT TCP RX / TX, per broker | SRAM1 ROM boot areas | 1,536 / 1,024 |
| MQTT TLS records, per broker | Same allocation as its TCP windows | 8,192 / 1,024 |
| HTTPS TCP RX / TX, five listeners | One pair in each ROM boot area, three in upper SRAM1 | 5 × 2,560 |
| MQTT application publication queue, primary broker | SRAM2 | 4,896 |
| Gateway OTA partial-sector scratch | Upper SRAM1 | 4,096 |
| Shared certificate, JSON, firmware upload and NTS-KE workspace | Upper SRAM1 | 14,336 |
| Installed trust anchors, shared across verified clients | SRAM2 | 6,144 DER payload |
| MbedTLS allocator for HTTPS, EST and certificate parsing | Upper SRAM1 | 53,248 |
| DHCPv6 client, UDP packet metadata and buffers, outgoing message | Upper SRAM1 | 2,664 |
| General allocator, including Rust RSA and request futures | Upper SRAM1 | 16,384 |
| Certificate status or MQTT certificate capture | RTC FAST | One shared slot |

The secondary broker subscribes, accepts commands and sends MQTT protocol acknowledgements. Application publications go to the primary broker. Removing the secondary queue saves 4,896 B. The two MQTT TCP window pairs save 3,072 B and the five HTTPS pairs save another 2,560 B. Together these remove **10,528 B of application storage**. Both MQTT sessions keep their own TLS records and a 2,048 B maximum MQTT packet. Moving the five HTTPS listeners into static storage also removes their lifetime heap allocations.

Aligned, complete OTA sectors use the incoming chunk directly through the flash driver's 512 B DRAM bounce buffer. Partial sectors preserve adjacent data in one static 4 KiB workspace, including writes to boot metadata. NTS key exchange borrows the HTTPS buffers for TCP, its outgoing TLS records and its response. EST borrows them as its TCP buffers. The Home Assistant task streams one device or sensor at a time and releases its locks before publishing. The queue takes ownership of the discovery payload.

The TLS pool is installed before the first MbedTLS allocation. Each allocation has 16-byte alignment, zeroed payload and an independent lifetime. `GET /api/v1/system` and memory profiles expose its capacity, usage, peak and failure count separately from the general heap. The two pools reserve 69,632 B combined. Pool and general-heap peaks can occur at different times. RTC FAST has 2,344 B free and the node log occupies RTC SLOW.

### DHCPv6 workspace

The DHCPv6 task initializes one 2,664 B workspace in upper SRAM1 whenever it starts. The workspace holds the client state, two receive and two transmit metadata entries, the 768 B receive buffer, the 384 B transmit buffer and the 384 B outgoing message. It stays reserved when DHCPv6 is off. The task runs on the Embassy executor and uses no general heap for this storage.

| Bytes | Before (`cc03d13`) | Static DHCPv6 |
|---|---:|---:|
| General heap reservation in SRAM1 | 34,816 | 31,744 |
| DHCPv6 workspace in SRAM1 | — | 2,664 |
| DHCPv6 task pool in SRAM2 | 64 | 184 |
| General heap at the end of the QEMU idle scenario | 3,224 | 424 |
| General heap peak in the combined CAN/MQTT/HTTPS scenario | 9,452 | 6,652 |

Upper SRAM1 gains 408 B after accounting for the new workspace. The larger task pool costs 120 B of SRAM2 stack budget, leaving a **net saving of 288 B** across those banks. The 2,800 B drop in live heap use measures the removal of the DHCPv6 allocation. The idle heap peak includes startup work and reached 3,384 B on both builds.

### Automation configuration reload

When the gateway starts or an edit changes its configuration, the automation task rebuilds the engine from flash. Its 3,384 B snapshot preserves output levels and shutter motion by name and kind, plus eligible motion-light and schedule state. The snapshot lives in the static Embassy task in SRAM2 across flash reads. At startup the task scans the flash store in its final location under the store lock. This removes a temporary copy from the task frame.

| Bytes | Before (`2f2dda8`) | After |
|---|---:|---:|
| Automation task pool in SRAM2, release ELF | 7,520 | 6,096 |
| Main stack budget, release ELF | 49,212 | 50,636 |
| General heap peak, fresh boot through idle | 3,384 | 424 |
| General heap used at steady idle | 424 | 424 |
| General heap peak, HTTPS automation edits | 9,968 | 8,792 |
| Untouched stack immediately after saving a button, profiled build | 9,364 | 10,788 |
| Untouched stack after the full edit sequence, profiled build | 9,156 | 10,372–10,580 |

Both builds at this step ran the same QEMU `idle` and `https-automation` workloads twice. The 1,424 B reduction in static SRAM2 gives the main stack 1,424 B more headroom. The general heap still reserves 31 KiB in SRAM1. The reload no longer allocates a snapshot from it.

### SNTP and DNS

The time task polls plain SNTP directly. Its two 128 B UDP buffers and packet metadata are live only while trying the resolved addresses, so their task storage can be reused during NTS. A single 48 B packet holds the request and then the reply. Each DNS caller polls the network stack directly, including both MQTT brokers, EST and NTS. Independent lookups can still run concurrently.

| Bytes | Before (`393e746`) | After |
|---|---:|---:|
| Time task pool in SRAM2, production ELF | 1,552 | 1,120 |
| Total static SRAM2, production ELF | 145,972 | 145,540 |
| Main stack budget, production ELF | 50,636 | 51,068 |
| General heap used at steady idle, pending time-server lookup | 424 | 0 |
| General heap peak from boot through idle | 424 | 388 |
| General heap peak, combined CAN/MQTT/HTTPS, gateway A | 6,652 | 6,228 |
| General heap peak, HTTPS automation | 8,792 | 8,368 |

The release reserves 432 B less SRAM2 and the flash image grows by 108 B. The general heap still reserves 31 KiB in SRAM1 for other workloads. Two QEMU runs per build gave the same heap values. To isolate the lookup, run `memory_profile.py qemu` with `--scenario sntp-dns`. The functional test `tests/test_sntp.py` uses QEMU and a local UDP peer on loopback port 123 to check that the gateway retries after an unsynchronised response.

### CA bundle and EST identity import

The HTTPS upload handler already owns the request body. For a CA bundle or
an EST identity, it decodes PEM certificates into the front of that buffer.
Concatenated DER is already in that layout. The gateway validates each
certificate and writes the populated slice to flash in input order. This
removes a second general-heap allocation as large as the decoded chain
(up to 6 KiB for trust anchors or 4 KiB for an identity). The identity's
private key still needs a separate temporary buffer during decoding.

The certificate status endpoint fills one result object and releases each
raw flash blob before reading the next. This matters when the browser
reloads the certificate details after a successful upload.

| General heap high-water mark, bytes | Before (`6c7e7d6`) | After |
|---|---:|---:|
| First PEM upload of six CA certificates (8,020 B PEM, 5,677 B DER) | 20,732 | 15,052 |
| Whole CA import scenario, including status and rejected uploads | 28,060–28,064 | 24,016 |
| First PEM upload of an EST identity (2,896 B DER plus RSA key) | 18,332 | 15,436 |
| Whole EST identity import scenario (2,896 B DER plus RSA key) | 25,276 | 21,232 |
| HTTPS server certificate rotation and reconnect | 22,380–22,384 | 18,724–18,728 |

Run `https-trust-import` and `https-identity-import` separately in QEMU to
reproduce the upload and whole-scenario peaks. Each scenario ran twice per
build, starting from a fresh virtual installation. The general heap still
reserves 31,744 B of SRAM1 and MbedTLS reserves 49,152 B. The production
ELF uses 145,540 B of static SRAM2 and leaves 51,068 B for the stack on
both builds. The flash image shrinks by 348 B. A production QEMU run left
11,292 B of stack untouched immediately after saving an automation button
and 11,084 B after the edit sequence on both builds.

### Shared certificate storage and the TLS pool

Certificate uploads, JSON serialization, EST requests and replies, NTS key exchange and firmware uploads borrow the same 14 KiB buffer in upper SRAM1. Each user retains its own protocol limit. A certificate upload accepts up to 10 KiB and can decode an identity key beside the request. The key buffer is wiped when its guard drops. Firmware uploads release the workspace between chunks. The OTA flash writer still needs its separate 4 KiB buffer for partial sectors.

The gateway loads installed trust anchors once and shares them with each verified client for its handshake. The flash writer updates the cached copy after committing the new bundle. EST converts a chain to PEM in the workspace and omits installed duplicates of enabled built-in roots. A single certificate can go directly into MbedTLS as DER. Certificate status and MQTT capture take turns in RTC FAST.

| Release reservation or image, bytes | Before (`2c6c1f4`) | After |
|---|---:|---:|
| General heap / dedicated TLS pool | 31,744 / 49,152 | 16,384 / 53,248 |
| SRAM1 linked | 124,008 | 127,080 |
| SRAM2 statics | 145,540 | 143,892 |
| RTC FAST / SLOW | 7,896 / 8,192 | 5,848 / 8,192 |
| Main stack budget | 51,068 | 52,716 |
| Flash image | 2,905,833 | 2,906,593 |

The combined allocator reservations shrink by 11,264 B. Counting the new static workspaces across SRAM1, SRAM2 and RTC gives a net saving of 624 B. IRAM is unchanged. The production QEMU test left 11,292 → 12,940 B of stack untouched immediately after an automation button save and 11,084 → 12,732 B after the edit sequence.

| QEMU workload | General heap peak before (B) | After (B) |
|---|---:|---:|
| Six-CA import, including status and error handling | 24,016 | 11,512 |
| EST identity import, including status and error handling | 21,232 | 11,512 |
| HTTPS certificate rotation | 18,724 | 11,512 |
| HTTPS automation edits | 8,368 | 8,360 |
| Two CA-verified Mosquitto brokers, CA replacement and concurrent HTTPS status | 30,600 | 11,512 |
| NTS authenticated time exchange, before requesting certificate details | 18,104 | 9,212 |

The first four workloads ran twice per build on fresh installations. For the larger EST case, the baseline reset while requesting 22,528 B to assemble the trust list. With fixed storage, enrollment and renewal succeeded with built-in and installed roots, the client identity and its intermediate certificate. The dedicated TLS pool peaked at 50,712 B. A 48 KiB pool exhausted its capacity in this workload.

The general heap still serves Picoserve request futures, temporary certificate and CSR data, Rust RSA verification and the NTS task's UDP buffers. Steady idle reaches zero live bytes. Removing the allocator requires fixed storage or allocation-free implementations for these active paths as well.

### Earlier MQTT and HTTPS measurements

The same `combined` scenario exercises two gateways, active CAN nodes, MQTT and an HTTPS automation read. The values below come from gateway A. The baseline allocator included MbedTLS. On the new build each allocator tracks its own peak, and those peaks can occur at different times.

| Bytes | Baseline `e3a0c88` | HTTPS/RTC step `a9e1b19` | After #104 (`cc03d13`) |
|---|---:|---:|---:|
| General heap reserved | 96,256 | 96,256 | 34,816 |
| Dedicated MbedTLS pool reserved | — | — | 49,152 |
| Total reserved for both allocators | 96,256 | 96,256 | 83,968 |
| General heap used at end of combined workload | 32,000 | 21,808 | 3,224 |
| General heap peak in combined workload | 72,724 | 62,436 | 9,452 |
| MbedTLS peak in combined workload | included above | included above | 39,276 |
| Untouched stack in combined workload | 26,316 | 23,620 | 21,052 |

The separate automation-edit test used to stop at the stack guard during the button save. In two consecutive QEMU runs it saved and read back the binding on the same boot. Both runs left **9,484 B** untouched immediately after the save and **9,276 B** after the entire edit sequence, out of 49,260 B in the profiling build. The minimum regression threshold is 8,192 B. The combined workload above has its own stack watermark.

## Baseline by bank

Baseline on 2026-11-21, gateway firmware `e3a0c88`, release, Picoserve 0.21.0. The release ELF gives the reservations below. QEMU used the same firmware sources with the additional `memory-profile` task. Sizes are bytes unless marked KiB (1,024 B). The previous hardware measurements used a smaller firmware with a larger stack. [Static report and trends](https://th3link.github.io/homeautomation/firmware-memory/) · [Earlier hardware measurements](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-core/docs/ram.md)

| Physical bank | Linked use | Where the remaining space goes |
|---|---|---|
| SRAM0, 192 KiB | 64 KiB flash cache, then 12,032 B of vectors and IRAM code at `0x40080000` | 119,040 B available for instruction-side placement. Word-only data access and no Ethernet DMA make this unsuitable for byte buffers and ordinary Rust objects. |
| SRAM1, 128 KiB | 2,160 B of ROM data. Primary MQTT at `0x3ffe0440`, secondary at `0x3ffe4350`, 13,312 B each. General heap at `0x3ffe7e30`, 96,256 B in `.dram2_uninit`. | Free gaps of 1,760 + 1,760 + 2,512 B. Separate physical ranges. The MQTT placements reuse ROM boot stacks after boot and require the APP CPU to stay parked. |
| SRAM2, 200 KiB | 8 KiB reserved. In the 192 KiB linker region at `0x3ffb0000`, `.data` takes 35,968 B and `.bss` takes 114,716 B. | The rest, 45,924 B starting at `0x3ffd4c9c`, belongs to the single main stack. Moving a static out of this bank returns room to that stack. |
| RTC FAST, 8 KiB | 0 B in the baseline application ELF. Data address `0x3ff80000`. | PRO CPU only. Its instruction alias is the same 8 KiB. Candidate for upload staging. |
| RTC SLOW, 8 KiB | 0 B in the baseline application ELF. Data address `0x50000000`. | Separate, slower bank shared with the ULP. Candidate for infrequently accessed CPU-owned state. |

Ethernet descriptors and packet buffers need DMA-capable internal DRAM. Keep them there. Use RTC for CPU-owned data with an explicit boot initialization policy. The project linker script and pinned HAL define the available ranges. [Gateway linker script](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/rom-boot-stacks.x) · [Pinned ESP32 memory map](https://github.com/Th3Link/esp-hal/blob/475cf2f76be9b03acf8da92b66b466c2ccf0d051/esp-hal/ld/esp32/memory.x) · [Espressif memory access rules](https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/memory-types.html)

### Largest SRAM2 owners

| Symbol or group | B | Section and lifetime |
|---|---:|---|
| MQTT task pool, two broker slots | 24,080 | `.bss`, reserved from boot, even with both brokers disabled |
| Ethernet DMA, 8 RX and 4 TX descriptors with packet storage | 18,676 | `.bss`, static |
| MQTT publish queues, 16 entries per slot | 9,792 | `.bss`, static |
| Embassy/smoltcp resources for 14 sockets | 8,792 | `.bss`, static, separate from each socket's byte windows |
| Automation runtime and automation task pool | 8,376 + 7,520 | `.data` and `.bss`, both resident |
| Device list and console task pool | 6,820 + 6,664 | `.bss`, both resident |
| Gateway log ring | 6,156 | `.data`, permanent |
| HA published-topic index and HTTPS shared HTTP/output buffers | 4,116 + 4,112 | `.bss`, both resident |
| Main task pool and JSON response buffer | 3,912 + 3,088 | `.bss`, both resident |
| CAN update session including its 1,024 B retransmission block | 1,068 | `.data`, reserved while idle too |

Most application code and read-only assets live in flash. The 96,256 B heap reservation is counted once in SRAM1. Its runtime clients below subdivide that reservation. Likewise, a local array or an awaited child inside a listed future is already included in the future size.

## What the heap and stack actually reached

These are the highest observed allocator use and least untouched stack across both repeats, gateways and recorded boots for each scenario. The two extrema may come from different samples. The instrumented stack is 45,852 B. The serial sampler reads boot-local high-water marks every two seconds. A reset can erase the final high-water mark before it is read. A passing OTA test can therefore have an incomplete memory trace. [Simulation run 44938906434 and artifacts](https://github.com/Th3Link/homeautomation/actions/runs/44938906434)

| Scenario | Peak heap used (B) | Minimum untouched stack (B) | Result |
|---|---:|---:|---|
| Idle, plain HTTP configured | 44,248 | 35,660 | Both passed. Steady heap use in the first trace was 40,000 B. |
| REST polling | 45,048 | 0 | Both failed, partial. Serial recorded a stack-guard write. |
| CAN/MQTT and HA reconnect | 36,248 | 35,660 | Both repeats of each passed. |
| MQTT TLS | 36,248 | 29,772 | One complete pass, one partial failure. |
| Active nodes, MQTT and an HTTPS automation request | 72,724 | 26,316 | Both passed. Highest captured heap use, 23,532 B free at that peak. |
| HTTPS automation routes | 36,248 | 33,340 | Both failed, partial. First trace shows a CPU load exception. Its last sample cannot establish the cause. |
| Gateway OTA | 50,544 | 0 | Both functional tests passed with partial traces. |
| Addressed CAN OTA | 50,544 | 36 | Both passed. The watermark excludes the 64 B guard area. |
| Artificial 8 KiB allocation probe | 48,192 | 35,084 | Both passed. Instrumentation workload. |

The combined workload gives a lower bound for required heap capacity. The traces give no per-owner peaks or largest contiguous free block. In particular they do not establish a safe smaller heap for overlapping HTTPS, two MQTT TLS handshakes and certificate management.

## Allocations in the baseline

### Web listeners and router

Compiler layouts give **10,456 B per plain HTTP listener** (three boxed futures, 31,368 B together), **3,136 B per HTTPS listener** (five, 15,680 B) and **7,688 B for the single HTTP-to-HTTPS redirect**. Plain mode and HTTPS-plus-redirect are alternatives selected at boot. In plain mode the listeners alone account for about 78% of the observed 40,000 B steady idle heap. The five HTTPS listeners accept browser connections concurrently but only one serves a TLS session at a time. Their individual TCP windows must survive while they wait. [Web server and task lifetimes](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/src/web/mod.rs)

For the remaining five HTTPS listeners, evaluate in-place storage in SRAM1 after measuring the TCP window changes. The router and handlers also allocate boxes per request. Measure the large poll frames before sizing a single HTTPS request slot. Keep request borrows valid through cancellation and release the slot on all exits.

### MQTT and TCP windows

Each of the **two 13,312 B SRAM1 buffers** contains 2,048 B TCP RX, 2,048 B TCP TX, 8,192 B TLS RX and 1,024 B TLS TX. Both connections can use TLS simultaneously. The embedded-tls connection borrows its record buffers throughout the session. A shared handshake lock alone cannot release either read buffer for the other live connection. The project recorded a 4,119 B certificate record with Mosquitto. A blanket 4 KiB receive limit would break that chain. A smaller socket window still allows a TLS record to arrive across multiple TCP segments. [Broker task](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/src/mqtt.rs) · [TLS record constraints](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/src/tls.rs)

There is more room outside the record layer. The compiler puts 12,000 B into each MQTT task future (12,040 B including its Embassy slot). The TLS session child is 7,720 B, the plain child 4,600 B. Their layouts include a 2,048 B MQTT receive arena. The outer task also holds a 528 B full configuration and a 2,056 B optional pinned certificate across suspension points. Load only per-broker settings and keep temporary verification state in a bounded handshake workspace. If handshakes are serialized, keep existing connections serving traffic and retain independent per-session records. Preserve the advertised MQTT 5 maximum packet size and test large retained messages.

Only the primary had a live application-queue producer, including the reconnect announcement. Removing the unused secondary 16-entry queue released **4,896 B in SRAM2**. Protocol traffic such as CONNECT, SUBSCRIBE and acknowledgements still needs the secondary socket's TX path.

### OTA, RTC and flash scratch

All HTTP upload targets use the same `BUSY` guard. In the baseline each active upload carries a 4,096 B array in a boxed handler. Release compiler sizes are 7,008 B for the plain handler and 7,016 B for HTTPS. The nested receive future accounts for 6,616/6,624 B respectively, including the array. The current build borrows one RTC FAST staging slot under the existing exclusivity rule. Preserve the 409 conflict, timeout, CRC error, interrupted upload and subsequent retry. Initialize synchronization state on every boot. [Upload path](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/src/web/firmware.rs)

The gateway has a **second** sector-sized cost. `esp-hal-ota` calls `SharedFlash::Storage::write`, which delegates to `esp-storage 0.10.0`. That implementation reads a sector into a local aligned 4,096 B buffer, merges the data, erases and writes it back, even for full-sector writes. The existing 512 B static bounce serves other flash operations. For aligned full sectors, investigate erasing once and feeding the existing aligned DRAM bounce from RTC. Preserve bytes outside partial first/last writes and writes to OTA metadata. Keep the final readback/CRC. A static DRAM sector workspace is the fallback if the partial-sector path needs it. Reducing the HTTP chunk to 1 KiB with the existing writer can turn one sector erase into four. [Shared flash adapter](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/common-hardware/src/flash.rs) · [Gateway OTA](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/src/update.rs) · [Pinned sector writer](https://github.com/esp-rs/esp-hal/blob/63e97be553f435f29171f7ae5f37a41ad737a75a/esp-storage/src/storage.rs)

The addressed CAN protocol stores its 1,024 B block until ACK or retransmission. Legacy frames carry at most eight bytes. The retry block could borrow the appropriate slice of the existing upload slot while sending, if all producers and cancellation paths preserve its lifetime. Keep retries byte-identical. The remaining RTC FAST space is too small for the 1,068 B CAN session. RTC SLOW holds the full node log, so relocating the 4,116 B HA published-topic index there would require a smaller log or a different layout. [CAN update session](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/src/can_update.rs) · [HA index](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/src/ha.rs)

### Other general-heap clients

| Client | Evidence and replacement |
|---|---|
| HA discovery and logging | HA's long-running boxed future is 2,904 B, the two log worker futures 1,464 B each. HA also collects three variable-length snapshots. Iterate bounded batches, release locks before publishing, and define how to rescan after concurrent changes. The node web log grows a heap string up to 8 KiB. Stream pages or use a bounded log buffer shared for the lifetime of a download and its reader. |
| Automation restore | `Box<Saved>` is 3,384 B during restore. Read entries directly into the runtime or borrow temporary boot storage. A permanent dedicated 3,384 B buffer would also cost that much after boot. |
| Time and enrollment | NTS key exchange uses a boxed 8,000 B handshake future plus dynamic TCP, record and reply arrays (1,024 + 512, 8,192 + 1,024 and 1,280 B), plus a variable certificate chain. NTS UDP queries allocate three 1,024 B buffers. EST allocates two 2,048 B TCP buffers and can collect a 12 KiB HTTP response plus decoded DER. Reuse bounded buffers according to the existing HTTPS/EST/NTS-KE slot. UDP queries and MQTT verification can overlap that slot. |
| Certificates, TLS and RSA | Credential and trust blobs are loaded from flash into vectors (up to 6,144 B per server chain or trust blob, 4,096 B for the EST identity chain). Built-in root DER is already in flash. MbedTLS allocates parsed certificates, keys, records (16 KiB input, 8 KiB output configured) and variable-size handshake internals via calloc/free. MQTT/NTS verification can also use MbedTLS X.509 and the Rust RSA bignum allocator. The global TLS context and server credentials can outlive a session. |
| Other futures and allocations | DHCPv6's long-running box is 2,800 B. DNS, time sync, console and API paths have shorter-lived boxes. Bound payloads and in-place temporaries before removing boxes. Exclude the deliberately allocating `memory-profile` probe from a production zero-allocation count. |

For TLS, first trace Rust allocations **and** the C calloc/free path, by size, lifetime and simultaneous count. Separate long-lived credential objects from reusable handshake scratch. Use typed static slots for records and bounded size-class pools for library allocations that still need independent frees. Initialize allocator hooks once before any TLS object exists, preserve alignment and zero-on-calloc, and handle exhaustion without a reboot. A region reset on session close would invalidate long-lived objects or another broker's certificate verification. A TLS-specific allocator is still dynamic allocation and should have its own budget. Claim a fully heapless build only once Rust and C paths both demonstrate it.

## Earlier release-build sizing experiment

An isolated copy of the gateway used a single RTC FAST mutex-protected upload array, MQTT TCP windows of 1,536/1,024 B (RX/TX), a plain HTTP RX window of 2,048 B and an HTTPS RX window of 1,536 B. The TLS record sizes, listener counts and secondary broker stayed the same. The table compares compiler layouts. Multiplicities describe allocations in the selected mode.

| Object | Baseline B | Experiment B | Effect |
|---|---:|---:|---|
| MQTT TCP + TLS buffers per broker | 13,312 | 11,776 | 3,072 B of SRAM1 released across two distinct ROM boot areas |
| Plain HTTP listener future | 10,456 | 8,408 | 6,144 B less long-lived heap allocation across three listeners in plain mode |
| HTTPS listener future | 3,136 | 2,624 | 2,560 B less long-lived heap allocation across five listeners in HTTPS mode |
| Redirect future | 7,688 | 7,688 | Present once in HTTPS mode |
| Boxed upload handler, plain / HTTPS | 7,008 / 7,016 | 2,912 / 2,928 | Active allocation shrinks by 4,096 / 4,088 B. RTC holds one staging slot even while idle. |

The experimental release ELF places the 4,096 B upload array and 16 B of mutex state at `0x3ff80000` in `.rtc_fast.data` (4,112 B total). That leaves 4,080 B in FAST and all 8,192 B in SLOW. This initialized section also has a flash load image. A separate boot-initialized guard and a zeroed payload section could avoid storing the zeros in flash. The ELF still gives the main stack 45,924 B and reserves 96,256 B for the general heap. No runtime saving or new stack margin has been measured for this experiment.

In this earlier experiment, shrinking the HTTPS RX windows without moving the buffers out of the listener futures would give 13,120 B across five connections. The two MQTT savings land in different linker sections and cannot simply be added to a contiguous TLS pool. The upload move trades general heap for RTC. The TCP changes reduce total bytes required. Throughput and interoperability require separate tests.

## Implementation and remaining validation

1. **MQTT and TCP windows.** The secondary publication queue is gone. Exercise commands from both brokers, retained packets across TCP segments, HA republish on reconnect, a large TLS certificate and five simultaneous browser downloads. Compare gateway OTA with an added loopback delay.
2. **Flash writes.** Full and partial OTA sectors now share the flash lock and use bounded scratch. QEMU covers a wrong CRC followed by a valid update, a version switch and preservation of settings or the peer node. Cold boot and interrupted power during a flash write still need physical ESP32 coverage.
3. **Application storage.** HTTPS socket buffers, the DHCPv6 workspace and the HA task are static. NTS-KE and EST reuse the HTTPS slot. The automation task owns its reload snapshot. The plain SNTP request and shared DNS resolver are heapless. The installed root list and certificate inspection now have static storage. Temporary credentials, NTS UDP buffers and some request futures still use the general heap.
4. **Crypto allocator.** MbedTLS runs in a dedicated 52 KiB pool. Local QEMU tests exercised EST issuance and renewal against a test CA, then authenticated NTS against chronyd. Long production certificate chains and simultaneous pinned/RSA broker verification still need a deployment-hardware measurement.
5. **Automation button stack.** A name lookup retains only the slot and enabled flag after decoding, and updates serialize the submitted item by reference. Switches and outputs have their own subrouters. Authentication hands each API request to its resource group, which uses the existing per-request allocation for that group’s handlers. The QEMU test writes and reads back the binding, checks the boot ID and reports the stack watermark immediately after the save and after the full edit sequence. The existing 8 KiB regression threshold applies to both measurements.

### Reproducing the sizes

Run these commands from `software/gateway-hardware` with the ESP toolchain environment loaded.

```sh
cargo +esp build --locked --release
elf=../../target/xtensa-esp32-none-elf/release/gateway-hardware
xtensa-esp32-elf-readelf -SW "$elf"
xtensa-esp32-elf-nm -S --size-sort --radix=d -C "$elf" | rg ' [bBdD] ' | tail -35
cargo +esp rustc --locked --release --lib -- -Zprint-type-sizes > /tmp/gateway-types.txt 2>&1
rg 'web::(serve_session|https_task)|mqtt::.*task' /tmp/gateway-types.txt
```

Generate each compiler dump from a fresh build of the gateway crate if Cargo reports it as fresh. Compare like profiles and feature sets. Use the existing simulation runner and preserve `runtime.json` plus serial logs for each before/after comparison. [Memory runner](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/sim/memory_profile.py) · [QEMU test scenarios](https://github.com/Th3Link/homeautomation/tree/e3a0c88/software/sim/tests)
