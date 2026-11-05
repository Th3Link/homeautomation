"""I2C buses for QEMU's i2c-remote (hw/i2c/i2c_remote.c in the esp32-sim
QEMU). One socket per I2C bus; QEMU connects and asks one line at a time,
hex addresses and bytes:

  Q aa      -> A/N   is there a device at aa?
  S aa W|R  -> A/N   start a write / read transfer (again: repeated start)
  W hh      -> A/N   byte written
  R         -> hh    byte read
  P                  stop

QEMU waits for each answer, so the models (simlib/devices.py) answer right
away. A bus also takes transfers from outside (`transfer`, behind
`./sim i2c`), one at a time with the firmware's, and reports every
finished transaction to `trace` as text: "77 W 74 25" or
"77 W 1d R 80 01 49" for a write and a read with a repeated start.
"""

import os
import socket
import threading


class Transaction:
    """The transfers between a start and the stop, for the trace."""

    def __init__(self):
        self.parts = []

    def start(self, addr, write, acked):
        self.parts.append([f"{addr:02x}", "W" if write else "R"] + ([] if acked else ["NACK"]))

    def byte(self, b):
        if self.parts:
            self.parts[-1].append(f"{b:02x}")

    def text(self):
        out = []
        for i, p in enumerate(self.parts):
            out += p if i == 0 or p[0] != self.parts[0][0] else p[1:]
        return " ".join(out)


class Bus:
    """The devices on one I2C bus, served on a Unix socket QEMU connects to
    (as a client that reconnects)."""

    def __init__(self, path, number, devices, trace=None):
        self.path = str(path)
        self.number = number
        self.devices = {d.addr: d for d in devices}
        self.trace = trace
        # Held from a start to the stop, so a transfer from outside never
        # lands inside one of the firmware's.
        self.lock = threading.Lock()
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

    def add(self, device):
        with self.lock:
            self.devices[device.addr] = device

    def _serve(self):
        while True:
            try:
                conn, _ = self.srv.accept()
            except OSError:
                return
            with conn:
                session = Session(self)
                try:
                    session.run(conn)
                except OSError:
                    pass
                finally:
                    session.close()

    def transfer(self, addr, write=b"", read=0, timeout=2.0):
        """A transaction from outside: `write` bytes, then (with a repeated
        start) `read` bytes. Returns the bytes read, or None if nothing
        answers at `addr`."""
        if not self.lock.acquire(timeout=timeout):
            raise TimeoutError(f"I2C bus {self.number} stayed busy")
        try:
            device = self.devices.get(addr)
            tx = Transaction()
            data = None
            if device:
                if write or not read:
                    device.start(True)
                    tx.start(addr, True, True)
                    for b in write:
                        device.write(b)
                        tx.byte(b)
                data = b""
                if read:
                    device.start(False)
                    tx.start(addr, False, True)
                    data = bytes(device.read() for _ in range(read))
                    for b in data:
                        tx.byte(b)
                device.stop()
            else:
                tx.start(addr, bool(write) or not read, False)
        finally:
            self.lock.release()
        if self.trace:
            self.trace(self.number, tx.text() + "  (from outside)")
        return data


class Session:
    """One QEMU connection: answers its lines, holds the bus lock from a
    start to the stop."""

    def __init__(self, bus):
        self.bus = bus
        self.current = None
        self.tx = None

    def close(self):
        # QEMU went away inside a transaction.
        if self.tx is not None:
            self.tx = None
            self.bus.lock.release()

    def run(self, conn):
        f = conn.makefile("rw", newline="\n")
        for line in f:
            parts = line.split()
            if not parts:
                continue
            answer = self.line(parts)
            if answer is not None:
                f.write(answer + "\n")
                f.flush()

    def line(self, parts):
        bus, op = self.bus, parts[0]
        if op == "Q":
            return "A" if int(parts[1], 16) in bus.devices else "N"
        if op == "S":
            if self.tx is None:
                bus.lock.acquire()
                self.tx = Transaction()
            addr = int(parts[1], 16)
            write = parts[2] == "W"
            if self.current and self.current.addr != addr:
                self.current.stop()
            self.current = bus.devices.get(addr)
            if self.current:
                self.current.start(write)
            self.tx.start(addr, write, self.current is not None)
            return "A" if self.current else "N"
        if op == "W":
            b = int(parts[1], 16)
            if self.current:
                self.current.write(b)
            if self.tx:
                self.tx.byte(b)
            return "A" if self.current else "N"
        if op == "R":
            b = self.current.read() if self.current else 0xFF
            if self.tx:
                self.tx.byte(b)
            return f"{b:02x}"
        if op == "P":
            if self.current:
                self.current.stop()
            self.current = None
            if self.tx is not None:
                text = self.tx.text()
                self.tx = None
                bus.lock.release()
                if bus.trace:
                    bus.trace(bus.number, text)
        return None
