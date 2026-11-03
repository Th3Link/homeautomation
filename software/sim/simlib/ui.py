"""Terminal output: colours (off without a terminal or with NO_COLOR),
tables and durations."""

import os
import re
import sys
import zlib

COLOR = sys.stdout.isatty() and "NO_COLOR" not in os.environ
ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
CHIP_COLORS = (33, 36, 35, 32, 34, 93, 96, 95, 92, 94)


def style(text, *codes):
    if not COLOR or not codes:
        return str(text)
    return f"\x1b[{';'.join(map(str, codes))}m{text}\x1b[0m"


def bold(t):
    return style(t, 1)


def dim(t):
    return style(t, 2)


def red(t):
    return style(t, 31)


def green(t):
    return style(t, 32)


def yellow(t):
    return style(t, 33)


def cyan(t):
    return style(t, 36)


def chip_name(name, gateway=False, width=0):
    color = CHIP_COLORS[zlib.crc32(name.encode()) % len(CHIP_COLORS)]
    return style(name.ljust(width), color, 1) if gateway else style(name.ljust(width), color)


def strip(text):
    return ANSI.sub("", text)


def table(headers, rows):
    """Left-aligned columns; cells may carry colour codes."""
    widths = [len(h) for h in headers]
    for row in rows:
        for i, cell in enumerate(row):
            widths[i] = max(widths[i], len(strip(str(cell))))

    def line(cells):
        out = []
        for i, cell in enumerate(cells):
            cell = str(cell)
            out.append(cell + " " * (widths[i] - len(strip(cell))))
        return "  ".join(out).rstrip()

    print(dim(line(headers)))
    for row in rows:
        print(line(row))


def duration(seconds):
    seconds = int(seconds)
    if seconds < 60:
        return f"{seconds}s"
    if seconds < 3600:
        return f"{seconds // 60}m{seconds % 60:02d}s"
    return f"{seconds // 3600}h{seconds // 60 % 60:02d}m"


def die(message, hint=None):
    print(red("✗ ") + message, file=sys.stderr)
    if hint:
        print("  " + hint, file=sys.stderr)
    sys.exit(1)
