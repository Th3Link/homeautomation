#!/usr/bin/env python3
"""Memory footprint of ESP32 (Xtensa) firmware ELFs, for CI.

Two subcommands, no dependencies beyond the Python standard library (it
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
  static DRAM   .data + .bss + .noinit (+ RTC): everything the firmware
                statically allocates in data RAM — including embassy task
                futures and esp-alloc's heap, which is a static array.
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

SHF_ALLOC = 0x2
SHT_PROGBITS = 1
SHT_SYMTAB = 2
SHT_NOBITS = 8
STT_OBJECT = 1

# ESP32 address map (ESP32 TRM, "System and Memory").
DRAM = (0x3FFB0000, 0x3FFE0000)  # data RAM region esp-hal links into
IRAM = (0x40080000, 0x400A0000)
IROM = (0x400D0000, 0x40400000)  # flash-mapped code
DROM = (0x3F400000, 0x3F800000)  # flash-mapped read-only data
RTC_FAST = [(0x3FF80000, 0x3FF82000), (0x400C0000, 0x400C2000)]
RTC_SLOW = (0x50000000, 0x50002000)

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
    for s in alloc:
        size, addr = s["size"], s["addr"]
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
    if base is None:
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

    out += ["| firmware | profile | flash image | IRAM | static DRAM | "
            "stack budget (DRAM left) |",
            "|---|---|---|---|---|---|"]
    for (name, profile), m in sorted(cur.items()):
        b = base.get((name, profile))
        out.append(
            f"| `{name}` | {profile} "
            f"| {kib(m['flash_image'])} / {kib(m['app_partition'])} "
            f"({pct(m['flash_image'], m['app_partition'])}) "
            f"`{bar(m['flash_image'], m['app_partition'])}`"
            f"{delta(m, b, 'flash_image')} "
            f"| {kib(m['iram'])} ({pct(m['iram'], m['iram_total'])})"
            f"{delta(m, b, 'iram')} "
            f"| {kib(m['dram_static'])} / {kib(m['dram_total'])} "
            f"({pct(m['dram_static'], m['dram_total'])}) "
            f"`{bar(m['dram_static'], m['dram_total'])}`"
            f"{delta(m, b, 'dram_static')} "
            f"| {kib(m['stack_budget'])} ({pct(m['stack_budget'], m['dram_total'])})"
            f"{delta(m, b, 'stack_budget')} |")
    out += [
        "",
        "_Static DRAM = `.data` + `.bss` (+ RTC), including esp-alloc's heap "
        "and embassy task futures. Stack budget = the DRAM esp-hal's linker "
        "script leaves over for the stack; it is **not** measured peak stack "
        "usage, which can't be derived from the ELF. Static DRAM + stack "
        "budget always add up to the whole data-RAM region._",
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
HISTORY_LIMIT = 500  # entries kept; a commit per entry


def history(args):
    """Append this run's metrics to a history.json (one entry per commit)."""
    entries = []
    if os.path.exists(args.history):
        with open(args.history) as f:
            entries = json.load(f)
    entries = [e for e in entries if e["sha"] != args.sha]  # re-run: replace
    entries.append(dict(
        sha=args.sha, date=args.date, subject=args.subject,
        firmware={f"{n}/{p}": {k: m[k] for k in HISTORY_KEYS}
                  for (n, p), m in sorted(load(args.metrics).items())}))
    entries.sort(key=lambda e: e["date"])
    with open(args.history, "w") as f:
        json.dump(entries[-HISTORY_LIMIT:], f, indent=1)
    print(f"history: {len(entries[-HISTORY_LIMIT:])} entries")


CHARTS = [
    ("flash_image", "Flash image", "app_partition",
     "of the app partition"),
    ("dram_static", "Static DRAM (.data + .bss)", "dram_total",
     "of data RAM"),
    ("stack_budget", "Stack budget (DRAM left over)", "dram_total",
     "of data RAM"),
    ("iram", "IRAM", None, ""),
]
PALETTE = ["#2a6fdb", "#e0792a", "#2f9e6e", "#a14fc9"]


def svg_chart(entries, firmware, key, title, cap_key, cap_note):
    series = {}
    for i, e in enumerate(entries):
        for fw, m in e["firmware"].items():
            if fw == f"{firmware}/release":
                series.setdefault(firmware, []).append((i, m))
    if not series:
        return ""
    W, H, L, R, T, B = 760, 220, 62, 20, 34, 34
    vals = [m[key] for pts in series.values() for _, m in pts]
    lo, hi = min(vals), max(vals)
    pad = max((hi - lo) * 0.15, hi * 0.01, 1)
    lo, hi = max(0, lo - pad), hi + pad
    n = max(len(entries) - 1, 1)
    x = lambda i: L + (W - L - R) * i / n
    y = lambda v: T + (H - T - B) * (1 - (v - lo) / (hi - lo))
    o = [f'<svg viewBox="0 0 {W} {H}" role="img" aria-label="{title}">',
         f'<text x="{L}" y="20" class="t">{title} (KiB, release)</text>']
    for k in range(5):
        v = lo + (hi - lo) * k / 4
        o.append(f'<line x1="{L}" x2="{W - R}" y1="{y(v):.1f}" y2="{y(v):.1f}" '
                 f'class="g"/><text x="{L - 6}" y="{y(v) + 4:.1f}" class="a" '
                 f'text-anchor="end">{v / 1024:.1f}</text>')
    step = max(1, len(entries) // 6)
    for i, e in enumerate(entries):
        if i % step == 0 or i == len(entries) - 1:
            o.append(f'<text x="{x(i):.1f}" y="{H - 12}" class="a" '
                     f'text-anchor="middle">{e["date"][5:10]}</text>')
    for ci, (name, pts) in enumerate(sorted(series.items())):
        col = PALETTE[ci % len(PALETTE)]
        path = " ".join(f"{x(i):.1f},{y(m[key]):.1f}" for i, m in pts)
        o.append(f'<polyline points="{path}" fill="none" stroke="{col}" '
                 f'stroke-width="2"/>')
        for i, m in pts:
            share = f" — {100 * m[key] / m[cap_key]:.1f} % {cap_note}" if cap_key else ""
            o.append(
                f'<circle cx="{x(i):.1f}" cy="{y(m[key]):.1f}" r="3.5" fill="{col}">'
                f'<title>{name}: {m[key]:,} B ({m[key] / 1024:.1f} KiB){share}\n'
                f'{entries[i]["sha"][:7]} {entries[i]["subject"]}</title></circle>')
    o.append("</svg>")
    return "\n".join(o)


def chart(args):
    with open(args.history) as f:
        entries = json.load(f)
    latest = entries[-1]
    rows = "".join(
        f"<tr><td><code>{fw}</code></td>"
        + "".join(f"<td>{m[k] / 1024:.1f}</td>" for k in
                  ("flash_image", "iram", "dram_static", "stack_budget"))
        + "</tr>" for fw, m in sorted(latest["firmware"].items()))
    names = sorted({fw.split("/")[0] for e in entries for fw in e["firmware"]})
    charts = "\n".join(
        f"<h2><code>{n}</code></h2>\n"
        + "\n".join(svg_chart(entries, n, *c) for c in CHARTS)
        for n in names)
    html = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Firmware memory history</title>
<style>
:root {{ color-scheme: light dark; --fg:#1d2330; --bg:#fff; --mut:#6b7385; --grid:#e3e6ee; }}
@media (prefers-color-scheme: dark) {{ :root {{ --fg:#e6e9f0; --bg:#14171f; --mut:#9aa2b5; --grid:#2a2f3c; }} }}
body {{ font:15px/1.5 system-ui,sans-serif; color:var(--fg); background:var(--bg);
       max-width:820px; margin:0 auto; padding:24px 16px; }}
svg {{ width:100%; height:auto; margin:8px 0 20px; }}
.t {{ fill:var(--fg); font-weight:600; font-size:14px; }}
.a {{ fill:var(--mut); font-size:11px; }} .g {{ stroke:var(--grid); }}
table {{ border-collapse:collapse; }} td,th {{ padding:4px 12px; text-align:right; }}
th:first-child,td:first-child {{ text-align:left; }}
</style></head><body>
<h1>Firmware memory history</h1>
<p>One point per commit on <code>main</code> (hover for details). Static DRAM +
stack budget always add up to the whole data-RAM region; the stack budget is
what esp-hal's linker script leaves over, not measured stack usage.</p>
<h2>Latest ({latest["sha"][:7]}, KiB)</h2>
<table><tr><th>firmware</th><th>flash</th><th>IRAM</th><th>static DRAM</th><th>stack budget</th></tr>{rows}</table>
{charts}
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
    c.add_argument("--history", required=True)
    c.add_argument("--out", required=True)
    c.set_defaults(fn=chart)

    args = ap.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
