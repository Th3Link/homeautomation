"""Scripting a running simulation (`./sim up`, or the one pytest starts).

    from simlib import Sim

    sim = Sim()                                   # sim.toml, or Sim("mine.toml")
    flur = sim["taster-flur"]                     # a chip, by name or prefix
    flur.press(0, "double")
    sim.expect_can("ButtonEvent", chip=flur)      # waits for the frame
    flur.bme680.set(temperature=25.5)             # a part, by name
    frame = sim.expect_can("TemperatureSensor", chip=flur, value=25.5, timeout=40)
    sim["relais-keller"].ds18b20_0.set(plugged=0) # a DS18B20 drops off the bus
    sim.expect("LOST", chip="relais-keller", kind="serial", timeout=60)

`expect*` waits for an event after the last thing this script did (a
press, a new value, a frame sent), so nothing that happens in between is
missed. `./sim run script.py` runs a script with `sim` already there.
"""

import json
import os
import re
import socket
import threading
import time
from collections import deque

from . import can, supervisor
from .topology import SIM_DIR, Topology


class SimError(Exception):
    pass


class Timeout(SimError, AssertionError):
    """An expected event didn't come (an AssertionError for pytest)."""


class Event(dict):
    """An event (see simlib/supervisor.py) with attribute access:
    `e.text`, `e.message`, `e.value`."""

    def __getattr__(self, name):
        try:
            return self[name]
        except KeyError:
            raise AttributeError(name) from None

    @property
    def payload(self):
        return bytes.fromhex(self.get("data", ""))


class Sim:
    def __init__(self, topology=None, connect=True, history=0):
        self.topology = Topology(topology or os.environ.get("SIM_TOPOLOGY") or SIM_DIR / "sim.toml")
        self._events = deque(maxlen=50000)
        self._cond = threading.Condition()
        self._mark = 0
        self._sock = None
        self._can = {}
        if connect:
            self.connect(history=history)

    # Connection

    def connect(self, timeout=10, history=0):
        """Follows the events of the running simulation, after the last
        `history` ones."""
        deadline = time.time() + timeout
        while True:
            answer = supervisor.request(self.topology, {"cmd": "status"}, 5)
            if answer is not None:
                break
            if time.time() > deadline:
                raise SimError(f"no simulation running for {self.topology.path} (./sim up)")
            time.sleep(0.2)
        self._mark = answer["seq"]
        req = {"cmd": "events", "history": history} if history else {"cmd": "events", "since": answer["seq"]}
        sock = supervisor.request(self.topology, req)
        f = sock.makefile("rb")
        hello = json.loads(f.readline())
        if "error" in hello:
            raise SimError(hello["error"])
        self._sock = sock
        threading.Thread(target=self._follow, args=(f,), daemon=True).start()

    def _follow(self, f):
        for line in f:
            try:
                event = Event(json.loads(line))
            except ValueError:
                continue
            with self._cond:
                self._events.append(event)
                self._cond.notify_all()

    def close(self):
        if self._sock:
            self._sock.close()
            self._sock = None
        for s in self._can.values():
            s.close()
        self._can = {}

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def request(self, req, timeout=30):
        answer = supervisor.request(self.topology, req, timeout)
        if answer is None:
            raise SimError("the simulation isn't running")
        if "error" in answer:
            raise SimError(answer["error"])
        return answer

    def mark(self):
        """From now on: `expect` looks at events after this point."""
        with self._cond:
            self._mark = self._events[-1]["seq"] if self._events else self._mark
        return self._mark

    # Chips

    def __getitem__(self, name):
        return ChipHandle(self, self.topology.chip(name).name)

    @property
    def chips(self):
        return [ChipHandle(self, c.name) for c in self.topology.chips]

    def status(self):
        return {c["name"]: c for c in self.request({"cmd": "status"})["chips"]}

    def wait_ready(self, timeout=120, chips=None):
        """Until every gateway has its network and every node was
        discovered (or runs, without a gateway on its bus)."""
        names = {c.name if isinstance(c, ChipHandle) else c for c in chips} if chips else None
        deadline = time.time() + timeout
        while True:
            pending = []
            for s in self.status().values():
                if names and s["name"] not in names:
                    continue
                if s["kind"] == "gateway":
                    ok = s["ip"] is not None
                else:
                    has_gw = any(g.bus == s["bus"] for g in self.topology.gateways)
                    ok = s["discovered"] if has_gw else s["state"] == "running"
                if not ok:
                    pending.append(s["name"])
            if not pending:
                return
            if time.time() > deadline:
                raise Timeout(f"not ready after {timeout} s: {', '.join(pending)}")
            time.sleep(0.5)

    # Events

    def events(self, since=None):
        """The events seen so far (after `since`, a sequence number)."""
        with self._cond:
            return [e for e in self._events if since is None or e["seq"] > since]

    def expect(self, pattern=None, *, chip=None, kind=None, timeout=30, since=None, where=None):
        """The first event after the last action (or `since`) whose text
        matches `pattern` (a regular expression), of `chip` and `kind`,
        for which `where(event)` holds. Raises Timeout."""
        rx = re.compile(pattern) if isinstance(pattern, str) else pattern
        chip = _name(chip)
        since = self._mark if since is None else since
        deadline = time.time() + timeout

        def match(e):
            return (e["seq"] > since and (chip is None or e["chip"] == chip)
                    and (kind is None or e["kind"] == kind)
                    and (rx is None or rx.search(e["text"]))
                    and (where is None or where(e)))

        with self._cond:
            while True:
                for e in self._events:
                    if match(e):
                        return e
                left = deadline - time.time()
                if left <= 0:
                    what = " ".join(x for x in (kind, f"from {chip}" if chip else None,
                                                f"/{rx.pattern}/" if rx else None) if x)
                    raise Timeout(f"no {what or 'event'} within {timeout} s")
                self._cond.wait(min(left, 0.5))

    def expect_can(self, message, *, chip=None, data=None, value=None, sensor=None,
                   timeout=30, since=None, tolerance=1 / 16):
        """A CAN frame of `message` (a name from simlib/can.py), from or to
        `chip`, with `data` (bytes, or a prefix of them), a decoded `value`
        (within `tolerance`) and a `sensor` (as `sim can` shows it)."""
        def where(e):
            if e["message"] != message:
                return False
            if data is not None and not e.payload.startswith(bytes(data)):
                return False
            if value is not None and (e.get("value") is None or abs(e["value"] - value) > tolerance):
                return False
            return sensor is None or e.get("sensor") == sensor
        return self.expect(chip=chip, kind="can", timeout=timeout, since=since, where=where)

    def no_event(self, pattern=None, *, within=3, **kw):
        """Asserts that no such event comes within `within` seconds."""
        try:
            e = self.expect(pattern, timeout=within, **kw)
        except Timeout:
            return
        raise AssertionError(f"unexpected: {e['chip']} {e['text']}")

    # CAN

    def send_can(self, message, *, to=None, data=b"", bus=None, device_type=None, device_id=None,
                 rtr=False):
        """Sends a frame as the gateway would: `to` a node (its type and
        id), or `bus`, `device_type` and `device_id` directly."""
        if to is not None:
            node = self.topology.chip(_name(to))
            bus, device_type, device_id = node.bus, node.device_type, node.device_id
        if bus is None:
            raise SimError("send_can needs `to` or `bus`")
        if bus not in self._can:
            self._can[bus] = can.open_bus(bus)
        self.mark()
        can.write(self._can[bus], can.frame_id(device_type, device_id, message), data, rtr)


def _name(chip):
    if chip is None:
        return None
    return chip.name if isinstance(chip, ChipHandle) else chip


class ChipHandle:
    """A chip of the running simulation. Its parts are attributes:
    `flur.bme680`, `relais.ds18b20_0` (or `chip.part("ds18b20-0")`)."""

    def __init__(self, sim, name):
        self.sim = sim
        self.name = name

    def __repr__(self):
        return f"<chip {self.name}>"

    def _do(self, req, timeout=30):
        self.sim.mark()
        return self.sim.request({"chip": self.name, **req}, timeout)

    @property
    def topology(self):
        return self.sim.topology.chip(self.name)

    def status(self):
        return self._do({"cmd": "get"})["state"]

    def press(self, button, kind="single", secs=3.0):
        """single, double, triple or hold (for `secs`)."""
        self._do({"cmd": "press", "button": button, "kind": kind, "secs": secs}, timeout=secs + 15)

    def motion(self, secs=10.0):
        self._do({"cmd": "motion", "secs": secs})

    def pin(self, n, level):
        self._do({"cmd": "pin", "pin": n, "level": level})

    def restart(self):
        self._do({"cmd": "restart"}, timeout=60)

    @property
    def relays(self):
        """The relays of the node's own board, on or off."""
        return self.status().get("relays")

    def bank(self, bank):
        """The relays of relay bank `bank` (0 = own board, 1-4 on the
        extension bus), on or off; None without that board."""
        board = (self.status().get("banks") or {}).get(str(bank))
        return board and board["relays"]

    def part(self, name):
        return PartHandle(self, name)

    def __getattr__(self, name):
        if name.startswith("_"):
            raise AttributeError(name)
        return PartHandle(self, name.replace("_", "-"))

    def i2c(self, addr, write=b"", read=0, bus=1):
        """A raw I2C transaction on one of the chip's simulated buses:
        `write` bytes, then `read` bytes (repeated start). Returns them."""
        return bytes(self._do({"cmd": "i2c", "bus": bus, "addr": addr, "write": list(write),
                               "read": read})["read"])

    def console(self, command, wait=1.0, timeout=10):
        """Types `command` into the chip's console and returns what it
        answered (until it was quiet for `wait` seconds)."""
        sock = supervisor.request(self.sim.topology, {"cmd": "console", "chip": self.name})
        if sock is None:
            raise SimError("the simulation isn't running")
        try:
            f = sock.makefile("rb")
            hello = json.loads(f.readline())
            if "error" in hello:
                raise SimError(hello["error"])
            sock.settimeout(0.2)
            self.sim.mark()
            out = b""
            # Wake the console, then the command; `exit` leaves it again.
            for line in (b"\r", command.encode() + b"\r"):
                sock.sendall(line)
                time.sleep(0.3)
            deadline = time.time() + timeout
            quiet_since = time.time()
            while time.time() < deadline and time.time() - quiet_since < wait:
                try:
                    chunk = sock.recv(4096)
                except socket.timeout:
                    continue
                if not chunk:
                    break
                out += chunk
                quiet_since = time.time()
            sock.sendall(b"exit\r")
            time.sleep(0.2)
        finally:
            sock.close()
        text = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", out.decode(errors="replace"))
        lines = [ln.replace("❯", "").strip() for ln in text.replace("\r", "\n").split("\n")]
        return "\n".join(ln for ln in lines if ln and ln != command)


class PartHandle:
    """A part of a chip: `set(**values)`, `get()`, and its values as
    attributes (`flur.bme680.temperature`)."""

    def __init__(self, chip, name):
        self.chip = chip
        self.name = name

    def __repr__(self):
        return f"<{self.chip.name} {self.name}>"

    def set(self, **values):
        return self.chip._do({"cmd": "set", "device": self.name, "values": values})["state"]

    def get(self):
        return self.chip._do({"cmd": "get", "device": self.name})["state"]

    def __getattr__(self, key):
        if key.startswith("_"):
            raise AttributeError(key)
        state = self.get()
        if key not in state:
            raise AttributeError(f"{self.name} has no {key!r} ({', '.join(state)})")
        return state[key]

    def ramp(self, key, to, secs, steps=None):
        """Moves `key` to `to` over `secs` seconds, in even steps."""
        start = self.get()[key]
        steps = steps or max(1, int(secs))
        for i in range(1, steps + 1):
            self.set(**{key: start + (to - start) * i / steps})
            if i < steps:
                time.sleep(secs / steps)
