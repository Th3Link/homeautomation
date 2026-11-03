"""`sim up`: runs every chip in its own QEMU, plays the hardware around
them (buttons, PIR, relays, light) and prints what happens. Other `sim`
commands talk to it over a Unix socket, one JSON request per connection."""

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

from . import can, i2c, qemu, ui

# Logical relay -> (expander 0x26/0x27, bit), cancomponents-hardware relais.rs.
RELAY_BITS = ((0, 3), (0, 2), (0, 1), (0, 7), (0, 6), (0, 5), (0, 4),
              (1, 3), (1, 2), (1, 1), (1, 7), (1, 6))
# Messages the gateway sends to a node.
COMMANDS = {"Relais", "Rollershutter", "Nightlight", "LampGroup", "Restart",
            "HeatingCommand", "PwmFrequency", "SensorConfig"}
REPEATED = {"AmbientLightSensor", "TemperatureSensor", "HumiditySensor", "PressureSensor",
            "Co2Sensor", "Co2Equivalent", "VocBreath", "AirQuality", "GasResistance",
            "RollershutterState"}

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
        self.buses = []
        self.veml = None
        self.frames = 0
        self.reported = {}
        self.last = ""

    # Lifecycle

    def start(self):
        c = self.chip
        run = self.sup.run_dir
        self.buses = []
        if not c.is_gateway and c.has_relays:
            self.buses.append(i2c.Bus(run / f"{c.name}.i2c0", [
                i2c.Pca9554(0x26, lambda v: self._expander(0, v)),
                i2c.Pca9554(0x27, lambda v: self._expander(1, v)),
            ]))
        if not c.is_gateway and c.has_sensors:
            self.veml = i2c.Veml7700(0x10, c.lux)
            self.buses.append(i2c.Bus(run / f"{c.name}.i2c1", [self.veml]))
        args = qemu.argv(self.sup.qemu, c, self.sup.images.flash(c), self.sup.images.efuse(c),
                         run_dir=run, net="forward")
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
        for bus in self.buses:
            bus.close()
        self.buses = []
        if self.gpio:
            self.gpio.close()
            self.gpio = None

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
            self.sup.event(self.chip, ui.red(f"QEMU stopped (exit code {code})"), important=True)
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
        if self.sup.serial:
            self.sup.event(self.chip, ui.dim(line))
        for pattern, kind in LOG_EVENTS:
            m = pattern.search(line)
            if not m:
                continue
            if kind == "ip":
                self.ip = m.group(1)
                self.state = "running"
                self.sup.event(self.chip, ui.green(f"network up {self.ip}") +
                               ui.dim(f"  web UI {self.chip.url}"), important=True)
            elif kind == "discovered":
                if not self.discovered:
                    self.sup.event(self.chip, "discovered by the gateway")
                self.discovered = True
                self.state = "running"
            elif kind == "mqtt":
                self.mqtt = line
                self.sup.event(self.chip, line)
            elif kind == "panic":
                self.sup.event(self.chip, ui.red(line), important=True)
            elif kind == "reset":
                self.boots += 1
                self.discovered = False
                if self.boots > 1:
                    self.sup.event(self.chip, ui.yellow(f"restarted ({m.group(1)})"), important=True)
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
            self.sup.event(self.chip, ui.red("no GPIO socket"), important=True)
            return
        self.gpio = s
        # The model starts every input high; the PIR idles low.
        if not self.chip.is_gateway and self.chip.pir_pin is not None:
            self.pin(self.chip.pir_pin, 0)
        f = s.makefile("r")
        try:
            for line in f:
                parts = line.split()
                if len(parts) == 3 and parts[0] == "out":
                    self.outputs[int(parts[1])] = int(parts[2])
        except (OSError, ValueError):
            pass

    def pin(self, n, level):
        with self.gpio_lock:
            if not self.gpio:
                raise RuntimeError(f"{self.chip.name} has no GPIO connection yet")
            self.gpio.sendall(f"pin {n} {level}\n".encode())

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
            self.sup.event(self.chip, "⚡ " + ", ".join(parts))

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
                        pir=c.pir_pin is not None)
            if c.has_relays:
                info["relays"] = self.relays()
            if self.veml:
                info["lux"] = self.veml.lux
        return info


class Supervisor:
    def __init__(self, topo, images, qemu_path, chips, show_all=False, serial=False):
        self.topo = topo
        self.images = images
        self.qemu = qemu_path
        self.run_dir = topo.run_dir
        self.show_all = show_all
        self.serial = serial
        self.chips = {c.name: Chip(self, c) for c in chips}
        self.print_lock = threading.Lock()
        self.events = deque(maxlen=50)
        self.stopping = threading.Event()
        self.width = max(len(c.name) for c in topo.chips)

    def event(self, chip, text, important=False):
        name = chip.name if hasattr(chip, "name") else str(chip)
        gateway = getattr(chip, "is_gateway", False)
        stamp = time.strftime("%H:%M:%S")
        self.events.append(f"{stamp} {name} {ui.strip(text)}")
        if name in self.chips:
            self.chips[name].last = ui.strip(text)
        with self.print_lock:
            print(f"{ui.dim(stamp)}  {ui.chip_name(name, gateway, self.width)}  {text}", flush=True)

    # CAN

    def _watch_bus(self, bus):
        try:
            sock = can.open_bus(bus)
        except OSError as e:
            self.event(bus, ui.red(f"can't watch {bus}: {e}"))
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
            if not self.show_all:
                if frame.chatter or frame.message == "RelaisState":
                    continue
                # Sensors repeat their value once per send interval.
                if running and frame.message in REPEATED:
                    if running.reported.get(frame.message) == frame.data:
                        continue
                    running.reported[frame.message] = frame.data
            text = frame.describe()
            if frame.message in COMMANDS:
                text = ui.cyan("← " + text)
            if node:
                self.event(node, text)
            else:
                self.event(f"{bus}", ui.dim(f"[{frame.device}] ") + text)

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
        if answer is None:  # the connection became a console
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
            return {"chips": [c.status() for c in self.chips.values()],
                    "events": list(self.events)[-10:]}
        if cmd == "down":
            self.stopping.set()
            return {"ok": True}
        r = self._chip(req["chip"])
        if cmd == "press":
            return self._press(r, req["button"], req.get("kind", "single"), req.get("secs", 3.0))
        if cmd == "motion":
            return self._motion(r, req.get("secs", 10))
        if cmd == "lux":
            if not r.veml:
                raise RuntimeError(f"{r.chip.name} has no light sensor (extension = \"sensors\")")
            r.veml.lux = float(req["lux"])
            self.event(r.chip, f"☀ light set to {r.veml.lux:g} lx")
            return {"ok": True}
        if cmd == "pin":
            r.pin(int(req["pin"]), int(req["level"]))
            return {"ok": True}
        if cmd == "restart":
            r.stop()
            r.start()
            self.event(r.chip, ui.yellow("QEMU restarted"), important=True)
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
        self.event(c, ui.yellow(f"👆 button {button} {kind}" + (f" {secs:g}s" if kind == "hold" else "")))
        if kind == "hold":
            r.pin(pin, 0)
            time.sleep(float(secs))
            r.pin(pin, 1)
        elif kind in clicks:
            for i in range(clicks[kind]):
                if i:
                    time.sleep(0.12)
                r.pin(pin, 0)
                time.sleep(0.12)
                r.pin(pin, 1)
        else:
            raise RuntimeError("kind: single, double, triple or hold")
        return {"ok": True}

    def _motion(self, r, secs):
        c = r.chip
        if c.is_gateway or c.pir_pin is None:
            raise RuntimeError(f"{c.name} has no PIR (extension = \"sensors\")")
        self.event(c, ui.yellow(f"🚶 motion for {secs:g}s"))
        r.pin(c.pir_pin, 1)

        def end():
            time.sleep(float(secs))
            try:
                r.pin(c.pir_pin, 0)
            except (RuntimeError, OSError):
                pass
        threading.Thread(target=end, daemon=True).start()
        return {"ok": True}

    # Running

    def run(self):
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
        signal.signal(signal.SIGINT, on_signal)
        signal.signal(signal.SIGTERM, on_signal)

        for r in self.chips.values():
            r.start()
        try:
            while not self.stopping.wait(0.5):
                pass
        finally:
            with self.print_lock:
                print(ui.dim("\nstopping …"), flush=True)
            threads = [threading.Thread(target=r.stop) for r in self.chips.values()]
            for t in threads:
                t.start()
            for t in threads:
                t.join()
            srv.close()
            try:
                control.unlink()
            except FileNotFoundError:
                pass


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
        if req.get("cmd") == "console":
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
        if req.get("cmd") != "console":
            s.close()
    return json.loads(data) if data else {"error": "no answer"}
