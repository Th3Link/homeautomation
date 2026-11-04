"""The I2C parts around the chips, played by `sim up`: what the firmware
reads from them follows values you set (`./sim set`, scripts, the TUI), and
what it writes to them shows up as events.

A part is a `Device`: an address, `start`/`write`/`read`/`stop` for the
transfers QEMU forwards (simlib/i2c.py), `VALUES` that can be set, and
`state()` for status, TUI and scripts. A new part is a subclass here, or
your own class named in the topology (see the README).
"""

import struct


class Device:
    """One part on an I2C bus."""

    kind = "device"
    # Settable values: name -> (unit, step for the TUI, minimum, maximum).
    VALUES = {}
    DEFAULTS = {}

    def __init__(self, addr, name=None, **values):
        self.addr = addr
        self.name = name or self.kind
        self.values = dict(self.DEFAULTS)
        # Called with a short text when the firmware changed an output.
        self.notify = None
        self.set(**values)

    def set(self, **values):
        for key, value in values.items():
            if key not in self.VALUES:
                known = ", ".join(self.VALUES) or "nothing"
                raise ValueError(f"{self.name} has no {key!r} (it has {known})")
            _, _, lo, hi = self.VALUES[key]
            value = float(value)
            if not lo <= value <= hi:
                raise ValueError(f"{self.name}: {key} must be {lo:g} to {hi:g}")
            self.values[key] = value
        self.updated()

    def updated(self):
        """The values changed."""

    def state(self):
        """Values and outputs, JSON-able."""
        return {"kind": self.kind, "addr": self.addr, **self.values}

    def summary(self):
        units = {k: u for k, (u, *_) in self.VALUES.items()}
        return "  ".join(f"{v:g}{units.get(k, '')}" for k, v in self.values.items())

    def _notify(self, text):
        if self.notify:
            self.notify(self, text)

    # I2C, one transfer at a time: start (write or read), bytes, stop.

    def start(self, write):
        pass

    def write(self, b):
        pass

    def read(self):
        return 0xFF

    def stop(self):
        pass


class RegisterDevice(Device):
    """A part with 8-bit registers: a write sets the register pointer with
    its first byte. Bosch sensors then take register/value pairs
    (`PAIRS`), others the following registers (auto-increment). Reads go
    on from the pointer."""

    PAIRS = False
    SIZE = 256

    def __init__(self, addr, **kw):
        self.regs = bytearray(self.SIZE)
        self.ptr = 0
        self.pos = 0
        super().__init__(addr, **kw)

    def start(self, write):
        self.pos = 0
        self.writing = write

    def write(self, b):
        if self.pos == 0 or (self.PAIRS and self.pos % 2 == 0):
            self.ptr = b % self.SIZE
        else:
            self.store(self.ptr, b)
            if not self.PAIRS:
                self.ptr = (self.ptr + 1) % self.SIZE
        self.pos += 1

    def read(self):
        b = self.load(self.ptr)
        self.ptr = (self.ptr + 1) % self.SIZE
        return b

    def store(self, reg, value):
        self.regs[reg] = value

    def load(self, reg):
        return self.regs[reg]


def invert(f, target, lo, hi):
    """The integer x in [lo, hi] where the monotonic f(x) comes closest to
    target."""
    rising = f(hi) > f(lo)
    while hi - lo > 1:
        mid = (lo + hi) // 2
        if (f(mid) < target) == rising:
            lo = mid
        else:
            hi = mid
    return lo if abs(f(lo) - target) <= abs(f(hi) - target) else hi


# Relays


class Pca9554(Device):
    """8-bit I/O expander: registers 0 input, 1 output, 2 polarity,
    3 direction (1 = input). `changed(outputs)` gets the driven outputs."""

    kind = "pca9554"

    def __init__(self, addr, changed=None, **kw):
        self.regs = [0xFF, 0xFF, 0x00, 0xFF]
        self.ptr = 0
        self.first = True
        self.changed = changed
        super().__init__(addr, name=f"pca9554-{addr:02x}", **kw)

    @property
    def outputs(self):
        return self.regs[1] & ~self.regs[3] & 0xFF

    def state(self):
        return {"kind": self.kind, "addr": self.addr, "outputs": self.outputs}

    def summary(self):
        return f"outputs {self.outputs:08b}"

    def start(self, write):
        self.first = write

    def write(self, b):
        if self.first:
            self.ptr = b & 3
            self.first = False
            return
        before = self.outputs
        self.regs[self.ptr] = b
        if self.ptr in (1, 3) and before != self.outputs and self.changed:
            self.changed(self.outputs)
        self.ptr = (self.ptr + 1) & 3

    def read(self):
        v = self.regs[self.ptr]
        self.ptr = (self.ptr + 1) & 3
        return v


# The sensor extension


class Veml7700(Device):
    """Ambient light: 16-bit registers by command code. The ALS count
    (register 4) follows `lux` for the gain and integration time the
    firmware configured in register 0."""

    kind = "veml7700"
    VALUES = {"lux": (" lx", 10, 0, 120000)}
    DEFAULTS = {"lux": 200.0}
    GAIN = {0b00: 1.0, 0b01: 2.0, 0b10: 1 / 8, 0b11: 1 / 4}
    IT_MS = {0b1100: 25, 0b1000: 50, 0b0000: 100, 0b0001: 200, 0b0010: 400, 0b0011: 800}

    def __init__(self, addr=0x10, **kw):
        self.regs = {0: 0x0001, 5: 0, 6: 0, 7: 0x0081}
        self.cmd = 0
        self.pos = 0
        self.wbuf = []
        super().__init__(addr, **kw)

    @property
    def lux(self):
        return self.values["lux"]

    def counts(self):
        conf = self.regs.get(0, 0)
        gain = self.GAIN[(conf >> 11) & 3]
        it = self.IT_MS.get((conf >> 6) & 0xF, 100)
        # 0.0042 lx per count at gain 2 and 800 ms (Vishay application note).
        resolution = 0.0042 * (2 / gain) * (800 / it)
        return max(0, min(0xFFFF, round(self.lux / resolution)))

    def register(self, cmd):
        if cmd in (4, 5):
            return self.counts()
        return self.regs.get(cmd, 0)

    def start(self, write):
        self.pos = 0
        if write:
            self.wbuf = []

    def write(self, b):
        if self.pos == 0:
            self.cmd = b
        else:
            self.wbuf.append(b)
            if len(self.wbuf) == 2:
                self.regs[self.cmd] = self.wbuf[0] | self.wbuf[1] << 8
                self.wbuf = []
        self.pos += 1

    def read(self):
        b = (self.register(self.cmd) >> (8 * self.pos)) & 0xFF
        self.pos += 1
        return b


class Bme680(RegisterDevice):
    """Temperature, humidity, pressure and gas resistance (the bme68x
    crate's register map and float compensation). A forced measurement
    (ctrl_meas mode 01) fills the field registers with the raw values that
    compensate to the set values, using the crate's reference calibration;
    the gas reading is valid and heat-stable when the firmware ran the
    heater."""

    kind = "bme680"
    PAIRS = True
    VALUES = {
        "temperature": (" °C", 0.5, -40, 85),
        "humidity": (" %", 1, 0, 100),
        "pressure": (" hPa", 1, 300, 1100),
        "gas": (" kΩ", 5, 1, 2000),
    }
    DEFAULTS = {"temperature": 21.5, "humidity": 45.0, "pressure": 1013.25, "gas": 120.0}

    CAL = dict(h1=824, h2=1019, h3=0, h4=45, h5=20, h6=120, h7=-100, gh1=-30, gh2=-2500,
               gh3=4, t1=27504, t2=26435, t3=3, p1=36477, p2=-10685, p3=88, p4=10485,
               p5=-53, p6=30, p7=7, p8=-14600, p9=6000, p10=30, res_heat_range=1,
               res_heat_val=30, range_sw_err=-2)
    GAS_CORRECTION_1 = (0, 0, 0, 0, 0, -1, 0, -0.8, 0, 0, -0.2, -0.5, 0, -1, 0, 0)
    GAS_CORRECTION_2 = (0, 0, 0, 0, 0.1, 0.7, 0, -0.8, -0.1, 0, 0, 0, 0, 0, 0, 0)
    CHIP_ID = 0x61

    def __init__(self, addr=0x77, **kw):
        super().__init__(addr, **kw)
        self.reset()
        self.measurements = 0

    def reset(self):
        self.regs[:] = bytes(self.SIZE)
        self.regs[0xD0] = self.CHIP_ID
        self.regs[0xF0] = 0  # variant: BME680 (low gas)
        c = self.CAL
        b = bytearray(42)
        struct.pack_into("<hb", b, 0, c["t2"], c["t3"])
        struct.pack_into("<Hhb", b, 4, c["p1"], c["p2"], c["p3"])
        struct.pack_into("<hhbb", b, 10, c["p4"], c["p5"], c["p7"], c["p6"])
        struct.pack_into("<hhB", b, 18, c["p8"], c["p9"], c["p10"])
        b[23] = c["h2"] >> 4
        b[24] = (c["h2"] & 0x0F) << 4 | (c["h1"] & 0x0F)
        b[25] = c["h1"] >> 4
        struct.pack_into("<bbbBb", b, 26, c["h3"], c["h4"], c["h5"], c["h6"], c["h7"])
        struct.pack_into("<Hhbb", b, 31, c["t1"], c["gh2"], c["gh1"], c["gh3"])
        b[37] = c["res_heat_val"] & 0xFF
        b[39] = (c["res_heat_range"] & 3) << 4
        b[41] = (c["range_sw_err"] * 16) & 0xF0
        self.regs[0x8A:0x8A + 23] = b[:23]
        self.regs[0xE1:0xE1 + 14] = b[23:37]
        self.regs[0x00:0x05] = b[37:42]

    # The crate's float compensation (float.rs), and its inverse.

    def _t_fine(self, adc):
        c = self.CAL
        var1 = (adc / 16384 - c["t1"] / 1024) * c["t2"]
        x = adc / 131072 - c["t1"] / 8192
        return var1 + x * x * c["t3"] * 16

    def _pressure(self, adc, t_fine):
        c = self.CAL
        var1 = t_fine / 2 - 64000
        var2 = var1 * var1 * c["p6"] / 131072 + var1 * c["p5"] * 2
        var2 = var2 / 4 + c["p4"] * 65536
        var1 = (c["p3"] * var1 * var1 / 16384 + c["p2"] * var1) / 524288
        var1 = (1 + var1 / 32768) * c["p1"]
        p = 1048576 - adc
        p = (p - var2 / 4096) * 6250 / var1
        var1 = c["p9"] * p * p / 2147483648
        var2 = p * c["p8"] / 32768
        var3 = (p / 256) ** 3 * c["p10"] / 131072
        return p + (var1 + var2 + var3 + c["p7"] * 128) / 16

    def _humidity(self, adc, t_fine):
        c = self.CAL
        t = t_fine / 5120
        var1 = adc - (c["h1"] * 16 + c["h3"] / 2 * t)
        var2 = var1 * (c["h2"] / 262144 * (1 + c["h4"] / 16384 * t + c["h5"] / 1048576 * t * t))
        h = var2 + (c["h6"] / 16384 + c["h7"] / 2097152 * t) * var2 * var2
        return min(100.0, max(0.0, h))

    def _gas(self, ohms):
        """(adc, range) for a resistance."""
        var1 = 1340 + 5 * self.CAL["range_sw_err"]
        best = None
        for r in range(16):
            var2 = var1 * (1 + self.GAS_CORRECTION_1[r] / 100)
            var3 = 1 + self.GAS_CORRECTION_2[r] / 100
            x = 1 / (ohms * var3 * 0.000000125 * (1 << r))
            adc = round(512 + var2 * (x - 1))
            if 0 <= adc <= 1023 and (best is None or abs(adc - 512) < abs(best[0] - 512)):
                best = (adc, r)
        return best or (1023, 15)

    def raw(self):
        v = self.values
        t_adc = invert(self._t_fine, v["temperature"] * 5120, 0, (1 << 20) - 1)
        t_fine = self._t_fine(t_adc)
        p_adc = invert(lambda a: self._pressure(a, t_fine), v["pressure"] * 100, 0, (1 << 20) - 1)
        h_adc = invert(lambda a: self._humidity(a, t_fine), v["humidity"], 0, 0xFFFF)
        gas_adc, gas_range = self._gas(v["gas"] * 1000)
        return t_adc, p_adc, h_adc, gas_adc, gas_range

    def store(self, reg, value):
        if reg == 0xE0:
            if value == 0xB6:
                self.reset()
            return
        if reg in (0xD0, 0xF0) or 0x8A <= reg < 0xA1 or 0xE1 <= reg < 0xEF or reg < 5:
            return  # read-only
        self.regs[reg] = value
        if reg == 0x74 and value & 3 == 1:
            self.measure()

    def measure(self):
        t_adc, p_adc, h_adc, gas_adc, gas_range = self.raw()
        self.measurements = (self.measurements + 1) & 0xFF
        heater = self.regs[0x71] & 0x10  # run_gas
        f = bytearray(17)
        f[0] = 0x80  # new data, gas index 0
        f[1] = self.measurements
        f[2], f[3], f[4] = p_adc >> 12, (p_adc >> 4) & 0xFF, (p_adc & 0xF) << 4
        f[5], f[6], f[7] = t_adc >> 12, (t_adc >> 4) & 0xFF, (t_adc & 0xF) << 4
        f[8], f[9] = h_adc >> 8, h_adc & 0xFF
        f[13] = gas_adc >> 2
        f[14] = (gas_adc & 3) << 6 | (0x30 if heater else 0) | gas_range
        self.regs[0x1D:0x1D + 17] = f
        self.regs[0x74] &= ~3  # back to sleep


class Bme280(RegisterDevice):
    """Temperature, humidity and pressure (Bosch BME280, chip id 0x60, the
    datasheet's float compensation). Each read of the data registers
    returns the set values; the node firmware has no driver for it (it
    looks for a BME680 and logs that there is none)."""

    kind = "bme280"
    PAIRS = True
    VALUES = {
        "temperature": (" °C", 0.5, -40, 85),
        "humidity": (" %", 1, 0, 100),
        "pressure": (" hPa", 1, 300, 1100),
    }
    DEFAULTS = {"temperature": 21.5, "humidity": 45.0, "pressure": 1013.25}
    CAL = dict(t1=27504, t2=26435, t3=-1000, p1=36477, p2=-10685, p3=3024, p4=2855, p5=140,
               p6=-7, p7=15500, p8=-14600, p9=6000, h1=75, h2=362, h3=0, h4=324, h5=0, h6=30)

    def __init__(self, addr=0x76, **kw):
        super().__init__(addr, **kw)
        self.reset()

    def reset(self):
        c = self.CAL
        self.regs[:] = bytes(self.SIZE)
        self.regs[0xD0] = 0x60
        struct.pack_into("<Hhh", self.regs, 0x88, c["t1"], c["t2"], c["t3"])
        struct.pack_into("<Hhhhhhhhh", self.regs, 0x8E, *(c[f"p{i}"] for i in range(1, 10)))
        self.regs[0xA1] = c["h1"]
        struct.pack_into("<hB", self.regs, 0xE1, c["h2"], c["h3"])
        self.regs[0xE4] = (c["h4"] >> 4) & 0xFF
        self.regs[0xE5] = (c["h4"] & 0xF) | (c["h5"] & 0xF) << 4
        self.regs[0xE6] = (c["h5"] >> 4) & 0xFF
        struct.pack_into("<b", self.regs, 0xE7, c["h6"])
        self.updated()

    def _t_fine(self, adc):
        c = self.CAL
        var1 = (adc / 16384 - c["t1"] / 1024) * c["t2"]
        var2 = (adc / 131072 - c["t1"] / 8192) ** 2 * c["t3"]
        return var1 + var2

    def _pressure(self, adc, t_fine):
        c = self.CAL
        var1 = t_fine / 2 - 64000
        var2 = var1 * var1 * c["p6"] / 32768 + var1 * c["p5"] * 2
        var2 = var2 / 4 + c["p4"] * 65536
        var1 = (c["p3"] * var1 * var1 / 524288 + c["p2"] * var1) / 524288
        var1 = (1 + var1 / 32768) * c["p1"]
        p = 1048576 - adc
        p = (p - var2 / 4096) * 6250 / var1
        return p + (c["p9"] * p * p / 2147483648 + p * c["p8"] / 32768 + c["p7"]) / 16

    def _humidity(self, adc, t_fine):
        c = self.CAL
        h = t_fine - 76800
        h = (adc - (c["h4"] * 64 + c["h5"] / 16384 * h)) * (
            c["h2"] / 65536 * (1 + c["h6"] / 67108864 * h * (1 + c["h3"] / 67108864 * h)))
        h = h * (1 - c["h1"] * h / 524288)
        return min(100.0, max(0.0, h))

    def updated(self):
        if not hasattr(self, "regs") or self.regs[0xD0] != 0x60:
            return
        v = self.values
        t_adc = invert(self._t_fine, v["temperature"] * 5120, 0, (1 << 20) - 1)
        t_fine = self._t_fine(t_adc)
        p_adc = invert(lambda a: self._pressure(a, t_fine), v["pressure"] * 100, 0, (1 << 20) - 1)
        h_adc = invert(lambda a: self._humidity(a, t_fine), v["humidity"], 0, 0xFFFF)
        self.regs[0xF7:0xFF] = bytes((p_adc >> 12, (p_adc >> 4) & 0xFF, (p_adc & 0xF) << 4,
                                      t_adc >> 12, (t_adc >> 4) & 0xFF, (t_adc & 0xF) << 4,
                                      h_adc >> 8, h_adc & 0xFF))

    def store(self, reg, value):
        if reg == 0xE0:
            if value == 0xB6:
                self.reset()
        elif reg in (0xF2, 0xF4, 0xF5):
            self.regs[reg] = value


def sensirion_crc(data):
    crc = 0xFF
    for b in data:
        crc ^= b
        for _ in range(8):
            crc = (crc << 1) ^ 0x31 if crc & 0x80 else crc << 1
            crc &= 0xFF
    return crc


class Scd30(Device):
    """CO2, temperature and humidity (Sensirion SCD30, libscd's command
    set): 16-bit commands, words with a CRC each. While measuring
    continuously, a new measurement is ready every interval."""

    kind = "scd30"
    VALUES = {
        "co2": (" ppm", 50, 0, 40000),
        "temperature": (" °C", 0.5, -40, 70),
        "humidity": (" %", 1, 0, 100),
    }
    DEFAULTS = {"co2": 650.0, "temperature": 22.0, "humidity": 45.0}

    def __init__(self, addr=0x61, **kw):
        import time
        self._time = time.monotonic
        self.measuring = False
        self.interval = 2
        self.pressure = 0
        self.asc = 0
        self.temperature_offset = 0
        self.altitude = 0
        self.ready_at = None
        self.cmd = None
        self.inbuf = []
        self.out = []
        super().__init__(addr, **kw)

    def state(self):
        return {**super().state(), "measuring": self.measuring, "pressure_hpa": self.pressure}

    @staticmethod
    def _words(*words):
        out = []
        for w in words:
            b = [w >> 8 & 0xFF, w & 0xFF]
            out += b + [sensirion_crc(b)]
        return out

    def _floats(self, *values):
        words = []
        for v in values:
            raw = struct.unpack(">I", struct.pack(">f", v))[0]
            words += [raw >> 16, raw & 0xFFFF]
        return self._words(*words)

    def _ready(self):
        return self.measuring and self.ready_at is not None and self._time() >= self.ready_at

    def start(self, write):
        if not write:
            self.execute()  # a read after a repeated start
        self.inbuf = []

    def write(self, b):
        self.inbuf.append(b)

    def stop(self):
        self.execute()

    def execute(self):
        if not self.inbuf:
            return
        data = self.inbuf
        self.inbuf = []
        if len(data) < 2:
            return
        cmd = data[0] << 8 | data[1]
        arg = data[2] << 8 | data[3] if len(data) >= 5 and sensirion_crc(data[2:4]) == data[4] else None
        self.out = []
        if cmd == 0x0010:  # start continuous measurement
            self.measuring = True
            self.pressure = arg or 0
            self.ready_at = self._time() + self.interval
            self._notify(f"measuring every {self.interval} s" +
                         (f", compensated for {self.pressure} hPa" if self.pressure else ""))
        elif cmd == 0x0104:
            self.measuring = False
        elif cmd == 0x4600:
            if arg is not None:
                self.interval = max(2, min(1800, arg))
            else:
                self.out = self._words(self.interval)
        elif cmd == 0x0202:
            self.out = self._words(1 if self._ready() else 0)
        elif cmd == 0x0300:
            if self._ready():
                v = self.values
                self.out = self._floats(v["co2"], v["temperature"], v["humidity"])
                self.ready_at = self._time() + self.interval
            else:
                self.out = self._words(0, 0, 0, 0, 0, 0)
        elif cmd == 0x5306:
            if arg is not None:
                self.asc = arg
            else:
                self.out = self._words(self.asc)
        elif cmd == 0x5403:
            if arg is not None:
                self.temperature_offset = arg
            else:
                self.out = self._words(self.temperature_offset)
        elif cmd == 0x5102:
            if arg is not None:
                self.altitude = arg
            else:
                self.out = self._words(self.altitude)
        elif cmd == 0xD100:
            self.out = self._words(0x0342)  # firmware 3.66
        elif cmd == 0xD304:
            self.measuring = False

    def read(self):
        return self.out.pop(0) if self.out else 0xFF


class NightLight(Device):
    """The night light: a PCF8574A, its port the LEDs on P0-P2 (active
    low). Shows the level the firmware set (0-3)."""

    kind = "nightlight"

    def __init__(self, addr=0x38, **kw):
        self.port = 0xFF
        super().__init__(addr, **kw)

    @property
    def level(self):
        return sum(1 for bit in range(3) if not self.port >> bit & 1)

    def state(self):
        return {"kind": self.kind, "addr": self.addr, "port": self.port, "level": self.level}

    def summary(self):
        return "●" * self.level + "○" * (3 - self.level)

    def start(self, write):
        self.first = write

    def write(self, b):
        before = self.level
        self.port = b
        if self.level != before:
            self._notify(f"🌙 night light {self.summary()} ({self.level})")

    def read(self):
        return self.port


class Pca9685(RegisterDevice):
    """A PWM board of the PWM extension: 16 channels, 12 bits. Shows the
    duty of each channel the firmware set, in percent."""

    kind = "pca9685"

    def __init__(self, addr, **kw):
        super().__init__(addr, **kw)
        self.regs[0x00] = 0x11  # MODE1: sleep, ALLCALL
        self.regs[0x01] = 0x04
        self.regs[0xFE] = 0x1E
        self.shown = [0.0] * 16

    def duty(self, channel):
        on_l, on_h, off_l, off_h = self.regs[6 + 4 * channel:10 + 4 * channel]
        if on_h & 0x10:
            return 100.0
        if off_h & 0x10:
            return 0.0
        on = (on_h & 0xF) << 8 | on_l
        off = (off_h & 0xF) << 8 | off_l
        return ((off - on) % 4096) / 40.96

    @property
    def frequency(self):
        return round(25_000_000 / (4096 * (self.regs[0xFE] + 1)))

    def state(self):
        return {"kind": self.kind, "addr": self.addr, "frequency": self.frequency,
                "sleeping": bool(self.regs[0] & 0x10), "duty": [self.duty(c) for c in range(16)]}

    def summary(self):
        lit = [f"{c}:{d:.0f}%" for c, d in enumerate(self.duty(c) for c in range(16)) if d]
        return " ".join(lit) or "all off"

    def write(self, b):
        auto = self.regs[0] & 0x20
        if self.pos == 0:
            self.ptr = b
        else:
            self.store(self.ptr, b)
            if auto:
                self.ptr = (self.ptr + 1) & 0xFF
        self.pos += 1

    def stop(self):
        now = [self.duty(c) for c in range(16)]
        changed = [c for c in range(16) if abs(now[c] - self.shown[c]) >= 0.05]
        self.shown = now
        if changed and not self.regs[0] & 0x10:
            parts = [f"{c} {now[c]:.0f}%" for c in changed]
            self._notify("💡 pwm " + ", ".join(parts))


# DS18B20s aren't on I2C: QEMU's GPIO model plays them (1-Wire timing is
# microseconds). This only keeps their values and builds ROM codes.


def onewire_crc(data):
    crc = 0
    for b in data:
        for _ in range(8):
            mix = (crc ^ b) & 1
            crc >>= 1
            if mix:
                crc ^= 0x8C
            b >>= 1
    return crc


class Ds18b20(Device):
    """A DS18B20 on the node's 1-Wire header. `plugged` 0 pulls it from the
    bus (the firmware reports it lost after three cycles)."""

    kind = "ds18b20"
    VALUES = {"temperature": (" °C", 0.5, -55, 125), "plugged": ("", 1, 0, 1)}
    DEFAULTS = {"temperature": 21.0, "plugged": 1.0}

    def __init__(self, serial, name, **kw):
        body = bytes([0x28]) + serial.to_bytes(6, "little")
        self.rom = body + bytes([onewire_crc(body)])
        super().__init__(None, name=name, **kw)

    @property
    def rom_hex(self):
        return self.rom.hex()

    def state(self):
        return {"kind": self.kind, "rom": self.rom_hex, **self.values}

    def summary(self):
        t = f"{self.values['temperature']:g} °C"
        return t if self.values["plugged"] else f"{t}, unplugged"


KINDS = {cls.kind: cls for cls in (Veml7700, Bme680, Bme280, Scd30, NightLight, Pca9685)}
# The parts of the light switch's sensor extension, where they are.
SENSOR_ADDRESSES = {"veml7700": 0x10, "bme680": 0x77, "bme280": 0x76, "scd30": 0x61,
                    "nightlight": 0x38}
DEFAULT_SENSORS = ("veml7700", "bme680", "nightlight")

