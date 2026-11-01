# RAM on the gateway (ESP32)

What lies where in the gateway's RAM, which sizes are critical, and where
there is room left.

**Measured on** the integration build of 2026-11-01: `main` with the
branches `feat/ca-trust-store`, `feat/tls-key-pin`, `feat/ha-discovery` and
`feat/nts` (firmware `release/1.4-194-ge033598`). Static sizes are from the
ELF (`xtensa-esp32-elf-size -A`, `nm -S`, a linker map); run-time figures
are from `GET /api/v1/system` on the test gateway after 75 minutes, with
HTTPS, MQTT over TLS and NTS in use. CI tracks static DRAM and the stack
budget per commit (`.github/scripts/memory_report.py`).

## The chip's RAM

| Block | Size | Addresses | Access |
|---|---|---|---|
| SRAM0 | 192 KB | `0x4007_0000`–`0x4009_FFFF` | Instruction bus only: code, or 32-bit loads and stores. No DMA. The first 64 KB are the flash cache. |
| SRAM1 | 128 KB | `0x3FFE_0000`–`0x3FFF_FFFF` (data), `0x400A_0000`–`0x400B_FFFF` (instruction, the same RAM mirrored) | Code or data. DMA-capable. |
| SRAM2 | 200 KB | `0x3FFA_E000`–`0x3FFD_FFFF` | Data only. DMA-capable. |
| RTC FAST | 8 KB | `0x3FF8_0000` (data) | Data and code, PRO CPU only. No DMA. |
| RTC SLOW | 8 KB | `0x5000_0000` | Data and code, slower bus. No DMA. |

The gateway runs on one core (PRO CPU). The APP CPU is never started.

## The map today

```text
SRAM0  0x4007_0000  flash cache (PRO + APP)                    64 KB
       0x4008_0000  .vectors                                    1 KB
       0x4008_0400  .rwtext (interrupt and flash-off code)     10.8 KB
       0x4008_2F00  -- unused --                              116.3 KB

SRAM2  0x3FFA_E000  reserved for the ROM (esp-hal)               8 KB
       0x3FFB_0000  .data                                      16.6 KB
       0x3FFB_4288  .bss                                      138.2 KB
       0x3FFD_6B74  main stack (grows down to here)            37.1 KB
       0x3FFE_0000

SRAM1  0x3FFE_0000  ROM data, PRO CPU                           1.1 KB
       0x3FFE_0440  -- unused --                                3.7 KB
       0x3FFE_1320  ROM stack, PRO CPU (boot only)               11 KB
       0x3FFE_3F20  ROM data, APP CPU                           1.0 KB
       0x3FFE_4350  -- unused --                                3.7 KB
       0x3FFE_5230  ROM stack, APP CPU (never used)              11 KB
       0x3FFE_7E30  heap (esp-alloc, `.dram2_uninit`)            94 KB
       0x3FFF_F630  -- unused --                                2.5 KB

RTC    0x3FF8_0000  RTC FAST -- unused --                         8 KB
       0x5000_0000  RTC SLOW -- unused --                         8 KB
```

esp-hal's linker script (`ld/esp32/memory.x`) fixes the layout. The 192 KB
from `0x3FFB_0000` to `0x3FFE_0000` (`dram_seg`) hold `.data`, `.bss` and
then the main stack: **the stack is whatever the statics leave**. Every
byte a static grows is a byte less stack.

## The statics (`.data` 17,028 B + `.bss` 141,548 B)

Grouped by owner, largest first. Sizes in bytes.

| What | Size | Notes |
|---|---:|---|
| **MQTT** | **68,600** | 48 % of `.bss` |
| `mqtt::BUFFERS` | 33,808 | Per broker slot (2): TCP 2 + 2 KiB, TLS read record 8 KiB, write record 2.5 KiB, certificate capture 2 KiB. Static even for a slot that is off. |
| `mqtt::mqtt_task::POOL` | 24,032 | The two task futures, ~12 KB each: a pinned certificate (2 KiB), the client's bump buffer (2 KiB), the configuration, the TLS connection's state. |
| `mqtt::PUBLISH_QUEUES` | 9,792 | 2 × 16 publications of 96 + 200 bytes. |
| `mqtt::RETAINED` | 904 | One Home Assistant discovery payload. |
| **Ethernet and IP** | **28,700** | |
| `ethernet::DMA_STORAGE` | 18,676 | 8 RX + 4 TX buffers of 1,536 B and their descriptors. Must be DMA-capable RAM. |
| `ethernet::NET_RESOURCES` | 8,736 | smoltcp's 14 socket slots, ~620 B each. |
| `dhcpv6::STATUS`, `ethernet::INFO`, task futures | ~1,300 | |
| **Console** | **12,264** | `console_task::POOL`: the line editor 2.2 KB; `handle_command` 9.9 KB, of which 4.6 KB are the 160-byte reply strings of its ~28 arms, each in a slot of its own (see below); `show` 5.3 KB, of which 2 KB are such strings and 2.5 KB the configuration load. |
| **Devices and sensors** | **11,300** | `device_list::DEVICE_LIST` 6,820 (100 devices), `sensor_list` 2,416, `can_update::SESSION` 1,068, CAN channel and receive task 1,000. |
| **`.data` constants** | **10,215** | Switch tables (7,467) and merged constants (2,748), placed in RAM by esp-hal's `place-switch-tables-in-ram` (default on). |
| **Web server and HTTPS** | **7,300** | `https::SLOT` 3,600 (the HTTP and response buffers of the one TLS session), `web::json::BUFFER` 3,088, the listener tasks' futures (their buffers are on the heap). |
| **Home Assistant** | **4,650** | `ha::PUBLISHED` 4,116 (hashes of 256 published topics), `ha::FEATURES` 532. |
| **`main`'s future** | **3,848** | Holds the whole `GatewayConfig` for as long as the gateway runs. |
| **Time and NTS** | **3,400** | `nts::ASSOCIATION` 1,800 (cookies and keys), `sntp_task::POOL` 1,552 (UDP buffers). |
| **Configuration** | **2,140** | `config::CONFIG`, the store's 2 KiB item buffer. |
| **Runtime** | ~3,500 | `esp_rtos::scheduler` 1,368, MbedTLS's globals 1,300, esp-hal's peripheral state, wakers. |

## Heap (94 KiB, in SRAM1)

`GET /system` during an HTTPS request: **70,812 B used, 25,444 B free**.

- **Always there**: the five HTTPS listeners and the port-80 redirect
  (TCP buffers of 2 + 1 KiB each, ~19 KB together), the parsed HTTPS
  certificate and key, the trust anchors.
- **One at a time** (`https::slot()`): an HTTPS session ~40 KB (MbedTLS
  records 16 KiB in and 8 KiB out, handshake state, picoserve), the NTS key
  exchange ~25 KB, the EST client.
- **Now and then**: the RSA signature check of an MQTT broker's chain
  (bignums), boxed futures (`heap::boxed`) of large handlers and console
  commands.

Free memory isn't the only limit: a session needs its 16 KiB record
buffer in one piece, so fragmentation counts.

## Stack (38,028 B)

Everything runs on the one main stack: the executor, every task's poll,
MbedTLS's handshake, flash access (see "One main stack for everything" in
[technical debt](technical-debt.md)).

**Measured: 2,720 B never touched since boot**, i.e. a peak of ~35.3 KB.
That is the most critical figure on the gateway. On 2026-10-27 the stack
was 47 KB with ~14 KB to spare. Since then the statics grew by ~9 KB (HA
discovery, NTS, the trust store, DHCPv6 status), which took that much from
the stack, and the peak grew by ~2 KB.

Directly below the stack are MbedTLS's globals (the cipher suite list, the
ECP state). An overflow overwrites them before it reaches esp-hal's stack
guard word, so it shows as TLS failures or crashes far from the cause.
Which code path makes the peak isn't known yet: the watermark only says
how deep, not where.

## Critical sizes

| Size | Value | Why it matters |
|---|---:|---|
| Stack margin | 2.7 KB | See above. A larger future or a deeper call in a handler crashes the gateway. |
| Free heap during HTTPS | 25 KB | A second large user (EST, NTS key exchange) has to wait for the slot; anything outside the slot must fit here, in pieces. |
| MbedTLS record in/out | 16 / 8 KiB | Browsers send full 16 KiB records (firmware upload); the certificate chain must fit in one 8 KiB message. |
| MQTT TLS read record | 8 KiB | A broker's whole certificate chain must arrive in one record. |
| Config store item | 2 KiB | One setting or pinned certificate; longer values (certificate chains) are split over several items. |

## Room for improvement

Ordered by what it does for the stack. Nothing here puts more on the heap:
the heap stays for what can't do without it (MbedTLS allocates its
session state, picoserve's per-connection futures are boxed so they don't
take stack). Sizes stay fixed at link time.

1. **Futures that waste their space: ~7 KB in the console alone.** rustc
   gives every temporary that lives across an `.await` a slot of its own
   in the future and doesn't share it with the temporaries of other match
   arms or statements. `respond(writer, &fmt_msg(...)).await` keeps a
   160-byte `String` per call site: 28 of them in `handle_command`
   (4.6 KB), 13 in `show` (2 KB). One reply variable per function (`let
   reply = match ... ; respond(writer, &reply).await`) or writing straight
   into the UART with `core::fmt` brings the console from 12.2 KB to ~5 KB.
   `-Zprint-type-sizes` shows these layouts (`padding` between the slots);
   the MQTT task (12 KB each) and the port-80 redirect (7.7 KB for an
   answer of a few lines) are worth the same look.
2. **Large static buffers into SRAM1: +52.5 KB stack.** The MQTT buffers
   (33.8 KB) and the Ethernet DMA buffers (18.7 KB) don't have to be in
   SRAM2. Placed in `.dram2_uninit` (`#[link_section]`, like the heap
   today; not zeroed at boot, so they are initialised once in `main`) they
   leave SRAM2 to the stack. The heap shrinks by as much, which the next
   point gives back.
3. **Reclaim the ROM areas: +35.7 KB.** ESP-IDF uses them on the ESP32,
   keeping only the ROM's data (`memory_layout.c`): `0x3FFA_E6E0`–
   `0x3FFB_0000` (6,432 B), `0x3FFE_0440`–`0x3FFE_3F20` (15,072 B, the PRO
   CPU's boot stack) and `0x3FFE_4350`–`0x3FFE_7E30` (15,072 B, the APP
   CPU's stack, never used; it adjoins `.dram2_uninit`). Either as heap
   regions (esp-alloc takes three, `HEAP.add_region`) to keep the heap at
   94 KB after point 2, or with linker sections of our own for more
   statics. Plus the unused 2.5 KB after the heap.
4. **Smaller MQTT task futures: up to +20 KB.** Each holds a pinned
   certificate (2 KiB), the client's 2 KiB bump buffer, the configuration
   and the TLS state for the whole session, also in a slot that is off.
   Load the pin only for the handshake, keep only the broker's settings.
5. **Switch tables in flash: +10.2 KB.** `ESP_HAL_CONFIG_PLACE_SWITCH_TABLES_IN_RAM=false`.
   The risk: code that runs while the flash cache is off (flash writes,
   IRAM interrupt handlers) must not read them. Test with OTA and
   configuration writes under CAN and Ethernet traffic.
6. **RTC memory for statics no DMA touches: up to +16 KB.** RTC FAST (8 KB)
   for the device list (6.8 KB), RTC SLOW (8 KB) for things read rarely,
   such as `ha::PUBLISHED`. Needs a check that they start zeroed.
7. **Small items: ~+8 KB.** One publish queue of 16 for the primary broker
   and a short one for the secondary (−4.9 KB); `main` keeps only the
   fields of the configuration it needs (−3 KB); fewer smoltcp socket
   slots if the count allows; 6 Ethernet RX buffers instead of 8 (−3 KB)
   after a test under load.

**Not usable for data: SRAM0's 116 KB.** It only takes 32-bit accesses
and no DMA; Rust code reads and writes bytes everywhere. ESP-IDF either
gives it out as "32-bit only" heap or emulates byte access in an exception
handler (slow). It can take more code (MbedTLS's bignum or AES routines)
for speed, which frees no RAM.

**Before any of this: find the stack's peak.** A watermark that can be
reset from the console would show, per action (HTTPS request, upload,
MQTT connect, NTS key exchange, EST), how deep each goes.
