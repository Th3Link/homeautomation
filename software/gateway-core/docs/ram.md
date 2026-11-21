# RAM on the gateway (ESP32)

## HTTPS, logs and firmware uploads

The gateway serves the UI and API on port 443. Removing the port 80 redirect eliminates a 7,688 B long-lived boxed future in the former default mode. A saved plain-HTTP setting from older firmware has no effect after upgrading. The five HTTPS sockets still share one active TLS session.

| Storage | Location | Allocated bytes |
|---|---|---:|
| Node log download text, shared by successive requests | RTC SLOW, `.rtc_slow.bss` | 8,192 |
| CAN log frame queue | RTC FAST, `.rtc_fast.data` | 672 |
| Gateway log batch sent over MQTT, including its lock | RTC FAST, `.rtc_fast.data` | 1,044 |
| Shared firmware upload chunk for gateway and node updates, including its lock | RTC FAST, `.rtc_fast.data` | 4,112 |
| Gateway log ring with cursor and lock | SRAM2, `.data` | 6,156 |

The two log workers use ordinary Embassy task storage in SRAM2. A download resets its length and status under the same lock used by readers. Only its valid UTF-8 prefix is exposed. RTC FAST has 2,364 B left. Only one upload runs at a time, so all three upload routes use the same chunk. The flash driver still has its own sector-sized stack buffer. The bank figures and sizing experiment below describe the earlier build used to select these changes.

One local `memory-profile` release run repeated the baseline QEMU `combined` scenario with two gateways, active CAN nodes, Mosquitto and a concurrent HTTPS automation read. Gateway A reached 62,436 B peak heap and ended at 21,808 B. The baseline reached 72,724 B and ended at 32,000 B. The unchanged 96,256 B heap reservation therefore had 10,288 B more headroom at the measured peak. Stack margin in this workload was 23,620 B (baseline 26,316 B). A separate ten-second HTTPS `/system` capture completed on one boot with 15,508 B untouched. The full automation-edit workload still hit the guard in `put_button` during output lookup.

Both HTTPS OTA scenarios completed their bad-CRC checks and the subsequent version switch from `qemu-v1` to `qemu-v2`. The gateway update preserved the configuration across reboot and reached 65,436 B peak heap with 15,508 B of stack untouched. The addressed CAN update left the other node on `qemu-v1` and reached 65,300 B peak gateway heap with 15,352 B of stack untouched. These are QEMU measurements.

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

Only the primary currently has a live application-queue producer, including the reconnect announcement. Dropping the unused secondary 16-entry queue would release **4,896 B in SRAM2**. If an explicit secondary publish must remain available, give it one or two entries and document its existing full-queue policy. Protocol traffic such as CONNECT, SUBSCRIBE and acknowledgements still needs the secondary socket's TX path.

### OTA, RTC and flash scratch

All HTTP upload targets use the same `BUSY` guard. In the baseline each active upload carries a 4,096 B array in a boxed handler. Release compiler sizes are 7,008 B for the plain handler and 7,016 B for HTTPS. The nested receive future accounts for 6,616/6,624 B respectively, including the array. The current build borrows one RTC FAST staging slot under the existing exclusivity rule. Preserve the 409 conflict, timeout, CRC error, interrupted upload and subsequent retry. Initialize synchronization state on every boot. [Upload path](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/src/web/firmware.rs)

The gateway has a **second** sector-sized cost. `esp-hal-ota` calls `SharedFlash::Storage::write`, which delegates to `esp-storage 0.10.0`. That implementation reads a sector into a local aligned 4,096 B buffer, merges the data, erases and writes it back, even for full-sector writes. The existing 512 B static bounce serves other flash operations. For aligned full sectors, investigate erasing once and feeding the existing aligned DRAM bounce from RTC. Preserve bytes outside partial first/last writes and writes to OTA metadata. Keep the final readback/CRC. A static DRAM sector workspace is the fallback if the partial-sector path needs it. Reducing the HTTP chunk to 1 KiB with the existing writer can turn one sector erase into four. [Shared flash adapter](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/common-hardware/src/flash.rs) · [Gateway OTA](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/src/update.rs) · [Pinned sector writer](https://github.com/esp-rs/esp-hal/blob/63e97be553f435f29171f7ae5f37a41ad737a75a/esp-storage/src/storage.rs)

The addressed CAN protocol stores its 1,024 B block until ACK or retransmission. Legacy frames carry at most eight bytes. The retry block could borrow the appropriate slice of the existing upload slot while sending, if all producers and cancellation paths preserve its lifetime. Keep retries byte-identical. The 1,068 B CAN session also fits in the remaining RTC FAST space. RTC SLOW now holds the full node log, so relocating the 4,116 B HA published-topic index there would require a smaller log or a different layout. [CAN update session](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/src/can_update.rs) · [HA index](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/gateway-hardware/src/ha.rs)

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

For the current HTTPS-only firmware, the same smaller RX windows would give 13,120 B of listener futures across five connections. The two MQTT savings land in different linker sections and cannot simply be added to a contiguous TLS pool. The upload move trades general heap for RTC. The TCP changes reduce total bytes required. Throughput and interoperability require separate tests.

## Order of implementation and acceptance

1. **Release unused SRAM2 and shrink the long-lived allocations.** Remove or shorten the secondary application queue. Apply the TCP window changes independently. Compare the ELF and the live allocator after boot, over HTTPS. Run two simultaneous brokers on one gateway, including two TLS connections, HA republish on reconnect, long certificate chains, retained commands and five browser sockets. Record transfer time for a large OTA before and after, including a higher-latency connection.
2. **Validate the shared RTC slot on hardware and remove the flash-sector stack frame.** Check section address, size and initialization in the ELF and exercise the real ESP32 after cold boot and software reset. Test HTTPS uploads, gateway version switch, CAN retransmission, wrong CRC, truncated body, odd last sector, busy and retry. Compare per-boot watermarks. Retest the independent automation-edit stack-guard failure when no OTA is active.
3. **Convert persistent application allocations to typed SRAM1 storage.** Start with the five HTTPS listeners. Then measure and migrate HA discovery, DNS, NTS/EST staging and temporary handler state in lifetime order. Keep shared storage behind one owner. Validate concurrent requests, repeated connects/disconnects and cancellation. Publish static bytes per bank, dynamic bytes by owner, largest free block, stack margin and total reserved bytes. A smaller peak with the same 94 KiB reservation is an intermediate result.
4. **Bound the residual crypto allocator, then retire the general heap.** Exercise first boot, full certificate chains, key renewal, EST, NTS, dual-broker TLS, simultaneous HTTPS and OTA, and repeated failures. Record peak live counts per class, peak bytes, largest request, failed requests and steady-state alloc/free calls. Size the pool from these traces plus an explicit margin and repeat on hardware. Check that normal application paths make zero general-heap allocations, then check boot and crypto separately before reducing or removing `esp-alloc`. Track CPU stack in parallel. Require complete traces with no guard write or unexpected reset before accepting a smaller stack budget.

### Reproducing the sizes

Run these commands from `software/gateway-hardware` with the ESP toolchain environment loaded.

```sh
cargo +esp build --locked --release
elf=../../target/xtensa-esp32-none-elf/release/gateway-hardware
xtensa-esp32-elf-readelf -SW "$elf"
xtensa-esp32-elf-nm -S --size-sort --radix=d -C "$elf" | rg ' [bBdD] ' | tail -35
cargo +esp rustc --locked --release --lib -- -Zprint-type-sizes > /tmp/gateway-types.txt 2>&1
rg 'type: .*async fn body of web::(serve_plain|serve_tls|serve_redirect)\(\)' /tmp/gateway-types.txt
```

Generate each compiler dump from a fresh build of the gateway crate if Cargo reports it as fresh. Compare like profiles and feature sets. Use the existing simulation runner and preserve `runtime.json` plus serial logs for each before/after comparison. [Memory runner](https://github.com/Th3Link/homeautomation/blob/e3a0c88/software/sim/memory_profile.py) · [QEMU test scenarios](https://github.com/Th3Link/homeautomation/tree/e3a0c88/software/sim/tests)
