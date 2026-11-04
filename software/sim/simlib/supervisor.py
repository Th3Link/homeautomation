"""`sim up`: runs every chip in its own QEMU, plays the hardware around
them (buttons, PIR, relays, sensors, night light, PWM boards, DS18B20s) and
reports what happens.

Everything that happens is an event: a dict with `seq`, `t` (Unix time),
`chip`, `kind` and `text`, plus fields per kind:

  info     the chip's life: network up, discovered, restarted, a panic
  action   what the simulation did to it: a press, motion, a new value
  can      a frame (`bus`, `id`, `message`, `data` hex, `value`, `sensor`,
           `shown`: whether `sim up` prints it)
  relay    relays switched (`relays`: all twelve)
  device   a part's output changed (`device`, `state`)
  serial   a line of the serial log
  i2c      a finished I2C transaction (`bus`)
  mqtt     the gateway's MQTT log lines

`sim up` prints them; other `sim` commands, the TUI and scripts talk to it
over a Unix socket, one JSON request per connection (`request`), and can
follow the events (`{"cmd": "events"}`).
"""

import importlib.util
import json
import os
import re
import select
import signal
import socket
import subprocess
import threading
import time
from collections import deque

from . import can, devices, i2c, qemu, ui
from .topology import ONEWIRE_PIN

# Logical relay -> (expander 0x26/0x27, bit), cancomponents-hardware relais.rs.
RELAY_BITS = ((0, 3), (0, 2), (0, 1), (0, 7), (0, 6), (0, 5), (0, 4),
              (1, 3), (1, 2), (1, 1), (1, 7), (1, 6))
# Messages the gateway sends to a node.
COMMANDS = {"Relais", "Rollershutter", "Nightlight", "LampGroup", "Restart",
            "HeatingCommand", "PwmFrequency", "SensorConfig"}
REPEATED = {"AmbientLightSensor", "TemperatureSensor", "HumiditySensor", "PressureSensor",
            "Co2Sensor", "Co2Equivalent", "VocBreath", "AirQuality", "GasResistance",
            "RollershutterState"}
# Printed by `sim up` (the rest only with -s / --i2c, or to followers).
PRINTED = {"info", "action", "can", "relay", "device", "mqtt"}

LOG_EVENTS = (
    (re.compile(r"ethernet: address (\S+?)/"), "ip"),
    (re.compile(r"discovery: acked"), "discovered"),
    (re.compile(r"^mqtt\d?: .*"), "mqtt"),
    (re.compile(r"panic|Guru Meditation|Exception occurred", re.I), "panic"),
    (re.compile(r"reset reason: (?:Some\()?(\w+)"), "reset"),
)


def numbers(ns):
    """[0, 1, 2, 5] -> "0-2, 5"."""
    out, start = [], None
    for i, n in enumerate(ns):
        if start is None:
            start = n
        if i + 1 == len(ns) or ns[i + 1] != n + 1:
            out.append(f"{start}" if start == n else f"{start}-{n}")
            start = None
    return ", ".join(out)


def load_model(topo, model):
    """A device class: a built-in kind, or "file.py:Class" relative to the
    topology."""
    if model in devices.KINDS:
        return devices.KINDS[model]
    path, _, name = model.partition(":")
    path = (topo.path.parent / path).resolve()
    spec = importlib.util.spec_from_file_location(f"sim_model_{path.stem}", path)
    if not spec or not name:
        raise RuntimeError(f"model {model!r}: a built-in kind or file.py:Class")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return getattr(module, name)


class Chip:
    """One chip while it runs."""

    def __init__(self, sup, chip):
        self.sup = sup
        self.chip = chip
        self.proc = None
        self.state = "stopped"
        self.started = None
        self.boots = 0
        self.ip = None
        self.discovered = False
        self.mqtt = None
        self.tail = deque(maxlen=200)
        self.consoles = set()
        self.lock = threading.Lock()
        self.gpio = None
        self.gpio_lock = threading.Lock()
        self.outputs = {}
        self.expanders = [0, 0]
        self.buses = {}
        self.parts = {}
        self.frames = 0
        self.reported = {}
        self.last = ""
        self._make_parts()

    def _make_parts(self):
        """The parts outlive QEMU restarts, like the hardware around a chip."""
        c = self.chip
        if c.is_gateway:
            return
        if c.has_relays:
            for i, addr in enumerate((0x26, 0x27)):
                self.parts[f"relays-{i}"] = devices.Pca9554(addr, lambda v, i=i: self._expander(i, v))
        for bus, kind, addr, name, values in c.i2c_devices():
            cls = load_model(self.sup.topo, kind)
            part = cls(addr, name=name, **values)
            part.bus = bus
            part.notify = self._part_changed
            self.parts[name] = part
        for s in c.ds18b20:
            part = devices.Ds18b20(s["serial"], s["name"], temperature=s["temperature"])
            part.notify = self._part_changed
            self.parts[s["name"]] = part

    def part(self, name):
        if name in self.parts and not name.startswith("relays-"):
            return self.parts[name]
        matches = [n for n in self.parts if n.startswith(name) and not n.startswith("relays-")]
        if len(matches) == 1:
            return self.parts[matches[0]]
        known = ", ".join(n for n in self.parts if not n.startswith("relays-")) or "none"
        raise RuntimeError(f"{self.chip.name} has no {name!r} (parts: {known})")

    def bus_parts(self, number):
        if number == 0:
            out = [p for n, p in self.parts.items() if n.startswith("relays-")]
            return out + [p for p in self.parts.values() if getattr(p, "bus", None) == 0]
        return [p for p in self.parts.values() if getattr(p, "bus", None) == 1]

    # Lifecycle

    def start(self):
        c = self.chip
        run = self.sup.run_dir
        self.buses = {}
        for n in qemu.i2c_buses(c):
            self.buses[n] = i2c.Bus(run / f"{c.name}.i2c{n}", n, self.bus_parts(n), trace=self._i2c)
        onewire = [(ONEWIRE_PIN, p.rom_hex, round(p.values["temperature"] * 1000))
                   for p in self.parts.values() if isinstance(p, devices.Ds18b20)]
        args = qemu.argv(self.sup.qemu, c, self.sup.images.flash(c), self.sup.images.efuse(c),
                         run_dir=run, net="forward", onewire=onewire)
        self.log = open(self.sup.images.logfile(c), "a", buffering=1)
        self.log.write(f"\n=== {time.strftime('%Y-%m-%d %H:%M:%S')} start\n")
        self.proc = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, start_new_session=True)
        self.state = "booting"
        self.started = time.time()
        self.boots = 0
        self.ip = None
        self.discovered = False
        threading.Thread(target=self._serial, daemon=True).start()
        threading.Thread(target=self._gpio_connect, daemon=True).start()

    def stop(self):
        if self.proc and self.proc.poll() is None:
            self.state = "stopping"
            self.proc.terminate()
            try:
                self.proc.wait(10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        for bus in self.buses.values():
            bus.close()
        self.buses = {}
        if self.gpio:
            self.gpio.close()
            self.gpio = None

    def event(self, kind, text, **data):
        self.sup.event(self.chip, kind, text, **data)

    # Serial console

    def _serial(self):
        fd = self.proc.stdout.fileno()
        partial = ""
        while True:
            try:
                chunk = os.read(fd, 4096)
            except OSError:
                chunk = b""
            if not chunk:
                break
            with self.lock:
                for conn in list(self.consoles):
                    try:
                        conn.sendall(chunk)
                    except OSError:
                        self.consoles.discard(conn)
            partial += chunk.decode(errors="replace")
            *lines, partial = partial.split("\n")
            for line in lines:
                self._line(ui.strip(line).replace("\r", "").replace("❯", "").strip())
        code = self.proc.wait()
        if self.state != "stopping":
            self.event("info", ui.red(f"QEMU stopped (exit code {code})"), important=True)
        self.state = "stopped"
        with self.lock:
            for conn in list(self.consoles):
                conn.close()
            self.consoles.clear()

    def _line(self, line):
        if not line:
            return
        stamp = time.strftime("%H:%M:%S")
        self.log.write(f"{stamp} {line}\n")
        self.tail.append(line)
        self.event("serial", ui.dim(line))
        for pattern, kind in LOG_EVENTS:
            m = pattern.search(line)
            if not m:
                continue
            if kind == "ip":
                self.ip = m.group(1)
                self.state = "running"
                self.event("info", ui.green(f"network up {self.ip}") +
                           ui.dim(f"  web UI {self.chip.url}"), important=True)
            elif kind == "discovered":
                if not self.discovered:
                    self.event("info", "discovered by the gateway")
                self.discovered = True
                self.state = "running"
            elif kind == "mqtt":
                self.mqtt = line
                self.event("mqtt", line)
            elif kind == "panic":
                self.event("info", ui.red(line), important=True)
            elif kind == "reset":
                self.boots += 1
                self.discovered = False
                if self.boots > 1:
                    self.event("info", ui.yellow(f"restarted ({m.group(1)})"), important=True)
                elif not self.chip.is_gateway:
                    self.state = "running"
            break

    def console_input(self, data):
        if self.proc and self.proc.poll() is None:
            self.proc.stdin.write(data)
            self.proc.stdin.flush()

    # GPIO

    def _gpio_connect(self):
        path = str(self.sup.run_dir / f"{self.chip.name}.gpio")
        for _ in range(100):
            if self.proc.poll() is not None:
                return
            try:
                s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                s.connect(path)
                break
            except OSError:
                s.close()
                time.sleep(0.1)
        else:
            self.event("info", ui.red("no GPIO socket"), important=True)
            return
        self.gpio = s
        # The model starts every input high; the PIR idles low.
        if not self.chip.is_gateway and self.chip.pir_pin is not None:
            self.pin(self.chip.pir_pin, 0)
        # The DS18B20s are on QEMU's command line; only unplugging is left.
        for part in self.parts.values():
            if isinstance(part, devices.Ds18b20) and not part.values["plugged"]:
                self._onewire(part, "set")
        f = s.makefile("r")
        try:
            for line in f:
                parts = line.split()
                if len(parts) == 3 and parts[0] == "out":
                    self.outputs[int(parts[1])] = int(parts[2])
                elif len(parts) >= 3 and parts[0] == "ow" and parts[2] == "error":
                    self.event("info", ui.red("1-wire: " + " ".join(parts[3:])), important=True)
        except (OSError, ValueError):
            pass

    def _gpio_send(self, line):
        with self.gpio_lock:
            if not self.gpio:
                raise RuntimeError(f"{self.chip.name} has no GPIO connection yet")
            self.gpio.sendall((line + "\n").encode())

    def pin(self, n, level):
        self._gpio_send(f"pin {n} {level}")

    def _onewire(self, part, op):
        mc = round(part.values["temperature"] * 1000)
        self._gpio_send(f"ow {ONEWIRE_PIN} {op} {part.rom_hex} {mc}")
        self._gpio_send(f"ow {ONEWIRE_PIN} unplug {part.rom_hex} {0 if part.values['plugged'] else 1}")

    # Parts

    def _part_changed(self, part, text):
        self.event("device", text, device=part.name, state=part.state())

    def _i2c(self, bus, text):
        self.event("i2c", ui.dim(f"i2c{bus} ") + text, bus=bus)

    def set_part(self, name, values):
        part = self.part(name)
        with self._bus_lock(part):
            part.set(**values)
        if isinstance(part, devices.Ds18b20) and self.gpio:
            self._onewire(part, "set")
        text = ", ".join(f"{k} {v:g}{part.VALUES[k][0]}" for k, v in part.values.items() if k in values)
        self.event("action", ui.yellow(f"🎛 {part.name}: {text}"), device=part.name, state=part.state())
        return part.state()

    def _bus_lock(self, part):
        bus = self.buses.get(getattr(part, "bus", None))
        return bus.lock if bus else threading.Lock()

    # Relays

    def _expander(self, index, value):
        before = self.relays()
        self.expanders[index] = value
        after = self.relays()
        on = [n for n, (a, b) in enumerate(zip(before, after)) if b and not a]
        off = [n for n, (a, b) in enumerate(zip(before, after)) if a and not b]
        parts = []
        if on:
            parts.append(ui.green(f"relay{'s' if len(on) > 1 else ''} {numbers(on)} on"))
        if off:
            parts.append(f"relay{'s' if len(off) > 1 else ''} {numbers(off)} off")
        if parts:
            self.event("relay", "⚡ " + ", ".join(parts), relays=after, on=on, off=off)

    def relays(self):
        return [bool(self.expanders[e] >> bit & 1) for e, bit in RELAY_BITS]

    def status(self):
        c = self.chip
        info = {
            "name": c.name, "kind": c.kind, "bus": c.bus, "state": self.state,
            "uptime": time.time() - self.started if self.started and self.state != "stopped" else None,
            "mac": c.mac_text, "frames": self.frames, "last": self.last,
            "pid": self.proc.pid if self.proc else None,
        }
        if c.is_gateway:
            info.update(ip=self.ip, url=c.url, mqtt=self.mqtt)
        else:
            info.update(id=c.device_id, discovered=self.discovered, buttons=c.buttons,
                        pir=c.pir_pin is not None, extension=c.extension)
            if c.pir_pin is not None:
                info["motion"] = self.sup.motion_until.get(c.name, 0) > time.time()
            if c.has_relays:
                info["relays"] = self.relays()
            info["parts"] = {n: {**p.state(), "summary": p.summary(),
                                 "values": {k: list(v) for k, v in p.VALUES.items()}}
                             for n, p in self.parts.items() if not n.startswith("relays-")}
            if "veml7700" in self.parts:
                info["lux"] = self.parts["veml7700"].lux
        return info


class Supervisor:
    def __init__(self, topo, images, qemu_path, chips, show_all=False, serial=False,
                 show_i2c=False, quiet=False):
        self.topo = topo
        self.images = images
        self.qemu = qemu_path
        self.run_dir = topo.run_dir
        self.show_all = show_all
        self.serial = serial
        self.show_i2c = show_i2c
        self.quiet = quiet
        self.motion_until = {}
        self.chips = {c.name: Chip(self, c) for c in chips}
        self.print_lock = threading.Lock()
        self.events = deque(maxlen=2000)
        self.seq = 0
        self.followers = []
        self.event_lock = threading.Lock()
        self.stopping = threading.Event()
        self.width = max(len(c.name) for c in topo.chips)

    def event(self, chip, kind, text, important=False, shown=True, **data):
        name = chip.name if hasattr(chip, "name") else str(chip)
        gateway = getattr(chip, "is_gateway", False)
        now = time.time()
        plain = ui.strip(text)
        with self.event_lock:
            self.seq += 1
            record = {"seq": self.seq, "t": now, "chip": name, "kind": kind, "text": plain,
                      "important": important, **data}
            if kind == "can":
                record["shown"] = shown
            if kind not in ("serial", "i2c") and shown:
                self.events.append(record)
                if name in self.chips:
                    self.chips[name].last = plain
            followers = list(self.followers)
        for f in followers:
            f.offer(record)
        printed = (kind in PRINTED and shown) or (kind == "serial" and self.serial) or \
            (kind == "i2c" and self.show_i2c)
        if printed and not self.quiet:
            stamp = time.strftime("%H:%M:%S", time.localtime(now))
            with self.print_lock:
                print(f"{ui.dim(stamp)}  {ui.chip_name(name, gateway, self.width)}  {text}", flush=True)

    # CAN

    def _watch_bus(self, bus):
        try:
            sock = can.open_bus(bus)
        except OSError as e:
            self.event(bus, "info", ui.red(f"can't watch {bus}: {e}"))
            return
        while not self.stopping.is_set():
            r, _, _ = select.select([sock], [], [], 0.5)
            if not r:
                continue
            frame = can.read(sock)
            if frame is None:
                continue
            node = self.topo.node_by_can(bus, frame.device_type, frame.device_id)
            running = self.chips.get(node.name) if node else None
            if running:
                running.frames += 1
            shown = True
            if not self.show_all:
                if frame.chatter or frame.message == "RelaisState":
                    shown = False
                # Sensors repeat their value once per send interval.
                elif running and frame.message in REPEATED:
                    key = (frame.message, bytes(frame.data[:6]))
                    if running.reported.get(key) == frame.data:
                        shown = False
                    running.reported[key] = frame.data
            text = frame.describe()
            if frame.message in COMMANDS:
                text = ui.cyan("← " + text)
            data = dict(bus=bus, id=frame.raw_id, message=frame.message, data=frame.data.hex(),
                        rtr=frame.rtr, value=frame.value, device=frame.device)
            if frame.message in can.FIXED_POINT:
                data["sensor"] = frame.sensor
            if node:
                self.event(node, "can", text, shown=shown, **data)
            else:
                self.event(bus, "can", ui.dim(f"[{frame.device}] ") + text, shown=shown, **data)

    # Control socket

    def _serve(self, srv):
        while not self.stopping.is_set():
            try:
                conn, _ = srv.accept()
            except OSError:
                return
            threading.Thread(target=self._handle, args=(conn,), daemon=True).start()

    def _handle(self, conn):
        f = conn.makefile("rb")
        try:
            request = json.loads(f.readline())
            answer = self._dispatch(request, conn)
        except Exception as e:  # noqa: BLE001 — every failure goes back to the caller
            answer = {"error": str(e)}
        if answer is None:  # the connection became a console or a follower
            return
        try:
            conn.sendall(json.dumps(answer).encode() + b"\n")
        except OSError:
            pass
        conn.close()

    def _chip(self, name):
        chip = self.topo.chip(name)
        if chip.name not in self.chips:
            raise RuntimeError(f"{chip.name} isn't running (sim up started only some chips)")
        return self.chips[chip.name]

    def _dispatch(self, req, conn):
        cmd = req.get("cmd")
        if cmd == "status":
            with self.event_lock:
                recent = list(self.events)[-req.get("events", 10):]
            return {"chips": [c.status() for c in self.chips.values()], "events": recent,
                    "seq": self.seq, "topology": str(self.topo.path), "pid": os.getpid()}
        if cmd == "down":
            self.stopping.set()
            return {"ok": True}
        if cmd == "events":
            Follower(self, conn, req).run()
            return None
        r = self._chip(req["chip"])
        if cmd == "press":
            return self._press(r, req["button"], req.get("kind", "single"), req.get("secs", 3.0))
        if cmd == "motion":
            return self._motion(r, req.get("secs", 10))
        if cmd == "lux":
            return {"state": r.set_part("veml7700", {"lux": req["lux"]})}
        if cmd == "set":
            return {"state": r.set_part(req["device"], req["values"])}
        if cmd == "get":
            status = r.status()
            if req.get("device"):
                part = r.part(req["device"])
                return {"state": {**part.state(), "summary": part.summary()}}
            return {"state": status}
        if cmd == "i2c":
            bus = r.buses.get(int(req.get("bus", 1)))
            if bus is None:
                raise RuntimeError(f"{r.chip.name} has no simulated I2C bus {req.get('bus', 1)} "
                                   f"(buses: {', '.join(map(str, r.buses)) or 'none'})")
            if req.get("scan"):
                return {"devices": [{"addr": a, "name": d.name, "kind": d.kind,
                                     "registers": isinstance(d, devices.RegisterDevice)}
                                    for a, d in sorted(bus.devices.items())]}
            data = bus.transfer(int(req["addr"]), bytes(req.get("write", [])), int(req.get("read", 0)))
            if data is None:
                raise RuntimeError(f"nothing answers at 0x{int(req['addr']):02x} on bus {bus.number}")
            return {"read": list(data)}
        if cmd == "pin":
            r.pin(int(req["pin"]), int(req["level"]))
            return {"ok": True}
        if cmd == "restart":
            r.stop()
            r.start()
            r.event("info", ui.yellow("QEMU restarted"), important=True)
            return {"ok": True}
        if cmd == "console":
            if not r.proc or r.proc.poll() is not None:
                raise RuntimeError(f"{r.chip.name} isn't running")
            conn.sendall(b'{"ok": true}\n')
            with r.lock:
                r.consoles.add(conn)
            try:
                while True:
                    data = conn.recv(1024)
                    if not data:
                        break
                    r.console_input(data)
            except OSError:
                pass
            with r.lock:
                r.consoles.discard(conn)
            conn.close()
            return None
        raise RuntimeError(f"unknown request {cmd!r}")

    def _press(self, r, button, kind, secs):
        c = r.chip
        if c.is_gateway:
            raise RuntimeError(f"{c.name} is a gateway")
        pin = c.button_pin(int(button))
        if pin is None:
            have = ", ".join(map(str, c.buttons)) or "none"
            raise RuntimeError(f"{c.name} has no button {button} (buttons: {have})")
        clicks = {"single": 1, "double": 2, "triple": 3}
        if kind not in clicks and kind != "hold":
            raise RuntimeError("kind: single, double, triple or hold")
        r.event("action", ui.yellow(f"👆 button {button} {kind}" + (f" {secs:g}s" if kind == "hold" else "")),
                button=int(button), press=kind)
        if kind == "hold":
            r.pin(pin, 0)
            time.sleep(float(secs))
            r.pin(pin, 1)
        else:
            for i in range(clicks[kind]):
                if i:
                    time.sleep(0.12)
                r.pin(pin, 0)
                time.sleep(0.12)
                r.pin(pin, 1)
        return {"ok": True}

    def _motion(self, r, secs):
        c = r.chip
        if c.is_gateway or c.pir_pin is None:
            raise RuntimeError(f"{c.name} has no PIR (extension = \"sensors\")")
        secs = float(secs)
        r.event("action", ui.yellow(f"🚶 motion for {secs:g}s"), motion=secs)
        until = time.time() + secs
        self.motion_until[c.name] = until
        r.pin(c.pir_pin, 1)

        def end():
            time.sleep(secs)
            if self.motion_until.get(c.name) != until:
                return  # more motion came in meanwhile
            try:
                r.pin(c.pir_pin, 0)
            except (RuntimeError, OSError):
                pass
        threading.Thread(target=end, daemon=True).start()
        return {"ok": True}

    # Running

    def run(self, ready=None):
        self.run_dir.mkdir(parents=True, exist_ok=True)
        control = self.run_dir / "control.sock"
        try:
            control.unlink()
        except FileNotFoundError:
            pass
        srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        srv.bind(str(control))
        srv.listen(8)
        threading.Thread(target=self._serve, args=(srv,), daemon=True).start()
        for bus in sorted({c.chip.bus for c in self.chips.values()}):
            threading.Thread(target=self._watch_bus, args=(bus,), daemon=True).start()

        def on_signal(*_):
            self.stopping.set()
        if threading.current_thread() is threading.main_thread():
            signal.signal(signal.SIGINT, on_signal)
            signal.signal(signal.SIGTERM, on_signal)

        for r in self.chips.values():
            r.start()
        if ready:
            ready()
        try:
            while not self.stopping.wait(0.5):
                pass
        finally:
            if not self.quiet:
                with self.print_lock:
                    print(ui.dim("\nstopping …"), flush=True)
            threads = [threading.Thread(target=r.stop) for r in self.chips.values()]
            for t in threads:
                t.start()
            for t in threads:
                t.join()
            srv.close()
            for f in list(self.followers):
                f.close()
            try:
                control.unlink()
            except FileNotFoundError:
                pass


class Follower:
    """A connection that gets the events as JSON lines: optionally only some
    kinds and chips, after the recent ones (`since` a sequence number, or
    the last `history` kept ones; serial lines and I2C aren't kept)."""

    def __init__(self, sup, conn, req):
        self.sup = sup
        self.conn = conn
        self.kinds = set(req.get("kinds") or [])
        self.chips = set(req.get("chips") or [])
        self.since = req.get("since")
        self.history = req.get("history")
        self.queue = deque(maxlen=20000)
        self.wake = threading.Event()
        self.closed = False

    def wants(self, record):
        return (not self.kinds or record["kind"] in self.kinds) and \
            (not self.chips or record["chip"] in self.chips)

    def offer(self, record):
        if self.wants(record):
            self.queue.append(record)
            self.wake.set()

    def close(self):
        self.closed = True
        self.wake.set()

    def run(self):
        with self.sup.event_lock:
            if self.history:
                kept = [r for r in self.sup.events if self.wants(r)]
                self.queue.extend(kept[-self.history:])
            elif self.since is not None:
                for record in self.sup.events:
                    if record["seq"] > self.since and self.wants(record):
                        self.queue.append(record)
            seq = self.sup.seq
            self.sup.followers.append(self)
        try:
            self.conn.sendall(json.dumps({"ok": True, "seq": seq}).encode() + b"\n")
            while not self.closed and not self.sup.stopping.is_set():
                self.wake.wait(1.0)
                self.wake.clear()
                lines = []
                while self.queue:
                    lines.append(json.dumps(self.queue.popleft()))
                if lines:
                    self.conn.sendall(("\n".join(lines) + "\n").encode())
                elif not _alive(self.conn):
                    break
        except OSError:
            pass
        finally:
            with self.sup.event_lock:
                if self in self.sup.followers:
                    self.sup.followers.remove(self)
            self.conn.close()


def _alive(conn):
    """Whether the other end is still there (it sends nothing)."""
    try:
        r, _, _ = select.select([conn], [], [], 0)
        return not r or conn.recv(1, socket.MSG_PEEK) != b""
    except OSError:
        return False


def request(topo, req, timeout=None):
    """Sends one request to a running `sim up`."""
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        s.connect(str(topo.run_dir / "control.sock"))
    except OSError:
        s.close()
        return None
    data = b""
    try:
        s.settimeout(timeout)
        s.sendall(json.dumps(req).encode() + b"\n")
        if req.get("cmd") in ("console", "events"):
            return s
        while not data.endswith(b"\n"):
            chunk = s.recv(65536)
            if not chunk:
                break
            data += chunk
    except OSError:
        # It is shutting down.
        if req.get("cmd") in ("status", "down"):
            return None
        raise
    finally:
        if req.get("cmd") not in ("console", "events"):
            s.close()
    return json.loads(data) if data else {"error": "no answer"}
