#!/usr/bin/env python3
"""Memory footprint of ESP32 (Xtensa) firmware ELFs, for CI.

No dependencies beyond the Python standard library (it
parses the ELF itself rather than shelling out to `size`, so it doesn't
depend on which binutils the runner's toolchain happens to ship):

  measure  <elf> --name N --profile P [--partitions CSV] --out metrics.json
  report   --metrics DIR [--baseline DIR]      -> Markdown on stdout

What's measured, and why these numbers:

  flash image   Sum of every allocated PROGBITS section — i.e. the bytes
                that actually have to live in flash. That's .text/.rodata
                (executed/read straight out of flash) *plus* .data and
                .rwtext, whose load address is in flash even though they're
                copied to RAM at boot. Compared against the app partition
                size from partitions.csv.
  IRAM          .rwtext/.vectors (code placed in internal instruction RAM).
  static DRAM   Initialized and uninitialized statics inside the primary
                linker DRAM range in SRAM2, including embassy task futures.
  RAM sections  Every allocated ELF section placed in an internal SRAM
                bank or RTC memory, with its runtime address and size.
  stack budget  esp-hal's linker script gives the stack *whatever DRAM is
                left* after static data (the `.stack` section runs up to the
                end of the DRAM region). That's the stack's *budget*, not
                its usage: peak stack depth can't be derived from the ELF
                (it needs a runtime measurement, e.g. stack painting on a
                real device), but "static DRAM + stack budget == the whole
                DRAM region" always holds, and a shrinking budget is the
                early warning that matters.
"""

import argparse
import csv
import glob
import json
import os
import re
import struct
import sys
from html import escape
from pathlib import Path

SHF_ALLOC = 0x2
SHT_PROGBITS = 1
SHT_SYMTAB = 2
SHT_NOBITS = 8
STT_OBJECT = 1

# ESP32 address map (ESP32 TRM, "System and Memory").
DRAM = (0x3FFB0000, 0x3FFE0000)  # data RAM region esp-hal links into
IRAM = (0x40080000, 0x400A0000)
SRAM1_SIZE = 128 * 1024  # instruction and data addresses alias these bytes
SRAM1_ROM = (0x0440 + 0x0430)
SRAM1_SLOTS = {
    ".rom_boot_pro_uninit": (0x3FFE0440, 0x3FFE3F20),
    ".rom_boot_app_uninit": (0x3FFE4350, 0x3FFE7E30),
    ".dram2_uninit": (0x3FFE7E30, 0x40000000),
}
IROM = (0x400D0000, 0x40400000)  # flash-mapped code
DROM = (0x3F400000, 0x3F800000)  # flash-mapped read-only data
RTC_FAST = [(0x3FF80000, 0x3FF82000), (0x400C0000, 0x400C2000)]
RTC_SLOW = (0x50000000, 0x50002000)

RAM_BANKS = {
    "SRAM0": ((0x40070000, IRAM[1]),),
    "SRAM1": ((0x3FFE0000, 0x40000000), (0x400A0000, 0x400C0000)),
    "SRAM2": ((0x3FFAE000, DRAM[1]),),
    "RTC FAST": tuple(RTC_FAST),
    "RTC SLOW": (RTC_SLOW,),
}

BANK_USES = {
    "SRAM0": "64 KiB for the flash caches. The remaining 128 KiB hold interrupt vectors and code in IRAM.",
    "SRAM1": "The data and instruction addresses map to the same bank. The gateway places its heap and MQTT buffers here, alongside ROM data.",
    "SRAM2": "8 KiB reserved for ROM. The primary 192 KiB DRAM range holds statics, task state, and the main stack.",
    "RTC FAST": "Retained memory with mirrored data and instruction addresses.",
    "RTC SLOW": "Retained memory shared with the low-power subsystem.",
}

SECTION_USES = {
    ".vectors": "Interrupt and exception vectors",
    ".rwtext": "Code that runs directly from RAM",
    ".data": "Initialized static data, copied from flash at boot",
    ".bss": "Zero-initialized statics and task state",
    ".noinit": "Static data left uninitialized at boot",
    ".stack": "Main stack reservation",
    ".rom_boot_pro_uninit": "Gateway MQTT buffers for the primary broker",
    ".rom_boot_app_uninit": "Gateway MQTT buffers for the secondary broker",
}

DEFAULT_APP_PARTITION = 0x100000  # if partitions.csv can't be read


def inside(addr, rng):
    return rng[0] <= addr < rng[1]


def read_sections_and_symbols(path):
    with open(path, "rb") as f:
        data = f.read()
    if data[:4] != b"\x7fELF" or data[4] != 1 or data[5] != 1:
        sys.exit(f"{path}: not a 32-bit little-endian ELF")

    shoff, = struct.unpack_from("<I", data, 0x20)
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", data, 0x2E)

    raw = []
    for i in range(shnum):
        (name, typ, flags, addr, off, size, link, info, align, entsize) = (
            struct.unpack_from("<10I", data, shoff + i * shentsize)
        )
        raw.append(dict(name=name, type=typ, flags=flags, addr=addr, off=off,
                        size=size, link=link, entsize=entsize))

    strtab = raw[shstrndx]

    def cstr(table, idx):
        start = table["off"] + idx
        return data[start:data.index(b"\0", start)].decode("utf-8", "replace")

    for s in raw:
        s["name"] = cstr(strtab, s["name"])

    symbols = []
    for s in raw:
        if s["type"] != SHT_SYMTAB:
            continue
        names = raw[s["link"]]
        for i in range(s["size"] // 16):
            (n, value, size, info, _other, shndx) = struct.unpack_from(
                "<IIIBBH", data, s["off"] + i * 16
            )
            if size and info & 0xF == STT_OBJECT and 0 < shndx < len(raw):
                symbols.append(dict(name=cstr(names, n), addr=value,
                                    size=size, section=raw[shndx]["name"]))
    return raw, symbols


_V0_TOKEN = re.compile(r"Cs[0-9A-Za-z]*_|B[0-9A-Za-z]*_|(\d+)|.", re.S)


def demangle_v0(name):
    """Heuristic only — pulls the length-prefixed identifiers out of a v0
    (`_R...`) symbol and joins them with `::`. Skips crate disambiguators
    (`Cs<base62>_`) and back-references (`B<base62>_`), whose base62 digits
    would otherwise read as identifier lengths. Good enough for a
    "what is this 4 KiB static" table; not a real demangler (generic args
    and impl paths are dropped)."""
    parts, i = [], 2
    while i < len(name):
        m = _V0_TOKEN.match(name, i)
        i = m.end()
        if m.group(1):
            n = int(m.group(1))
            if name[i:i + 1] == "_":  # v0: separator before a `_`-leading ident
                i += 1
            ident = name[i:i + n]
            if ident and (ident[0].isalpha() or ident[0] == "_"):
                parts.append(ident)
                i += n
    return "::".join(parts) or name


def demangle(name):
    """Just enough Rust demangling to make symbol names readable."""
    if name.startswith("_R"):
        return demangle_v0(name)
    if not name.startswith("_ZN"):
        return name
    parts, i = [], 3
    while i < len(name) and name[i].isdigit():
        j = i
        while name[j].isdigit():
            j += 1
        n = int(name[i:j])
        parts.append(name[j:j + n])
        i = j + n
    if parts and re.fullmatch(r"h[0-9a-f]{16}", parts[-1]):
        parts.pop()
    return "::".join(p.lstrip("_") if p.startswith("_$") else p for p in parts)


def app_partition_bytes(csv_path):
    """Size of the smallest app partition — the limit a firmware image has
    to fit (factory/ota_N slots are normally all the same size)."""
    try:
        sizes = []
        with open(csv_path, newline="") as f:
            for row in csv.reader(f):
                row = [c.strip() for c in row]
                if len(row) >= 5 and row[0] and not row[0].startswith("#") \
                        and row[1] == "app":
                    sizes.append(int(row[4], 0))
        return min(sizes)
    except (OSError, ValueError):
        return DEFAULT_APP_PARTITION


def measure(args):
    sections, symbols = read_sections_and_symbols(args.elf)
    alloc = [s for s in sections if s["flags"] & SHF_ALLOC and s["size"]]

    m = dict(flash_code=0, flash_rodata=0, iram=0, dram_data=0, dram_bss=0,
             rtc=0, stack_budget=0, flash_image=0)
    ram_sections = {bank: [] for bank in RAM_BANKS}
    for s in alloc:
        size, addr = s["size"], s["addr"]
        for bank, ranges in RAM_BANKS.items():
            if any(start <= addr and addr + size <= end for start, end in ranges):
                ram_sections[bank].append(dict(name=s["name"], addr=addr, size=size))
                break
        if s["name"] == ".stack":
            m["stack_budget"] += size
        elif s["type"] == SHT_PROGBITS:
            m["flash_image"] += size  # all of these are stored in flash
            if inside(addr, IROM):
                m["flash_code"] += size
            elif inside(addr, DROM):
                m["flash_rodata"] += size
            elif inside(addr, IRAM):
                m["iram"] += size
            elif inside(addr, DRAM):
                m["dram_data"] += size
            elif any(inside(addr, r) for r in RTC_FAST) or inside(addr, RTC_SLOW):
                m["rtc"] += size
        elif s["type"] == SHT_NOBITS:
            if inside(addr, DRAM):
                m["dram_bss"] += size
            elif any(inside(addr, r) for r in RTC_FAST) or inside(addr, RTC_SLOW):
                m["rtc"] += size

    m["dram_static"] = m["dram_data"] + m["dram_bss"]
    m["dram_total"] = DRAM[1] - DRAM[0]
    m["iram_total"] = IRAM[1] - IRAM[0]
    m["app_partition"] = app_partition_bytes(args.partitions) if args.partitions \
        else DEFAULT_APP_PARTITION
    m["ram_sections"] = ram_sections
    m["sram1_total"] = SRAM1_SIZE
    m["sram1_rom"] = SRAM1_ROM
    m["sram1_elf"] = sum(section["size"] for section in ram_sections["SRAM1"])
    m["sram1_sections"] = {
        section["name"]: section["size"] for section in ram_sections["SRAM1"]
        if section["name"] in SRAM1_SLOTS
    }
    if args.name == "gateway-hardware":
        m["heap_reserved"] = m["sram1_sections"].get(".dram2_uninit", 0)
    if args.image:
        m["flash_bin"] = os.path.getsize(args.image)
        if m["flash_bin"] > m["app_partition"]:
            sys.exit(f"{args.image}: {m['flash_bin']} bytes exceed the app partition ({m['app_partition']} bytes)")

    ram_names = {s["name"] for s in alloc
                 if inside(s["addr"], DRAM) and s["name"] != ".stack"}
    top = sorted((y for y in symbols if y["section"] in ram_names),
                 key=lambda y: -y["size"])[:8]
    m["top_ram"] = [dict(name=demangle(y["name"]), size=y["size"],
                         section=y["section"]) for y in top]

    out = dict(name=args.name, profile=args.profile, **m)
    with open(args.out, "w") as f:
        json.dump(out, f, indent=2)
    print(f"{args.name} ({args.profile}): flash {m['flash_image']:,} B, "
          f"static DRAM {m['dram_static']:,} B, "
          f"stack budget {m['stack_budget']:,} B")


# ---------------------------------------------------------------- report

def load(directory):
    out = {}
    for p in sorted(glob.glob(os.path.join(directory, "**", "*.json"),
                              recursive=True)):
        with open(p) as f:
            d = json.load(f)
        if "flash_image" in d:
            out[(d["name"], d["profile"])] = d
    return out


def kib(n):
    return f"{n / 1024:.1f} KiB"


def pct(n, total):
    return f"{100 * n / total:.1f} %"


def bar(n, total, width=12):
    filled = round(width * n / total) if total else 0
    return "█" * filled + "░" * (width - filled)


def delta(cur, base, key):
    if base is None or key not in cur or key not in base:
        return ""
    d = cur[key] - base[key]
    if d == 0:
        return " (±0)"
    return f" ({'+' if d > 0 else '−'}{abs(d):,} B)"


def report(args):
    cur = load(args.metrics)
    if not cur:
        sys.exit("no metrics found")
    base = load(args.baseline) if args.baseline and os.path.isdir(args.baseline) else {}

    out = ["## Firmware memory footprint", ""]
    if base:
        out += ["Δ is against the latest successful run on the base branch.", ""]
    else:
        out += ["_No baseline from the base branch available — no Δ shown._", ""]

    out += ["| firmware | profile | application / slot | IRAM | SRAM1 ELF | heap reserved | SRAM2 statics | "
            "stack budget (DRAM left) |",
            "|---|---|---|---|---|---|---|---|"]
    for (name, profile), m in sorted(cur.items()):
        b = base.get((name, profile))
        flash_key = "flash_bin" if "flash_bin" in m else "flash_image"
        flash = m[flash_key]
        sram1 = kib(m["sram1_elf"]) if "sram1_elf" in m else "—"
        heap = kib(m["heap_reserved"]) if "heap_reserved" in m else "—"
        out.append(
            f"| `{name}` | {profile} "
            f"| {kib(flash)} / {kib(m['app_partition'])} "
            f"({pct(flash, m['app_partition'])}) "
            f"`{bar(flash, m['app_partition'])}`"
            f"{delta(m, b, flash_key)} "
            f"| {kib(m['iram'])} ({pct(m['iram'], m['iram_total'])})"
            f"{delta(m, b, 'iram')} "
            f"| {sram1}{delta(m, b, 'sram1_elf')} | {heap}{delta(m, b, 'heap_reserved')} "
            f"| {kib(m['dram_static'])} / {kib(m['dram_total'])} "
            f"({pct(m['dram_static'], m['dram_total'])}) "
            f"`{bar(m['dram_static'], m['dram_total'])}`"
            f"{delta(m, b, 'dram_static')} "
            f"| {kib(m['stack_budget'])} ({pct(m['stack_budget'], m['dram_total'])})"
            f"{delta(m, b, 'stack_budget')} |")
    out += [
        "",
        "_Static DRAM covers statics in the primary SRAM2 linker range. "
        "Stack budget is the space the linker reserves for the main stack. "
        "Heap and stack usage during a workload require runtime measurements._",
        "",
    ]

    release = {k: v for k, v in cur.items() if k[1] == "release"} or cur
    names = [k[0] for k in sorted(release)]
    chart = lambda title, key, limit_key, fmt: [
        "```mermaid", "xychart-beta", f'    title "{title}"',
        "    x-axis [" + ", ".join(f'"{n}"' for n in names) + "]",
        f'    y-axis "KiB" 0 --> {fmt(max(release[k][limit_key] for k in sorted(release)))}',
        "    bar [" + ", ".join(f"{release[k][key] / 1024:.1f}"
                                for k in sorted(release)) + "]",
        "```", ""]
    to_kib = lambda n: int(n // 1024)
    out += chart("Flash image vs. app partition (KiB, release)",
                 "flash_image", "app_partition", to_kib)
    out += chart("Static DRAM vs. data RAM (KiB, release)",
                 "dram_static", "dram_total", to_kib)
    out += chart("Stack budget — DRAM left over (KiB, release)",
                 "stack_budget", "dram_total", to_kib)

    for (name, profile), m in sorted(cur.items()):
        if profile != "release" and release is not cur:
            continue
        out += [f"<details><summary>Largest static RAM objects — "
                f"<code>{name}</code> ({profile})</summary>", "",
                "| size | section | symbol |", "|---|---|---|"]
        for t in m["top_ram"]:
            out.append(f"| {kib(t['size'])} | `{t['section']}` | `{t['name']}` |")
        out += ["", "</details>", ""]

    print("\n".join(out))


# --------------------------------------------------------------- history

HISTORY_KEYS = ("flash_image", "iram", "dram_static", "stack_budget",
                "app_partition", "dram_total")
HISTORY_DETAILS = ("ram_sections", "top_ram", "flash_bin", "sram1_total", "sram1_rom",
                   "sram1_elf", "sram1_sections", "heap_reserved", "iram_total")
HISTORY_LIMIT = 500  # entries kept; a commit per entry


def history(args):
    """Append this run's metrics to a history.json (one entry per commit)."""
    entries = []
    if os.path.exists(args.history):
        with open(args.history) as f:
            entries = json.load(f)
    entries = [e for e in entries if e["sha"] != args.sha]  # re-run: replace
    firmware = {}
    for (name, profile), metrics in sorted(load(args.metrics).items()):
        summary = {key: metrics[key] for key in HISTORY_KEYS}
        summary.update({key: metrics[key] for key in HISTORY_DETAILS if key in metrics})
        firmware[f"{name}/{profile}"] = summary
    entries.append(dict(
        sha=args.sha, date=args.date, subject=args.subject,
        firmware=firmware))
    entries.sort(key=lambda e: e["date"])
    with open(args.history, "w") as f:
        json.dump(entries[-HISTORY_LIMIT:], f, indent=1)
    print(f"history: {len(entries[-HISTORY_LIMIT:])} entries")


CHARTS = [
    ("flash_image", "Flash sections in ELF", "app_partition",
     "of the app partition"),
    ("flash_bin", "Flash application binary", "app_partition", "of the app partition"),
    ("sram1_elf", "SRAM1 ELF sections", "sram1_total", "of the 128 KiB bank"),
    ("heap_reserved", "Heap reservation in SRAM1", "sram1_total", "of the 128 KiB bank"),
    ("dram_static", "SRAM2 statics", "dram_total", "of the SRAM2 linker range"),
    ("stack_budget", "SRAM2 stack reservation", "dram_total", "of the SRAM2 linker range"),
    ("iram", "SRAM0 IRAM", None, ""),
]
PALETTE = ["#2a6fdb", "#e0792a", "#2f9e6e", "#a14fc9"]


def svg_chart(entries, firmware, key, title, cap_key, cap_note):
    series = {}
    for i, e in enumerate(entries):
        for fw, m in e["firmware"].items():
            if fw.rsplit("/", 1)[0] == firmware and m.get(key) is not None:
                series.setdefault(fw.rsplit("/", 1)[1], []).append((i, m))
    if not series:
        return ""
    W, H, L, R, T, B = 760, 232, 62, 40, 34, 48
    vals = [m[key] for pts in series.values() for _, m in pts]
    lo, hi = min(vals), max(vals)
    pad = max((hi - lo) * 0.15, hi * 0.01, 1)
    lo, hi = max(0, lo - pad), hi + pad
    points = sorted({i for pts in series.values() for i, _ in pts})
    first, last = points[0], points[-1]
    x = lambda i: L + (W - L - R) * ((i - first) / (last - first) if last > first else 0.5)
    y = lambda v: T + (H - T - B) * (1 - (v - lo) / (hi - lo))
    o = [f'<svg viewBox="0 0 {W} {H}" role="img" aria-label="{title}">',
         f'<text x="{L}" y="20" class="t">{title} (KiB)</text>']
    for k in range(5):
        v = lo + (hi - lo) * k / 4
        o.append(f'<line x1="{L}" x2="{W - R}" y1="{y(v):.1f}" y2="{y(v):.1f}" '
                 f'class="g"/><text x="{L - 6}" y="{y(v) + 4:.1f}" class="a" '
                 f'text-anchor="end">{v / 1024:.1f}</text>')
    ticks = [first]
    spacing = (last - first) / 6
    for i in points[1:-1]:
        if i - ticks[-1] >= spacing and last - i >= spacing:
            ticks.append(i)
    if last != first:
        ticks.append(last)
    for i in ticks:
        date = escape(entries[i]["date"][:10])
        sha = escape(entries[i]["sha"][:7])
        o.append(f'<text x="{x(i):.1f}" y="{H - 26}" class="a" '
                 f'text-anchor="middle">{date}</text>')
        o.append(f'<text x="{x(i):.1f}" y="{H - 12}" class="a hash" '
                 f'text-anchor="middle">{sha}</text>')
    for ci, (name, pts) in enumerate(sorted(series.items())):
        col = PALETTE[ci % len(PALETTE)]
        o.append(f'<text x="{W - R - 164 + ci * 90}" y="20" class="a" '
                 f'style="fill:{col}">{escape(name)}</text>')
        path = []
        prev = -2
        for i, m in pts:
            command = "L" if i == prev + 1 else "M"
            path.append(f"{command}{x(i):.1f},{y(m[key]):.1f}")
            prev = i
        o.append(f'<path d="{" ".join(path)}" fill="none" stroke="{col}" stroke-width="2"/>')
        for i, m in pts:
            cap = m.get(cap_key) if cap_key else None
            share = f" — {100 * m[key] / cap:.1f} % {cap_note}" if cap else ""
            o.append(
                f'<circle cx="{x(i):.1f}" cy="{y(m[key]):.1f}" r="3.5" fill="{col}">'
                f'<title>{escape(name)}: {m[key]:,} B ({m[key] / 1024:.1f} KiB){share}\n'
                f'{escape(entries[i]["date"][:10])} · {escape(entries[i]["sha"][:7])}\n'
                f'{escape(entries[i]["subject"])}</title></circle>')
    o.append("</svg>")
    return "\n".join(o)


def ram_bank_map():
    rows = []
    for bank, ranges in RAM_BANKS.items():
        addresses = "<br>".join(f"<code>0x{start:08X}–0x{end - 1:08X}</code>"
                                for start, end in ranges)
        start, end = ranges[0]
        rows.append(f"<tr><th scope=\"row\">{bank}</th><td>{(end - start) // 1024} KiB</td>"
                    f"<td>{addresses}</td><td>{BANK_USES[bank]}</td></tr>")
    return "".join(rows)


def ram_layout(firmware, metrics):
    title = escape(firmware)
    if "ram_sections" not in metrics:
        return f"<p><code>{title}</code>. Section data is unavailable for this build.</p>"
    rows = []
    for bank, ranges in RAM_BANKS.items():
        sections = sorted(metrics["ram_sections"].get(bank, []), key=lambda s: s["addr"])
        start, end = ranges[0]
        total = sum(section["size"] for section in sections)
        group = [f'<tr class="bank"><th scope="rowgroup" colspan="4">{bank} · '
                 f'{kib(total)} in ELF of {(end - start) // 1024} KiB physical</th></tr>']
        if not sections:
            group.append('<tr><td colspan="4">No allocated ELF sections</td></tr>')
        for section in sections:
            name = section["name"]
            purpose = SECTION_USES.get(name, "")
            if name == ".dram2_uninit":
                purpose = ("Gateway heap reservation" if firmware.startswith("gateway-hardware/")
                           else "Static data left uninitialized at boot")
            if name in SRAM1_SLOTS:
                slot_start, slot_end = SRAM1_SLOTS[name]
                purpose += f" ({kib(slot_end - slot_start)} linker range)"
            group.append(f'<tr><th scope="row"><code>{escape(name)}</code></th>'
                         f'<td><code>0x{section["addr"]:08X}</code></td>'
                         f'<td>{kib(section["size"])}</td><td>{purpose}</td></tr>')
        rows.append("<tbody>" + "".join(group) + "</tbody>")
    opened = " open" if firmware == "gateway-hardware/release" else ""
    objects = ""
    if metrics.get("top_ram"):
        largest = "".join(
            f'<tr><th scope="row"><code>{escape(obj["name"])}</code></th>'
            f'<td><code>{escape(obj["section"])}</code></td><td>{kib(obj["size"])}</td></tr>'
            for obj in metrics["top_ram"])
        objects = ('<details><summary>Largest static objects in SRAM2</summary>'
                   '<div class="table-scroll"><table class="memory-map">'
                   '<thead><tr><th>Symbol</th><th>Section</th><th>Size</th></tr></thead>'
                   f'<tbody>{largest}</tbody></table></div></details>')
    return (f'<details{opened}><summary><code>{title}</code></summary>'
            '<div class="table-scroll"><table class="memory-map"><thead>'
            '<tr><th>ELF section</th><th>Start address</th><th>Size</th><th>Purpose</th></tr>'
            f'</thead>{"".join(rows)}</table></div>{objects}</details>')


def read_optional_json(path):
    if not path:
        return None
    try:
        with open(path) as file:
            return json.load(file)
    except (OSError, ValueError):
        return None


def number_cell(value, display=None):
    key = "" if value is None else str(value)
    if display is None:
        display = "—" if value is None else f"{value:,}"
    return f'<td class="number" data-sort="{escape(key)}">{escape(display)}</td>'


def runtime_report(runtime, run):
    info = ""
    if run and type(run.get("databaseId")) is int:
        url = f"https://github.com/Th3Link/homeautomation/actions/runs/{run['databaseId']}"
        conclusion = escape(str(run.get("conclusion") or "unknown"))
        sha = escape(str(run.get("headSha", ""))[:12])
        info = f'<p><a href="{url}">Latest CI run</a> · {conclusion} · <code>{sha}</code></p>'
    if not runtime or runtime.get("schema") != 1:
        return f'{info}<p>No runtime measurements are available for this run.</p>'

    platform = escape(str(runtime.get("platform", "—")))
    firmware = escape(str(runtime.get("firmware_sha", "—"))[:12])
    revision = str(runtime.get("firmware_sha", ""))
    dirty = " with local changes" if runtime.get("source_dirty") else ""
    profile = escape(str(runtime.get("build_profile", "—")))
    qemu = escape(str(runtime.get("qemu_revision", "—"))[:12])
    broker = escape(str(runtime.get("broker_version") or "—"))
    date = escape(str(runtime.get("date", "—")))
    info += (f'<p>{platform} · firmware <code>{firmware}</code>{dirty} ({profile}) · QEMU <code>{qemu}</code>'
             f'<br>{broker} · {date}</p>')
    rows = []
    measures = ("heap_size", "heap_peak_used", "heap_min_free", "stack_size",
                "stack_peak_used", "stack_untouched")
    for item in runtime.get("profiles", []):
        status = str(item["status"])
        state = status + (" / partial" if item.get("partial") else "")
        tone = "partial" if item.get("partial") else "neutral"
        if status in ("failed", "aborted"):
            tone = "failed"
        elif status == "passed" and not item.get("partial"):
            tone = "passed"
        label = escape(f'{item["scenario"]} #{item["run"]}')
        test = str(item.get("test", "")).split("::", 1)[0]
        if re.fullmatch(r"[0-9a-f]{40}", revision) and re.fullmatch(r"tests/\w+\.py", test):
            label = f'<a href="https://github.com/Th3Link/homeautomation/blob/{revision}/software/sim/{test}">{label}</a>'
        cells = f'<th scope="row">{label}</th><td><span class="status {tone}">{escape(state)}</span></td>'
        gateways = [(g["name"], g["boots"]) for g in item.get("gateways", [])]
        if item.get("rest"):
            errors = len(item["rest"]["errors"])
            gateways.append((f"REST ({errors} errors)", item["rest"]["boots"]))
        for gateway, boots in gateways or [("—", [])]:
            for n, boot in enumerate(boots or [{}], 1):
                chip = f'{gateway} / boot {n}' if boot else gateway
                if boot.get("panic"):
                    chip += " (panic)"
                count = boot.get("samples") or None
                coverage = (f'{count}, {boot["start_uptime_ms"] / 1000:.1f}–'
                            f'{boot["end_uptime_ms"] / 1000:.1f} s') if count else None
                values = "".join(number_cell(boot.get(key)) for key in measures)
                rows.append(f'<tr>{cells}<td>{escape(chip)}</td>{number_cell(count, coverage)}{values}</tr>')

    return f"""{info}
<p>Bytes per boot, including startup. Coverage gives the number of samples and their
firmware uptime range. Partial rows can end before the last peak. Repeats are shown separately.</p>
<div class="table-scroll data-scroll runtime-scroll" role="region" aria-labelledby="runtime" tabindex="0">
<table class="data-table" data-sortable aria-labelledby="runtime"><thead><tr>
<th scope="col">Scenario</th><th scope="col">Result</th><th scope="col">Gateway</th>
<th scope="col" class="number">Coverage</th><th scope="col" class="number">Heap size</th>
<th scope="col" class="number">Heap peak</th><th scope="col" class="number" title="Heap minimum free">Heap min. free</th>
<th scope="col" class="number">Stack budget</th><th scope="col" class="number" title="Observed stack peak">Stack peak</th>
<th scope="col" class="number">Stack untouched</th></tr></thead>
<tbody>{''.join(rows)}</tbody></table></div>"""


def chart(args):
    entries = read_optional_json(args.history) or []
    latest = entries[-1] if entries else {"sha": "—", "firmware": {}}
    def kib_cell(value):
        return number_cell(value, f"{value / 1024:.1f}" if value is not None else None)

    static_metrics = ("flash_image", "iram", "sram1_elf", "heap_reserved", "dram_static", "stack_budget")
    rows = "".join(
        f'<tr><th scope="row"><code>{escape(fw)}</code></th>'
        + number_cell(m.get("flash_bin"),
                      f'{m["flash_bin"] / 1024:.1f} / {m["app_partition"] / 1024:.0f}'
                      if m.get("flash_bin") is not None else f'— / {m["app_partition"] / 1024:.0f}')
        + "".join(kib_cell(m.get(key)) for key in static_metrics)
        + "</tr>" for fw, m in sorted(latest["firmware"].items()))
    charts = []
    for name in ("gateway-hardware", "cancomponents-hardware"):
        figures = "\n".join(filter(None, (svg_chart(entries, name, *c) for c in CHARTS)))
        charts.append(f'<h3 id="trends-{name}"><code>{name}</code></h3>\n' +
                      (figures or '<p>No builds have been recorded for this firmware yet.</p>'))
    layouts = "\n".join(ram_layout(fw, metrics) for fw, metrics in sorted(latest["firmware"].items()))
    static_data = f"""<h2 id="latest-build">Latest ({escape(latest["sha"][:7])}, KiB)</h2>
<div class="table-scroll data-scroll" role="region" aria-labelledby="latest-build" tabindex="0">
<table class="data-table" data-sortable aria-labelledby="latest-build"><thead><tr>
<th scope="col">Firmware</th><th scope="col" class="number">Flash binary / slot</th>
<th scope="col" class="number">Flash ELF</th><th scope="col" class="number">SRAM0 IRAM</th>
<th scope="col" class="number">SRAM1 ELF</th><th scope="col" class="number">SRAM1 heap</th>
<th scope="col" class="number">SRAM2 statics</th><th scope="col" class="number">SRAM2 stack budget</th>
</tr></thead><tbody>{rows}</tbody></table></div>
<p>The heap reservation is part of SRAM1 ELF usage. The ESP32 reserves another {SRAM1_ROM:,} B in SRAM1 for ROM data.
Older builds may have missing measurements.</p>
<h3>RAM sections by firmware and build profile</h3>
<p>The bank totals cover the ELF sections below. Cache and ROM reservations
are listed in the memory map above.</p>
{layouts}
<h2>Trends</h2>
{''.join(charts)}""" if entries else '<p>No static history is available yet.</p>'
    runtime_data = runtime_report(read_optional_json(args.runtime), read_optional_json(args.runtime_run))
    css = Path(__file__).with_suffix(".css").read_text(encoding="utf-8")
    js = Path(__file__).with_suffix(".js").read_text(encoding="utf-8")
    html = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Firmware memory history</title>
<style>{css}</style></head><body>
<p><a href="../">← Handbook</a></p>
<h1>Firmware memory history</h1>
<p>One point per commit on <code>main</code>. The ELF shows where the firmware
places code and data and how much space the linker reserves for heap and
stack. Usage within those reservations depends on the workload.</p>
<h2>ESP32 memory map</h2>
<div class="table-scroll"><table class="memory-map address-map">
<thead><tr><th>Bank</th><th>Physical size</th><th>Address range</th><th>Use</th></tr></thead>
<tbody>{ram_bank_map()}</tbody></table></div>
<p>The 520 KiB of internal SRAM comes from SRAM0 (192 KiB), SRAM1 (128 KiB),
and SRAM2 (200 KiB). RTC memory is separate. Most code and read-only data
are mapped from flash. See the
<a href="https://www.espressif.com/sites/default/files/documentation/esp32_technical_reference_manual_en.pdf">ESP32 technical reference manual</a>
and the
<a href="https://github.com/Th3Link/homeautomation/blob/main/software/gateway-core/docs/ram.md">gateway linker and RAM notes</a>.</p>
{static_data}
<h2 id="runtime">Gateway runtime profiles</h2>
{runtime_data}
<script>{js}</script>
</body></html>"""
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w") as f:
        f.write(html)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    m = sub.add_parser("measure")
    m.add_argument("elf")
    m.add_argument("--name", required=True)
    m.add_argument("--profile", required=True)
    m.add_argument("--partitions")
    m.add_argument("--image", help="raw application binary produced from this ELF")
    m.add_argument("--out", required=True)
    m.set_defaults(fn=measure)

    r = sub.add_parser("report")
    r.add_argument("--metrics", required=True)
    r.add_argument("--baseline")
    r.set_defaults(fn=report)

    h = sub.add_parser("history")
    h.add_argument("--history", required=True)
    h.add_argument("--metrics", required=True)
    h.add_argument("--sha", required=True)
    h.add_argument("--date", required=True)
    h.add_argument("--subject", default="")
    h.set_defaults(fn=history)

    c = sub.add_parser("chart")
    c.add_argument("--history")
    c.add_argument("--runtime")
    c.add_argument("--runtime-run")
    c.add_argument("--out", required=True)
    c.set_defaults(fn=chart)

    args = ap.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
