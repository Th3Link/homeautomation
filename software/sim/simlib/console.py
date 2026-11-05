"""A chip's serial console in the terminal (`sim console`, `c` in the TUI)."""

import json
import os
import select
import sys
import termios
import tty

from . import supervisor, ui


def attach(topo, name):
    """Until Ctrl-]. Returns an error text, or None."""
    sock = supervisor.request(topo, {"cmd": "console", "chip": name})
    if sock is None:
        return "the simulation isn't running"
    f = sock.makefile("rb")
    answer = json.loads(f.readline() or b"{}")
    if "error" in answer:
        sock.close()
        return answer["error"]
    print(ui.dim(f"── {name} console · Enter wakes it · Ctrl-] leaves ──"), flush=True)
    fd = sys.stdin.fileno()
    saved = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        while True:
            r, _, _ = select.select([fd, sock], [], [])
            if sock in r:
                data = sock.recv(4096)
                if not data:
                    break
                os.write(sys.stdout.fileno(), data.replace(b"\n", b"\r\n").replace(b"\r\r\n", b"\r\n"))
            if fd in r:
                data = os.read(fd, 1024)
                if b"\x1d" in data:
                    break
                sock.sendall(data)
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, saved)
        sock.close()
    print(ui.dim(f"\n── left {name} ──"))
    return None
