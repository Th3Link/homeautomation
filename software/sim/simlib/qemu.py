"""The QEMU command line for one chip."""

import os
import shutil
import struct

from .topology import SIM_DIR

# The esp32-sim models, by a string each leaves in the binary.
MODELS = {b"esp32.emac": "esp32.emac", b"i2c-remote": "i2c-remote", b"esp32.gpio": "esp32.gpio",
          b"ow <pin> add|set|unplug|remove": "1-Wire"}


class QemuError(Exception):
    pass


def find():
    """qemu-system-xtensa from $QEMU or PATH, checked for the esp32-sim
    models (Espressif's own QEMU lacks them and the firmware hangs)."""
    path = os.environ.get("QEMU") or shutil.which("qemu-system-xtensa")
    if not path or not os.path.isfile(path):
        raise QemuError(
            "qemu-system-xtensa not found: build branch esp32-sim of "
            "https://github.com/Th3Link/qemu and put it on PATH or in $QEMU "
            f"(see {SIM_DIR / 'README.md'})")
    with open(path, "rb") as f:
        binary = f.read()
    missing = [name for marker, name in MODELS.items() if marker not in binary]
    if missing:
        raise QemuError(f"{path} has no {', '.join(missing)}: it is not the esp32-sim build")
    return path


def efuse_image(chip):
    """The eFuse blocks as QEMU's esp32.efuse stores them (31 words): chip
    revision 3.0, which the bootloader requires, and the chip's own MAC
    address (BLK0 words 1 and 2, read by esp-hal's base_mac_address; the
    CRC isn't checked)."""
    words = [0] * 31
    mac = chip.mac
    words[1] = int.from_bytes(mac[2:6], "big")
    words[2] = int.from_bytes(mac[0:2], "big")
    words[3] |= 1 << 15  # CHIP_VER_REV1
    words[5] |= 1 << 20  # CHIP_VER_REV2 (with APB_CTRL_DATE bit 31: revision 3.0)
    return struct.pack("<31I", *words)


def argv(qemu, chip, flash, efuse, *, run_dir=None, can=True, net="none", onewire=()):
    """`net`: "none", "user" (Ethernet, no forwarding) or "forward" (the web
    UI on the host ports). `run_dir` adds the GPIO and I2C sockets.
    `onewire`: (pin, ROM hex, millidegrees) of the DS18B20s there from
    power-on."""
    args = [
        qemu, "-display", "none", "-machine", "esp32", "-m", "4M",
        "-drive", f"file={flash},if=mtd,format=raw",
        "-drive", f"file={efuse},if=none,format=raw,id=efuse",
        "-global", "driver=nvram.esp32.efuse,property=drive,value=efuse",
        "-serial", "stdio", "-monitor", "none",
    ]
    # A node with DS18B20s counts time in instructions (16 ns each): its
    # 1-Wire slots are a few microseconds, and on the host clock they
    # stretch whenever QEMU or the host is busy. While it idles, its time
    # follows the host's; it runs a little slow (a 3 s hold looks like
    # 2.6 s). Everything else runs on the host clock.
    icount = os.environ.get("SIM_ICOUNT", "shift=4,align=on,sleep=on" if onewire else "")
    if icount and icount != "off":
        args += ["-icount", icount]
    if can:
        args += [
            "-object", "can-bus,id=canbus0",
            "-object", f"can-host-socketcan,id=canhost0,if={chip.bus},canbus=canbus0",
            "-global", "driver=esp32.twai,property=canbus,value=canbus0",
        ]
    if run_dir is not None:
        args += [
            "-chardev", f"socket,id=gpio0,path={run_dir / (chip.name + '.gpio')},server=on,wait=off",
            "-global", "driver=esp32.gpio,property=chardev,value=gpio0",
        ]
        if onewire:
            # A comma inside an option value is written twice.
            value = ",,".join(f"{pin}:{rom}:{mc}" for pin, rom, mc in onewire)
            args += ["-global", f"driver=esp32.gpio,property=onewire,value={value}"]
        buses = i2c_buses(chip)
        for n in buses:
            args += ["-chardev", f"socket,id=i2c{n},path={run_dir / f'{chip.name}.i2c{n}'},reconnect-ms=200"]
        if buses:
            args += ["-global", "driver=esp32.i2c,property=remote,value=i2c"]
    if chip.is_gateway and net != "none":
        nic = "user,model=esp32.emac"
        if net == "forward":
            nic += f",hostfwd=tcp:127.0.0.1:{chip.https}-:443,hostfwd=tcp:127.0.0.1:{chip.http}-:80"
        args += ["-nic", nic]
    return args


def i2c_buses(chip):
    """Which I2C controllers have simulated devices: I2C0 the relay
    expanders, I2C1 the extension (sensors, PWM or relay boards), and
    either one your own devices."""
    if chip.is_gateway:
        return []
    used = {bus for bus, *_ in chip.i2c_devices()}
    used |= {min(bank, 1) for bank in chip.relay_banks}
    return sorted(used)
