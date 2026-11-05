"""The simulated installation: gateways and nodes from a TOML file, and what
the firmware does with each pin and I2C bus (cancomponents-hardware
src/bin/main.rs, extension.rs, sensors.rs, pwm.rs, temperature.rs)."""

import hashlib
import os
import tomllib

from . import devices
from dataclasses import dataclass, field
from pathlib import Path

SIM_DIR = Path(__file__).resolve().parent.parent
REPO = SIM_DIR.parent.parent

# Device types (common-core device_type.rs), the hardware revisions the node
# firmware has a pin mapping for, and the default relay mode.
NODE_TYPES = {
    "button": {"device_type": 4, "hwrevs": (1, 2), "hwrev": 1, "relays": False},
    "relay": {"device_type": 5, "hwrevs": (1, 2), "hwrev": 2, "relays": True,
              "relay_mode": "relais", "onewire": True},
    "shutter": {"device_type": 7, "hwrevs": (1,), "hwrev": 1, "relays": True,
                "relay_mode": "software_rollershutter"},
    "heating": {"device_type": 9, "hwrevs": (1, 2), "hwrev": 2, "relays": True,
                "relay_mode": "relais", "onewire": True},
}
RELAY_MODES = ("off", "relais", "software_rollershutter", "hardware_rollershutter")
EXTENSIONS = ("off", "button", "sensors", "pwm")
# The relay board's 1-wire header (DS18B20s).
ONEWIRE_PIN = 4

# Buttons 0-3 on the board, active low with pull-ups.
BUTTON_PINS = {1: (33, 35, 12, 34), 2: (25, 26, 5, 15)}
# The extension header: buttons 4-7 in "button" mode; in "sensors" mode
# SDA, SCL (I2C1), 1-wire and the PIR (active high, pull-down); in "pwm"
# mode SDA and SCL of the PCA9685 boards.
EXTENSION_PINS = {
    ("button", 1): (4, 16, 17, 18),
    ("button", 2): (4, 16, 17, 18),
    ("relay", 1): (15, 16, 17, 18),
    ("relay", 2): (25, 26, 5, 15),
    ("shutter", 1): (15, 16, 17, 18),
    ("heating", 1): (15, 16, 17, 18),
    ("heating", 2): (25, 26, 5, 15),
}


class TopologyError(Exception):
    pass


@dataclass
class Chip:
    name: str
    kind: str  # "gateway" or a NODE_TYPES key
    bus: str
    index: int  # position in the file, for the MAC address
    console: list = field(default_factory=list)

    @property
    def is_gateway(self):
        return self.kind == "gateway"

    @property
    def mac(self):
        """Locally administered and unique per chip."""
        return bytes((0x02, 0x53, 0x49, 0x4D, self.index >> 8, self.index & 0xFF))

    @property
    def mac_text(self):
        return ":".join(f"{b:02x}" for b in self.mac)


@dataclass
class Gateway(Chip):
    https: int = 8443
    http: int = 8080

    @property
    def url(self):
        return f"https://127.0.0.1:{self.https}"

    def setup_commands(self):
        return list(self.console)


@dataclass
class Node(Chip):
    device_id: int = 0
    hwrev: int = 1
    extension: str = "off"
    relay_mode: str = "off"
    # The sensor extension's parts, and initial values per part.
    sensors: tuple = devices.DEFAULT_SENSORS
    values: dict = field(default_factory=dict)
    pwm_boards: int = 1
    # DS18B20s: {"name", "serial", "temperature"}.
    ds18b20: list = field(default_factory=list)
    # Your own I2C devices: {"bus", "addr", "model", values…}.
    custom: list = field(default_factory=list)

    @property
    def device_type(self):
        return NODE_TYPES[self.kind]["device_type"]

    @property
    def has_relays(self):
        return NODE_TYPES[self.kind]["relays"]

    @property
    def has_sensors(self):
        return self.extension == "sensors"

    @property
    def has_onewire(self):
        return NODE_TYPES[self.kind].get("onewire", False)

    @property
    def lux(self):
        return self.values.get("veml7700", {}).get("lux", devices.Veml7700.DEFAULTS["lux"])

    def i2c_devices(self):
        """(bus, kind, address, name, initial values) of the parts on the
        I2C buses besides the relay expanders."""
        out = []
        if self.has_sensors:
            for kind in self.sensors:
                out.append((1, kind, devices.SENSOR_ADDRESSES[kind], kind, self.values.get(kind, {})))
        if self.extension == "pwm":
            for bank in range(1, self.pwm_boards + 1):
                out.append((1, "pca9685", 0x3F + bank, f"pwm-{bank}", {}))
        for c in self.custom:
            values = {k: v for k, v in c.items() if k not in ("bus", "addr", "model", "name")}
            out.append((c["bus"], c["model"], c["addr"], c.get("name", c["model"].split(":")[-1].lower()),
                        values))
        return out

    @property
    def can_key(self):
        """The CAN id without the message type (common-core can_id.rs)."""
        return 1 << 28 | (self.device_type & 0x3F) << 16 | self.device_id << 8

    def button_pin(self, button):
        if 0 <= button <= 3:
            return BUTTON_PINS[self.hwrev][button] if self.kind == "button" else None
        if 4 <= button <= 7 and self.extension == "button":
            return EXTENSION_PINS[(self.kind, self.hwrev)][button - 4]
        return None

    @property
    def buttons(self):
        return [b for b in range(8) if self.button_pin(b) is not None]

    @property
    def pir_pin(self):
        return EXTENSION_PINS[(self.kind, self.hwrev)][3] if self.has_sensors else None

    def setup_commands(self):
        """Console commands that make a blank node this one."""
        commands = [
            f"device-type {self.device_type}",
            f"hwrev {self.hwrev}",
            f"device-id {self.device_id}",
            f"extension-mode {self.extension}",
        ]
        if self.has_relays:
            commands.append(f"relais-mode {self.relay_mode}")
        commands.append(f"custom-string {self.name[:8]}")
        return commands + list(self.console)


class Topology:
    def __init__(self, path):
        self.path = Path(path).resolve()
        try:
            data = tomllib.loads(self.path.read_text())
        except FileNotFoundError:
            raise TopologyError(f"{path}: not found") from None
        except tomllib.TOMLDecodeError as e:
            raise TopologyError(f"{path}: {e}") from None
        # Firmware other than the release builds, relative to this file.
        self.firmware = {
            kind: (self.path.parent / data["firmware"][kind]).resolve()
            for kind in ("node", "gateway") if kind in data.get("firmware", {})
        }
        self.gateways = []
        self.nodes = []
        index = 1
        for i, g in enumerate(data.get("gateway", [])):
            self.gateways.append(Gateway(
                name=self._name(g), kind="gateway", index=index,
                bus=g.get("bus", f"vcan{i}"), https=g.get("https", 8443 + i),
                http=g.get("http", 8080 + i), console=g.get("console", [])))
            index += 1
        by_name = {g.name: g for g in self.gateways}
        for n in data.get("node", []):
            name = self._name(n)
            kind = n.get("type")
            if kind not in NODE_TYPES:
                raise TopologyError(f"{name}: type must be one of {', '.join(NODE_TYPES)}")
            spec = NODE_TYPES[kind]
            if "gateway" in n:
                if n["gateway"] not in by_name:
                    raise TopologyError(f"{name}: no gateway {n['gateway']!r}")
                bus = by_name[n["gateway"]].bus
            else:
                bus = n.get("bus", "vcan0")
            node = Node(
                name=name, kind=kind, bus=bus, index=index,
                console=n.get("console", []), device_id=n.get("id", 0),
                hwrev=n.get("hwrev", spec["hwrev"]),
                extension=n.get("extension", "off"),
                relay_mode=n.get("relay_mode", spec.get("relay_mode", "off")),
                pwm_boards=n.get("pwm_boards", 1))
            self._parts(node, n)
            if not 1 <= node.device_id <= 255:
                raise TopologyError(f"{name}: id must be 1-255")
            if node.hwrev not in spec["hwrevs"]:
                raise TopologyError(f"{name}: a {kind} has hwrev {spec['hwrevs']}")
            if node.extension not in EXTENSIONS:
                raise TopologyError(f"{name}: extension must be one of {', '.join(EXTENSIONS)}")
            if node.relay_mode not in RELAY_MODES:
                raise TopologyError(f"{name}: relay_mode must be one of {', '.join(RELAY_MODES)}")
            self.nodes.append(node)
            index += 1
        self._check_unique()

    def _parts(self, node, n):
        name = node.name
        sensors = n.get("sensors", list(devices.DEFAULT_SENSORS))
        for kind in sensors:
            if kind not in devices.SENSOR_ADDRESSES:
                raise TopologyError(f"{name}: sensors are {', '.join(devices.SENSOR_ADDRESSES)}")
        if "bme680" in sensors and "bme280" in sensors:
            raise TopologyError(f"{name}: the BME680 and BME280 can't both be on the board")
        node.sensors = tuple(sensors)
        if "lux" in n:
            node.values["veml7700"] = {"lux": float(n["lux"])}
        for kind, cls in devices.KINDS.items():
            if isinstance(n.get(kind), dict):
                values = n[kind]
                bad = [k for k in values if k not in cls.VALUES]
                if bad:
                    raise TopologyError(f"{name}: a {kind} has no {', '.join(bad)} "
                                        f"(it has {', '.join(cls.VALUES) or 'nothing to set'})")
                node.values[kind] = values
        if not 1 <= node.pwm_boards <= 16:
            raise TopologyError(f"{name}: pwm_boards must be 1-16")
        sensors_1w = n.get("ds18b20", [])
        if sensors_1w and not node.has_onewire:
            raise TopologyError(f"{name}: only relay and heating boards have the 1-wire header")
        for i, entry in enumerate(sensors_1w):
            entry = entry if isinstance(entry, dict) else {"temperature": entry}
            serial = entry.get("serial", node.index << 16 | (i + 1))
            node.ds18b20.append({"name": entry.get("name", f"ds18b20-{i}"), "serial": serial,
                                 "temperature": float(entry.get("temperature", 21.0))})
        for c in n.get("i2c", []):
            if not {"bus", "addr", "model"} <= set(c):
                raise TopologyError(f"{name}: an [[node.i2c]] device needs bus, addr and model")
            if c["bus"] not in (0, 1):
                raise TopologyError(f"{name}: I2C bus 0 or 1")
            node.custom.append(c)
        taken = {}
        for bus, _kind, addr, part, _ in node.i2c_devices():
            if (bus, addr) in taken:
                raise TopologyError(f"{name}: {part} and {taken[bus, addr]} both at 0x{addr:02x}")
            taken[bus, addr] = part
        if node.has_relays and any(b == 0 and a in (0x26, 0x27) for b, a in taken):
            raise TopologyError(f"{name}: 0x26 and 0x27 on bus 0 are the relay expanders")

    def _name(self, entry):
        name = entry.get("name", "")
        if not name or not all(c.isalnum() or c in "-_" for c in name):
            raise TopologyError(f"{name!r}: a name of letters, digits, - and _ is needed")
        return name

    def _check_unique(self):
        names, keys, ports = set(), set(), set()
        for chip in self.chips:
            if chip.name in names:
                raise TopologyError(f"{chip.name}: the name is used twice")
            names.add(chip.name)
        for node in self.nodes:
            key = (node.bus, node.device_type, node.device_id)
            if key in keys:
                raise TopologyError(f"{node.name}: another {node.kind} on {node.bus} has id {node.device_id}")
            keys.add(key)
        for gw in self.gateways:
            for port in (gw.https, gw.http):
                if port in ports:
                    raise TopologyError(f"{gw.name}: port {port} is used twice")
                ports.add(port)
        buses = [g.bus for g in self.gateways]
        if len(buses) != len(set(buses)):
            raise TopologyError("two gateways on one bus: each gateway needs its own")

    @property
    def chips(self):
        return self.gateways + self.nodes

    @property
    def buses(self):
        return sorted({c.bus for c in self.chips})

    def chip(self, name):
        for c in self.chips:
            if c.name == name:
                return c
        matches = [c for c in self.chips if c.name.startswith(name)]
        if len(matches) == 1:
            return matches[0]
        known = ", ".join(c.name for c in self.chips)
        raise TopologyError(f"no chip {name!r} ({known})")

    def select(self, names):
        return [self.chip(n) for n in names] if names else self.chips

    def node_by_can(self, bus, device_type, device_id):
        for n in self.nodes:
            if n.bus == bus and n.device_type == device_type and n.device_id == device_id:
                return n
        return None

    @property
    def state_dir(self):
        """Images and logs, next to the topology file."""
        return self.path.parent / ".sim" / self.path.stem

    @property
    def run_dir(self):
        """Sockets: short paths (a socket path has at most 107 bytes)."""
        base = os.environ.get("XDG_RUNTIME_DIR") or f"/tmp/sim-{os.getuid()}"
        tag = hashlib.sha1(str(self.path).encode()).hexdigest()[:8]
        return Path(base) / f"ha-sim-{tag}"
