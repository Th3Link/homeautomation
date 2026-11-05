"""CAN frames on the host's (v)can interfaces, decoded the way common-core
reads them (can_id.rs, can_message_type.rs)."""

import socket
import struct

MESSAGES = {
    0: "Available", 1: "DeviceError", 2: "Restart", 3: "DeviceUid0", 4: "DeviceUid1",
    5: "DeviceIdType", 6: "DeviceGroup", 7: "ApplicationVersion", 8: "Baudrate",
    9: "Uptime", 10: "CustomString", 11: "PwmFrequency", 12: "RequestParameter",
    13: "ApplicationVersionString", 14: "UpdateSilence", 15: "FlashStart",
    16: "FlashSelect", 17: "FlashErase", 18: "FlashRead", 19: "FlashWrite",
    20: "FlashVerify", 21: "FlashProgress", 22: "FlashComplete", 30: "ButtonEvent",
    31: "TemperatureSensor", 41: "HwRev", 42: "ExtensionMode", 43: "Discovery",
    44: "DiscoveryAck", 90: "LampGroup", 128: "PirSensor", 129: "HumiditySensor",
    130: "Relais", 131: "RelaisState", 132: "Rollershutter", 133: "RollershutterState",
    134: "RelaisMode", 140: "AmbientLightSensor", 141: "AmbientLightSensorWhite",
    150: "Nightlight", 151: "PressureSensor", 152: "Co2Equivalent", 153: "VocBreath",
    154: "AirQuality", 155: "LogDownload", 156: "Ping", 157: "PingDisable", 158: "Echo",
    159: "Co2Sensor", 165: "HeatingCommand", 166: "HeatingState", 167: "SensorConfig",
    168: "OtaControl", 169: "OtaData", 170: "GasResistance",
}
DEVICE_TYPES = {
    2: "legacy-relais", 3: "legacy-lamps", 4: "button", 5: "relay", 6: "gateway",
    7: "shutter", 8: "ssr", 9: "heating",
}
BUTTON_STATES = {0: "released", 1: "pressed", 2: "hold", 3: "single", 4: "double", 5: "triple"}

# Housekeeping between gateway and nodes, shown only with --all.
CHATTER = {
    "Available", "Uptime", "Discovery", "DiscoveryAck", "Ping", "PingDisable", "Echo",
    "DeviceIdType", "ApplicationVersion", "ApplicationVersionString", "HwRev",
    "ExtensionMode", "CustomString", "DeviceUid0", "DeviceUid1", "RelaisMode",
    "RequestParameter", "SensorConfig",
}

# Sensor frames in the fixed-point layout ([sub id u48, raw u16], value =
# raw / 16; cancomponents-core sensor_board.rs): what, unit, signed.
FIXED_POINT = {
    "TemperatureSensor": ("temperature", " °C", True),
    "HumiditySensor": ("humidity", " %", False),
    "PressureSensor": ("pressure", " hPa", False),
    "Co2Sensor": ("CO2", " ppm", False),
    "Co2Equivalent": ("eCO2", " ppm", False),
    "VocBreath": ("bVOC", " ppm", False),
    "AirQuality": ("air quality", "", False),
    "GasResistance": ("gas", " kΩ", False),
}

FRAME = struct.Struct("=IB3x8s")
EFF_FLAG = 0x80000000
RTR_FLAG = 0x40000000


class Frame:
    def __init__(self, raw_id, data, rtr=False):
        self.raw_id = raw_id
        self.data = data
        self.rtr = rtr
        self.group = (raw_id >> 22) & 0x3F
        self.device_type = (raw_id >> 16) & 0x3F
        self.device_id = (raw_id >> 8) & 0xFF
        self.msg_type = raw_id & 0xFF

    @property
    def message(self):
        return MESSAGES.get(self.msg_type, f"msg{self.msg_type}")

    @property
    def device(self):
        return f"{DEVICE_TYPES.get(self.device_type, f'type{self.device_type}')} {self.device_id}"

    @property
    def sensor(self):
        """Which sensor a fixed-point frame is from: a DS18B20's serial, or
        the BME680's / SCD30's sub id (common-core sensor_id.rs)."""
        sub = int.from_bytes(self.data[:6], "little")
        if sub >> 8 == 0x680000:
            return f"bme680@{sub & 0xFF:02x}"
        if sub >> 8 == 0x300000:
            return f"scd30@{sub & 0xFF:02x}"
        return self.data[:6].hex()

    @property
    def value(self):
        """A fixed-point frame's value, or None."""
        if self.message in FIXED_POINT and len(self.data) >= 8:
            return int.from_bytes(self.data[6:8], "little", signed=FIXED_POINT[self.message][2]) / 16
        if self.message == "AmbientLightSensor" and len(self.data) >= 4:
            return int.from_bytes(self.data[:4], "little")
        return None

    @property
    def chatter(self):
        """Housekeeping, and the steps before what a button or PIR event
        means (pressed before single/double, every second of motion)."""
        if self.message in CHATTER:
            return True
        state = self.data[1] if len(self.data) >= 2 else None
        if self.message == "ButtonEvent":
            return state == 1
        if self.message == "PirSensor":
            return state == 2
        return False

    def describe(self):
        """The payload in words, for the messages worth reading."""
        d = self.data
        m = self.message
        if self.rtr:
            return f"{m} (request)"
        if m == "ButtonEvent" and len(d) >= 2:
            state = BUTTON_STATES.get(d[1], d[1])
            count = int.from_bytes(d[2:4], "little")
            return f"button {d[0]} {state}" + (f" #{count}" if count else "")
        if m == "PirSensor" and len(d) >= 2:
            return "motion" if d[1] else "motion over"
        if m in ("Relais", "RelaisState") and len(d) >= 2:
            relay = f"relay {d[5]}/{d[0]}" if len(d) >= 6 and d[5] else f"relay {d[0]}"
            if m == "RelaisState":
                return f"{relay} is {'on' if d[1] else 'off'}"
            return f"{relay} → {'on' if d[1] else 'off'}"
        if m in ("Rollershutter", "RollershutterState") and d:
            states = {0: "stop", 1: "up", 2: "down"}
            return f"{m} {' '.join(str(states.get(b, b)) for b in d[:2])}"
        if m == "AmbientLightSensor" and len(d) >= 4:
            return f"light {int.from_bytes(d[:4], 'little')} lx"
        if m in FIXED_POINT and len(d) >= 8:
            what, unit, signed = FIXED_POINT[m]
            value = int.from_bytes(d[6:8], "little", signed=signed) / 16
            return f"{what} {value:g}{unit}  {self.sensor}"
        if m == "Nightlight" and d:
            return f"night light → {d[0]}"
        if m == "LampGroup" and d:
            return f"lamps {d.hex(' ')}"
        return f"{m} {d.hex(' ')}".rstrip()


def open_bus(iface):
    s = socket.socket(socket.AF_CAN, socket.SOCK_RAW, socket.CAN_RAW)
    s.bind((iface,))
    return s


def read(sock):
    raw = sock.recv(FRAME.size)
    can_id, dlc, data = FRAME.unpack(raw)
    if not can_id & EFF_FLAG:
        return None
    return Frame(can_id & 0x1FFFFFFF, data[:dlc], bool(can_id & RTR_FLAG))


def write(sock, raw_id, data=b"", rtr=False):
    flags = EFF_FLAG | (RTR_FLAG if rtr else 0)
    sock.send(FRAME.pack(raw_id | flags, len(data), bytes(data).ljust(8, b"\0")))


def frame_id(device_type, device_id, message, group=0):
    """The 29-bit id (common-core can_id.rs): bit 28 marks the new format."""
    msg = message if isinstance(message, int) else MESSAGE_IDS[message]
    return 1 << 28 | (group & 0x3F) << 22 | (device_type & 0x3F) << 16 | (device_id & 0xFF) << 8 | msg


MESSAGE_IDS = {name: n for n, name in MESSAGES.items()}


def interface_exists(iface):
    try:
        socket.if_nametoindex(iface)
        return True
    except OSError:
        return False
