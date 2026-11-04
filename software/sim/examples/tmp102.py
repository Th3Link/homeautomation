"""An I2C part of your own: a TMP102 temperature sensor (16-bit registers,
high byte first: 0 the temperature, 1 the configuration). Put it on a node
in the topology:

    [[node.i2c]]
    bus = 1
    addr = 0x48
    model = "examples/tmp102.py:Tmp102"
    temperature = 22.5

then `./sim set taster-flur tmp102 temperature=30`, and
`./sim i2c taster-flur read 0x48 0 2` reads it as a driver would.
"""

from simlib.devices import Device


class Tmp102(Device):
    kind = "tmp102"
    VALUES = {"temperature": (" °C", 0.5, -55, 128)}
    DEFAULTS = {"temperature": 21.0}

    def __init__(self, addr=0x48, **kw):
        self.pointer = 0
        self.pos = 0
        self.config = 0x60A0
        super().__init__(addr, **kw)

    def register(self):
        if self.pointer == 0:
            return (round(self.values["temperature"] * 16) & 0xFFF) << 4
        return self.config if self.pointer == 1 else 0

    def start(self, write):
        self.pos = 0

    def write(self, b):
        if self.pos == 0:
            self.pointer = b & 3
        elif self.pointer == 1:
            self.config = (self.config << 8 | b) & 0xFFFF
        self.pos += 1

    def read(self):
        b = self.register() >> (8 * (1 - self.pos % 2)) & 0xFF
        self.pos += 1
        return b
