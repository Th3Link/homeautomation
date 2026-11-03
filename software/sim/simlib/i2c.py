"""I2C devices for QEMU's i2c-remote (hw/i2c/i2c_remote.c in the esp32-sim
QEMU). One socket per I2C bus; QEMU connects and asks one line at a time,
hex addresses and bytes:

  Q aa      -> A/N   is there a device at aa?
  S aa W|R  -> A/N   start a write / read transfer
  W hh      -> A/N   byte written
  R         -> hh    byte read
  P                  stop

QEMU waits for each answer, so the models answer right away.
"""

import os
import socket
import threading


class Pca9554:
    """8-bit I/O expander: registers 0 input, 1 output, 2 polarity,
    3 direction (1 = input). `changed(outputs)` gets the driven outputs."""

    def __init__(self, addr, changed=None):
        self.addr = addr
        self.regs = [0xFF, 0xFF, 0x00, 0xFF]
        self.ptr = 0
        self.first = True
        self.changed = changed

    @property
    def outputs(self):
        return self.regs[1] & ~self.regs[3] & 0xFF

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


class Veml7700:
    """Ambient light: 16-bit registers by command code. The ALS count
    (register 4) follows `lux` for the gain and integration time the
    firmware configured in register 0."""

    GAIN = {0b00: 1.0, 0b01: 2.0, 0b10: 1 / 8, 0b11: 1 / 4}
    IT_MS = {0b1100: 25, 0b1000: 50, 0b0000: 100, 0b0001: 200, 0b0010: 400, 0b0011: 800}

    def __init__(self, addr, lux=200.0):
        self.addr = addr
        self.regs = {0: 0x0001, 5: 0, 6: 0, 7: 0x0081}
        self.lux = lux
        self.cmd = 0
        self.pos = 0
        self.wbuf = []

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


class Bus:
    """The devices on one I2C bus, served on a Unix socket QEMU connects to
    (as a client that reconnects)."""

    def __init__(self, path, devices, trace=None):
        self.path = str(path)
        self.devices = {d.addr: d for d in devices}
        self.trace = trace
        try:
            os.unlink(self.path)
        except FileNotFoundError:
            pass
        self.srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.srv.bind(self.path)
        self.srv.listen(1)
        threading.Thread(target=self._serve, daemon=True).start()

    def close(self):
        self.srv.close()

    def _serve(self):
        while True:
            try:
                conn, _ = self.srv.accept()
            except OSError:
                return
            with conn:
                self._session(conn)

    def _session(self, conn):
        f = conn.makefile("rw", newline="\n")
        current = None
        for line in f:
            if self.trace:
                self.trace(line.strip())
            parts = line.split()
            if not parts:
                continue
            op, answer = parts[0], None
            if op == "Q":
                answer = "A" if int(parts[1], 16) in self.devices else "N"
            elif op == "S":
                current = self.devices.get(int(parts[1], 16))
                if current:
                    current.start(parts[2] == "W")
                answer = "A" if current else "N"
            elif op == "W":
                if current:
                    current.write(int(parts[1], 16))
                answer = "A" if current else "N"
            elif op == "R":
                answer = f"{current.read() if current else 0xFF:02x}"
            elif op == "P":
                current = None
            if answer is not None:
                f.write(answer + "\n")
                f.flush()
