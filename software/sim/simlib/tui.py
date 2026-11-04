"""`sim tui`: the running simulation in one screen. Chips on the left, the
selected one in detail on the right (buttons, PIR, relays, every part with
its values), the events below. Keys do what the `sim` commands do.
"""

import curses
import locale
import re
import shlex
import threading
import time
import unicodedata
import webbrowser
from collections import deque

from . import console
from .client import Sim, SimError

HELP = [
    ("↑ ↓  j k", "choose a chip"),
    ("0-7", "press that button (single)"),
    ("d t h", "then 0-7: double, triple, hold 3 s"),
    ("m  M", "motion for 10 s / 60 s"),
    ("← →  Tab", "choose a value of a part"),
    ("+ -", "change it by one step"),
    ("Enter", "type a value"),
    ("c", "the chip's console (Ctrl-] comes back)"),
    ("r", "restart the chip"),
    ("w", "open the gateway's web UI"),
    ("f", "events: all chips / this chip"),
    ("s  i  a", "show serial log / I2C / CAN housekeeping"),
    ("PgUp PgDn", "scroll the events"),
    (":", "a command: press 0 double, set bme680 temperature=25, motion 30,"),
    ("", "  lux 5, pin 4 0, i2c 0x77 0xd0 1, restart"),
    ("q", "quit (the simulation keeps running)"),
]

EMOJI = re.compile(r"[\U0001F000-\U0001FFFF☀-➿]️?\s*")


def width(text):
    return sum(2 if unicodedata.east_asian_width(c) in "WF" else 0 if unicodedata.combining(c) else 1
               for c in text)


def clip(text, cols):
    out, w = "", 0
    for c in text:
        cw = 2 if unicodedata.east_asian_width(c) in "WF" else 1
        if w + cw > cols:
            break
        out += c
        w += cw
    return out


class Tui:
    def __init__(self, topology):
        self.sim = Sim(topology, history=300)
        self.topo = self.sim.topology
        self.names = [c.name for c in self.topo.chips]
        self.sel = 0
        self.status = {}
        self.events = deque(maxlen=3000)
        self.only_selected = False
        self.show = {"serial": False, "i2c": False, "chatter": False}
        self.scroll = 0
        self.value_sel = 0
        self.modifier = None
        self.message = ""
        self.message_at = 0
        self.help = False
        self.stop = threading.Event()
        self.seen = 0

    # Data

    def _poll(self):
        while not self.stop.is_set():
            try:
                self.status = self.sim.status()
            except SimError as e:
                self.flash(str(e))
            self.stop.wait(1.0)

    def _collect(self):
        while not self.stop.is_set():
            new = self.sim.events(since=self.seen)
            if new:
                self.seen = new[-1]["seq"]
                self.events.extend(new)
            self.stop.wait(0.2)

    def flash(self, text):
        self.message = text
        self.message_at = time.time()

    @property
    def chip(self):
        return self.names[self.sel]

    def visible(self, e):
        kind = e["kind"]
        if self.only_selected and e["chip"] != self.chip:
            return False
        if kind == "serial":
            return self.show["serial"]
        if kind == "i2c":
            return self.show["i2c"]
        if kind == "can" and not e.get("shown", True):
            return self.show["chatter"]
        return True

    def editable(self):
        s = self.status.get(self.chip, {})
        out = []
        for name, part in (s.get("parts") or {}).items():
            for key, (unit, step, lo, hi) in part.get("values", {}).items():
                out.append((name, key, unit, step, lo, hi, part.get(key)))
        return out

    # Actions (in a thread, so the screen stays live)

    def act(self, what, fn, *args, **kw):
        def run():
            try:
                fn(*args, **kw)
                self.flash(f"✓ {what}")
            except Exception as e:  # noqa: BLE001
                self.flash(f"✗ {what}: {e}")
        threading.Thread(target=run, daemon=True).start()

    def press(self, button, kind):
        h = self.sim[self.chip]
        self.act(f"{self.chip} button {button} {kind}", h.press, button, kind)

    def command(self, line):
        try:
            words = shlex.split(line)
        except ValueError as e:
            self.flash(str(e))
            return
        if not words:
            return
        h = self.sim[self.chip]
        cmd, rest = words[0], words[1:]
        try:
            if cmd == "press":
                self.press(int(rest[0]), rest[1] if len(rest) > 1 else "single")
            elif cmd == "motion":
                self.act("motion", h.motion, float(rest[0]) if rest else 10)
            elif cmd == "lux":
                self.act(f"{rest[0]} lx", h.part("veml7700").set, lux=float(rest[0]))
            elif cmd == "set":
                values = dict(v.split("=", 1) for v in rest[1:])
                self.act(f"{rest[0]} {' '.join(rest[1:])}", h.part(rest[0]).set, **values)
            elif cmd == "pin":
                self.act(f"pin {rest[0]} = {rest[1]}", h.pin, int(rest[0]), int(rest[1]))
            elif cmd == "restart":
                self.act(f"{self.chip} restarted", h.restart)
            elif cmd == "i2c":
                addr, reg = int(rest[0], 0), [int(x, 0) for x in rest[1:2]]
                n = int(rest[2], 0) if len(rest) > 2 else 1
                data = h.i2c(addr, bytes(reg), n)
                self.flash(f"i2c 0x{addr:02x} {' '.join(f'{r:02x}' for r in reg)} → {data.hex(' ')}")
            else:
                self.flash(f"unknown command {cmd!r} (? shows the keys)")
        except (IndexError, ValueError) as e:
            self.flash(f"{cmd}: {e or 'missing arguments'}")
        except SimError as e:
            self.flash(str(e))

    # Drawing

    def run(self, scr):
        self.scr = scr
        curses.curs_set(0)
        curses.use_default_colors()
        for i, c in enumerate((curses.COLOR_GREEN, curses.COLOR_YELLOW, curses.COLOR_RED,
                               curses.COLOR_CYAN, curses.COLOR_MAGENTA, curses.COLOR_BLUE), 1):
            curses.init_pair(i, c, -1)
        self.C = {"green": curses.color_pair(1), "yellow": curses.color_pair(2),
                  "red": curses.color_pair(3), "cyan": curses.color_pair(4),
                  "magenta": curses.color_pair(5), "blue": curses.color_pair(6)}
        scr.timeout(150)
        threading.Thread(target=self._poll, daemon=True).start()
        threading.Thread(target=self._collect, daemon=True).start()
        try:
            while True:
                self.draw()
                try:
                    key = scr.get_wch()
                except curses.error:  # no key within the timeout
                    continue
                if self.key(key) == "quit":
                    break
        finally:
            self.stop.set()
            self.sim.close()

    def put(self, y, x, text, attr=0, cols=None):
        h, w = self.scr.getmaxyx()
        if y < 0 or y >= h or x >= w:
            return 0
        cols = min(cols if cols is not None else w - x, w - x - (1 if y == h - 1 else 0))
        text = clip(text, max(0, cols))
        try:
            self.scr.addstr(y, x, text, attr)
        except curses.error:
            pass
        return width(text)

    def draw(self):
        scr = self.scr
        scr.erase()
        h, w = scr.getmaxyx()
        if h < 16 or w < 70:
            self.put(0, 0, "make the terminal at least 70×16")
            scr.refresh()
            return
        running = sum(1 for s in self.status.values() if s.get("state") == "running")
        title = f" sim · {self.topo.path.name} · {running}/{len(self.names)} running "
        self.put(0, 0, " " * w, curses.A_REVERSE)
        self.put(0, 0, title, curses.A_REVERSE | curses.A_BOLD)
        self.put(0, w - 10, time.strftime(" %H:%M:%S "), curses.A_REVERSE)

        left = max(len(n) for n in self.names) + 6
        top_h = max(9, (h - 3) // 2)
        self.draw_chips(2, left, top_h)
        self.draw_detail(2, left + 2, w - left - 3, top_h)
        self.draw_events(top_h + 2, h - top_h - 4, w)
        self.draw_footer(h - 1, w)
        if self.help:
            self.draw_help(h, w)
        scr.refresh()

    def draw_chips(self, y, wdt, hgt):
        self.put(y - 1, 1, "CHIPS", curses.A_DIM)
        for i, name in enumerate(self.names[:hgt]):
            s = self.status.get(name, {})
            state = s.get("state", "—")
            dot, color = {"running": ("●", "green"), "booting": ("◐", "yellow"),
                          "stopping": ("◐", "yellow"), "stopped": ("○", "red")}.get(state, ("·", None))
            attr = curses.A_REVERSE if i == self.sel else 0
            self.put(y + i, 1, " " * (wdt - 1), attr)
            self.put(y + i, 2, dot, self.C.get(color, 0) | attr)
            bold = curses.A_BOLD if s.get("kind") == "gateway" else 0
            self.put(y + i, 4, name, attr | bold)
            mark = ""
            if s.get("kind") == "gateway":
                mark = "⇄" if s.get("ip") else ""
            elif s.get("discovered"):
                mark = "✓"
            self.put(y + i, wdt - 1, mark, self.C["green"] | attr)

    def draw_detail(self, y, x, wdt, hgt):
        s = self.status.get(self.chip)
        if not s:
            self.put(y, x, f"{self.chip}: waiting for the simulation …", curses.A_DIM)
            return
        bottom = y + hgt
        line = y - 1
        head = f"{s['name']}  {s['kind']}  {s['bus']}"
        if s.get("id") is not None:
            head += f"  id {s['id']}"
        self.put(line, x, head, curses.A_BOLD)
        info = f"{s['state']}" + (f"  up {_dur(s['uptime'])}" if s.get("uptime") else "") + \
            f"  {s.get('frames', 0)} frames  mac {s['mac']}"
        line += 1
        self.put(line, x, info, curses.A_DIM)
        line += 1
        if s["kind"] == "gateway":
            self.put(line, x, f"network  {s.get('ip') or '—'}   {s.get('url', '')}")
            line += 1
            if s.get("mqtt"):
                self.put(line, x, "mqtt     " + s["mqtt"], cols=wdt)
                line += 1
            return
        self.put(line, x, "discovered" if s.get("discovered") else "not discovered yet",
                 self.C["green"] if s.get("discovered") else self.C["yellow"])
        line += 1
        if s.get("buttons"):
            col = self.put(line, x, "buttons  ")
            for b in s["buttons"]:
                col += self.put(line, x + col, f"[{b}]", curses.A_BOLD) + 1
            if self.modifier:
                self.put(line, x + col + 1, f"({self.modifier}: press 0-7)", self.C["yellow"])
            line += 1
        if s.get("pir"):
            on = s.get("motion")
            self.put(line, x, "PIR      " + ("● motion" if on else "○ quiet"),
                     self.C["yellow"] if on else curses.A_DIM)
            line += 1
        if s.get("relays") is not None:
            col = self.put(line, x, "relays   ")
            for i, r in enumerate(s["relays"]):
                col += self.put(line, x + col, "●" if r else "○", self.C["green"] if r else curses.A_DIM)
                if i % 4 == 3:
                    col += self.put(line, x + col, " ")
            line += 1
        editable = self.editable()
        self.value_sel = min(self.value_sel, max(0, len(editable) - 1))
        chosen = editable[self.value_sel][:2] if editable else None
        for name, part in (s.get("parts") or {}).items():
            if line >= bottom:
                break
            col = self.put(line, x, f"{name:<10} ", self.C["cyan"])
            keys = list(part.get("values", {}))
            if not keys:
                self.put(line, x + col, part.get("summary", ""), cols=wdt - col)
            for key in keys:
                unit = part["values"][key][0]
                value = part.get(key)
                text = f"{key} {value:g}{unit}" if isinstance(value, (int, float)) else f"{key} {value}"
                attr = curses.A_REVERSE if chosen == (name, key) else 0
                if col + width(text) + 2 > wdt:
                    line += 1
                    col = 11
                    if line >= bottom:
                        break
                col += self.put(line, x + col, text, attr) + 2
            line += 1

    def draw_events(self, y, hgt, w):
        label = "EVENTS · " + ("this chip" if self.only_selected else "all chips")
        extra = [k for k, on in self.show.items() if on]
        if extra:
            label += " · + " + ", ".join(extra)
        self.put(y - 1, 1, label, curses.A_DIM)
        shown = [e for e in self.events if self.visible(e)]
        self.scroll = max(0, min(self.scroll, len(shown) - hgt))
        end = len(shown) - self.scroll
        rows = shown[max(0, end - hgt):end]
        nw = max(len(n) for n in self.names)
        colors = {"action": "yellow", "relay": "green", "device": "magenta", "mqtt": "blue"}
        for i, e in enumerate(rows):
            stamp = time.strftime("%H:%M:%S", time.localtime(e["t"]))
            col = self.put(y + i, 1, stamp + " ", curses.A_DIM)
            col += self.put(y + i, 1 + col, f"{e['chip']:<{nw}} ", curses.A_BOLD)
            text = EMOJI.sub("", e["text"])
            attr = self.C.get(colors.get(e["kind"]), 0)
            if e["kind"] in ("serial", "i2c") or (e["kind"] == "can" and not e.get("shown", True)):
                attr = curses.A_DIM
            if e["kind"] == "can" and e.get("message") in ("Relais", "Nightlight", "LampGroup",
                                                         "Rollershutter", "Restart"):
                attr = self.C["cyan"]
            if e.get("important"):
                attr |= curses.A_BOLD
            self.put(y + i, 1 + col, text, attr, cols=w - col - 2)
        if self.scroll:
            self.put(y - 1, w - 16, f"↑ {self.scroll} more", self.C["yellow"])

    def draw_footer(self, y, w):
        if self.message and time.time() - self.message_at < 6:
            color = self.C["red"] if self.message.startswith("✗") else self.C["green"]
            self.put(y, 0, " " + self.message, color)
            return
        self.put(y, 0, " ? keys · 0-7 press · d/t/h then 0-7 · m motion · ←→ +- values · "
                       "c console · : command · q quit", curses.A_DIM)

    def draw_help(self, h, w):
        bw = min(w - 4, 76)
        bh = len(HELP) + 4
        y0, x0 = max(1, (h - bh) // 2), (w - bw) // 2
        for i in range(bh):
            self.put(y0 + i, x0, " " * bw, curses.A_REVERSE)
        self.put(y0 + 1, x0 + 2, "keys  (any key closes this)", curses.A_REVERSE | curses.A_BOLD)
        for i, (k, text) in enumerate(HELP):
            self.put(y0 + 3 + i, x0 + 2, f"{k:<12}{text}", curses.A_REVERSE, cols=bw - 4)

    # Input

    def prompt(self, label, initial=""):
        h, w = self.scr.getmaxyx()
        buf = initial
        curses.curs_set(1)
        self.scr.timeout(-1)
        try:
            while True:
                self.put(h - 1, 0, " " * (w - 1))
                self.put(h - 1, 0, label + buf, curses.A_BOLD)
                self.scr.move(h - 1, min(w - 2, width(label + buf)))
                k = self.scr.get_wch()
                if k in ("\n", "\r", curses.KEY_ENTER):
                    return buf
                if k == "\x1b":
                    return None
                if k in (curses.KEY_BACKSPACE, "\x7f", "\b"):
                    buf = buf[:-1]
                elif isinstance(k, str) and k.isprintable():
                    buf += k
        except curses.error:
            return None
        finally:
            curses.curs_set(0)
            self.scr.timeout(150)

    def key(self, k):
        if self.help:
            self.help = False
            return None
        h = self.sim[self.chip]
        if k in ("q", "Q"):
            return "quit"
        if k == "?":
            self.help = True
        elif k in (curses.KEY_UP, "k"):
            self.sel = (self.sel - 1) % len(self.names)
            self.value_sel = 0
        elif k in (curses.KEY_DOWN, "j"):
            self.sel = (self.sel + 1) % len(self.names)
            self.value_sel = 0
        elif k in ("d", "t", "h"):
            self.modifier = {"d": "double", "t": "triple", "h": "hold"}[k]
        elif isinstance(k, str) and k.isdigit():
            kind, self.modifier = self.modifier or "single", None
            self.press(int(k), kind)
        elif k in ("m", "M"):
            self.act("motion", h.motion, 10 if k == "m" else 60)
        elif k in (curses.KEY_RIGHT, "\t", "l"):
            self.value_sel += 1
            if self.value_sel >= len(self.editable()):
                self.value_sel = 0
        elif k in (curses.KEY_LEFT, curses.KEY_BTAB):
            self.value_sel = max(0, self.value_sel - 1)
        elif k in ("+", "-", "="):
            ed = self.editable()
            if ed:
                name, key, unit, step, lo, hi, value = ed[self.value_sel]
                new = min(hi, max(lo, value + (step if k in "+=" else -step)))
                self.act(f"{name} {key} {new:g}{unit}", h.part(name).set, **{key: new})
        elif k in ("\n", "\r", curses.KEY_ENTER):
            ed = self.editable()
            if ed:
                name, key, unit, step, lo, hi, value = ed[self.value_sel]
                text = self.prompt(f"{name} {key} ({lo:g}…{hi:g}{unit}) = ", f"{value:g}")
                if text:
                    self.act(f"{name} {key} {text}{unit}", h.part(name).set, **{key: text})
        elif k == ":":
            line = self.prompt(":")
            if line:
                self.command(line)
        elif k == "c":
            curses.def_prog_mode()
            curses.endwin()
            error = console.attach(self.topo, self.chip)
            curses.reset_prog_mode()
            self.scr.refresh()
            if error:
                self.flash("✗ " + error)
        elif k == "r":
            answer = self.prompt(f"restart {self.chip}? (y/n) ")
            if answer and answer.lower().startswith("y"):
                self.act(f"{self.chip} restarted", h.restart)
        elif k == "w":
            s = self.status.get(self.chip, {})
            if s.get("url"):
                webbrowser.open(s["url"])
                self.flash(f"opened {s['url']}")
        elif k == "f":
            self.only_selected = not self.only_selected
        elif k in ("s", "i", "a"):
            key = {"s": "serial", "i": "i2c", "a": "chatter"}[k]
            self.show[key] = not self.show[key]
        elif k == curses.KEY_PPAGE:
            self.scroll += 10
        elif k == curses.KEY_NPAGE:
            self.scroll = max(0, self.scroll - 10)
        elif k == curses.KEY_END:
            self.scroll = 0
        return None


def _dur(seconds):
    seconds = int(seconds)
    if seconds < 60:
        return f"{seconds}s"
    if seconds < 3600:
        return f"{seconds // 60}m{seconds % 60:02d}s"
    return f"{seconds // 3600}h{seconds // 60 % 60:02d}m"


def run(topology):
    locale.setlocale(locale.LC_ALL, "")
    tui = Tui(topology)
    try:
        curses.wrapper(tui.run)
    except KeyboardInterrupt:
        pass
