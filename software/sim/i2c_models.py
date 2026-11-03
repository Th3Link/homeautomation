#!/usr/bin/env python3
"""I2C devices for QEMU's i2c-remote (hw/i2c/i2c_remote.c), one TCP port
per I2C bus. QEMU connects as the client and asks one line at a time:

  Q aa      -> A/N   is there a device at aa?
  S aa W|R  -> A/N   start a write / read transfer
  W hh      -> A/N   byte written
  R         -> hh    byte read
  P                  stop

  i2c_models.py --bus 5600=pca9554:26,pca9554:27 --bus 5601=veml7700:10

Every change of an expander's outputs is printed.
"""
import argparse
import socket
import threading


class Pca9554:
    """8-bit I/O expander: 0 input, 1 output, 2 polarity, 3 direction."""

    def __init__(self, addr):
        self.addr = addr
        self.regs = [0xFF, 0xFF, 0x00, 0xFF]
        self.ptr = 0
        self.first = True

    def start(self, write):
        self.first = write

    def write(self, b):
        if self.first:
            self.ptr = b & 3
            self.first = False
            return
        before = self.regs[1] & ~self.regs[3] & 0xFF
        self.regs[self.ptr] = b
        after = self.regs[1] & ~self.regs[3] & 0xFF
        if self.ptr in (1, 3) and before != after:
            print(f"pca9554 {self.addr:02x}: outputs {after:08b}", flush=True)
        self.ptr = (self.ptr + 1) & 3

    def read(self):
        v = self.regs[self.ptr]
        self.ptr = (self.ptr + 1) & 3
        return v


class Veml7700:
    """Ambient light: 16-bit registers by command code (4 = ALS count)."""

    def __init__(self, addr, als=500):
        self.addr = addr
        self.regs = {0: 0x0001, 4: als, 5: als, 6: 0, 7: 0x0081}
        self.cmd = 0
        self.pos = 0
        self.wbuf = []

    def start(self, write):
        self.pos = 0
        self.wbuf = [] if write else self.wbuf

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
        v = self.regs.get(self.cmd, 0)
        b = (v >> (8 * self.pos)) & 0xFF
        self.pos += 1
        return b


MODELS = {"pca9554": Pca9554, "veml7700": Veml7700}
VERBOSE = False


def serve(port, devices):
    srv = socket.socket()
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", port))
    srv.listen(1)
    while True:
        conn, _ = srv.accept()
        f = conn.makefile("rw", newline="\n")
        current = None
        for line in f:
            if VERBOSE:
                print(f"{port}: {line.strip()}", flush=True)
            parts = line.split()
            if not parts:
                continue
            op = parts[0]
            answer = None
            if op == "Q":
                answer = "A" if int(parts[1], 16) in devices else "N"
            elif op == "S":
                current = devices.get(int(parts[1], 16))
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
        conn.close()


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--bus", action="append", required=True, help="PORT=model:addr,model:addr")
    p.add_argument("-v", "--verbose", action="store_true", help="print every line from QEMU")
    args = p.parse_args()
    global VERBOSE
    VERBOSE = args.verbose
    threads = []
    for spec in args.bus:
        port, devs = spec.split("=")
        devices = {}
        for d in devs.split(","):
            model, addr = d.split(":")
            devices[int(addr, 16)] = MODELS[model](int(addr, 16))
        print(f"bus on port {port}: " + ", ".join(f"{m}" for m in devs.split(",")), flush=True)
        t = threading.Thread(target=serve, args=(int(port), devices), daemon=True)
        t.start()
        threads.append(t)
    for t in threads:
        t.join()


if __name__ == "__main__":
    main()
