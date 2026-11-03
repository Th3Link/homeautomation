"""Flash images: one per chip, built from the release firmware, configured
through the chip's own console, and kept across runs like a real flash.
A new firmware replaces the apps and keeps the data partitions (the node's
and gateway's configuration, the automation store)."""

import csv
import hashlib
import json
import os
import shutil
import subprocess
import threading
import time
from pathlib import Path

from . import qemu
from .topology import REPO, SIM_DIR

FIRMWARE = {
    "node": {"crate": "cancomponents-hardware", "elf": "cancomponents", "size": "4mb"},
    "gateway": {"crate": "gateway-hardware", "elf": "gateway-hardware", "size": "16mb"},
}


class ImageError(Exception):
    pass


def kind_of(chip):
    return "gateway" if chip.is_gateway else "node"


def default_elf(kind):
    target = Path(os.environ.get("CARGO_TARGET_DIR", REPO / "target"))
    return target / "xtensa-esp32-none-elf" / "release" / FIRMWARE[kind]["elf"]


def partition_table(kind):
    return SIM_DIR.parent / FIRMWARE[kind]["crate"] / "partitions.csv"


def kept_partitions(kind):
    """(offset, size) of the data partitions a firmware update keeps: all
    but otadata (cleared, so the new app in the first slot boots) and
    phy_init."""
    kept = []
    with open(partition_table(kind)) as f:
        rows = [r for r in csv.reader(f) if r and not r[0].lstrip().startswith("#")]
    for name, ptype, subtype, offset, size, *_ in (map(str.strip, r) for r in rows):
        if ptype == "data" and subtype not in ("ota", "phy"):
            kept.append((int(offset, 0), int(size, 0)))
    return kept


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


class Images:
    def __init__(self, topo, log=print):
        self.topo = topo
        self.dir = topo.state_dir
        self.log = log
        self._lock = threading.Lock()

    def chip_dir(self, chip):
        return self.dir / "chips" / chip.name

    def flash(self, chip):
        return self.chip_dir(chip) / "flash.bin"

    def efuse(self, chip):
        return self.chip_dir(chip) / "efuse.bin"

    def logfile(self, chip):
        return self.chip_dir(chip) / "serial.log"

    def _meta(self, chip):
        try:
            return json.loads((self.chip_dir(chip) / "meta.json").read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            return {}

    def _save_meta(self, chip, meta):
        (self.chip_dir(chip) / "meta.json").write_text(json.dumps(meta, indent=1))

    # Base images

    def base(self, kind, elf=None):
        """The merged image of the current release build, rebuilt when the
        ELF changed. Returns (path, firmware hash)."""
        elf = Path(elf) if elf else self.topo.firmware.get(kind) or default_elf(kind)
        base_dir = self.dir / "base"
        base_dir.mkdir(parents=True, exist_ok=True)
        image = base_dir / f"{kind}.bin"
        stamp = base_dir / f"{kind}.json"
        try:
            known = json.loads(stamp.read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            known = {}
        if not elf.is_file():
            if image.is_file() and known.get("image"):
                self.log(f"{kind}: no firmware at {elf}, keeping the image from {known.get('path')}")
                return image, known["image"]
            raise ImageError(
                f"no {kind} firmware at {elf}: run `cargo build --release` in "
                f"software/{FIRMWARE[kind]['crate']} (or set CARGO_TARGET_DIR)")
        elf_hash = sha256(elf)
        if known.get("elf") != elf_hash or not image.is_file():
            espflash = shutil.which("espflash")
            if not espflash:
                raise ImageError("espflash not found (cargo install espflash)")
            self.log(f"{kind}: building the flash image from {elf}")
            r = subprocess.run(
                [espflash, "save-image", "--chip", "esp32", "--merge",
                 "--flash-size", FIRMWARE[kind]["size"],
                 "--partition-table", str(partition_table(kind)), str(elf), str(image)],
                capture_output=True, text=True)
            if r.returncode:
                raise ImageError(f"espflash failed:\n{r.stdout}{r.stderr}")
            known = {"elf": elf_hash, "image": sha256(image), "path": str(elf),
                     "built": time.strftime("%Y-%m-%d %H:%M:%S")}
            stamp.write_text(json.dumps(known, indent=1))
        return image, known["image"]

    # Chip images

    def prepare(self, chip, base, firmware, qemu_path, reset=False):
        """Creates, updates and configures one chip's image. Returns what
        was done, or None."""
        cdir = self.chip_dir(chip)
        flash = self.flash(chip)
        if reset and cdir.exists():
            shutil.rmtree(cdir)
        cdir.mkdir(parents=True, exist_ok=True)
        self.efuse(chip).write_bytes(qemu.efuse_image(chip))
        meta = self._meta(chip)
        done = []
        if not flash.is_file():
            shutil.copyfile(base, flash)
            meta = {"firmware": firmware}
            done.append("new image")
        elif meta.get("firmware") != firmware:
            self._update(chip, base, flash)
            meta["firmware"] = firmware
            done.append("firmware updated")
        commands = chip.setup_commands()
        if commands and meta.get("setup") != commands:
            self._configure(chip, qemu_path, commands)
            meta["setup"] = commands
            done.append("configured")
        self._save_meta(chip, meta)
        return ", ".join(done) or None

    def _update(self, chip, base, flash):
        new = bytearray(Path(base).read_bytes())
        old = Path(flash).read_bytes()
        for offset, size in kept_partitions(kind_of(chip)):
            new[offset:offset + size] = old[offset:offset + size]
        tmp = flash.with_suffix(".tmp")
        tmp.write_bytes(new)
        tmp.replace(flash)

    def _configure(self, chip, qemu_path, commands):
        """Boots the image alone (no CAN) and types the commands into its
        console, the way one commissions a real chip."""
        args = qemu.argv(qemu_path, chip, self.flash(chip), self.efuse(chip),
                         can=False, net="user")
        transcript = self.chip_dir(chip) / "setup.log"
        console = Console(args, transcript)
        try:
            console.wait_prompt(timeout=60)
            for command in commands:
                answer = console.command(command)
                if "error" in answer.lower() or "unknown" in answer.lower():
                    raise ImageError(f"{chip.name}: `{command}`: {answer.strip()}")
            console.command("exit")
            time.sleep(0.5)
        finally:
            console.close()


class Console:
    """A chip's serial console in a QEMU of its own."""

    PROMPT = "❯"

    def __init__(self, args, transcript):
        self.out = open(transcript, "wb")
        self.proc = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, start_new_session=True)
        self.buf = b""
        self.cond = threading.Condition()
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        while True:
            chunk = os.read(self.proc.stdout.fileno(), 4096)
            if not chunk:
                break
            self.out.write(chunk)
            with self.cond:
                self.buf += chunk
                self.cond.notify_all()
        with self.cond:
            self.cond.notify_all()

    def _text(self, start=0):
        return self.buf[start:].decode(errors="replace")

    def _wait(self, predicate, timeout):
        end = time.monotonic() + timeout
        with self.cond:
            while not predicate():
                left = end - time.monotonic()
                if left <= 0 or self.proc.poll() is not None:
                    return False
                self.cond.wait(left)
        return True

    def send(self, text):
        self.proc.stdin.write(text.encode())
        self.proc.stdin.flush()

    def wait_prompt(self, timeout):
        """The console stays passive until Enter, so Enter is pressed until
        the prompt shows."""
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            if self.proc.poll() is not None:
                raise ImageError(f"QEMU stopped: {self._text()[-500:]}")
            self.send("\r")
            if self._wait(lambda: self.PROMPT in self._text(), 1.0):
                return
        raise ImageError("the console didn't answer")

    def command(self, command, timeout=10, quiet=0.5):
        """Types `command` and returns what came back once the console has
        been quiet for `quiet` seconds."""
        start = len(self.buf)
        self.send(command + "\r")
        end = time.monotonic() + timeout
        seen = start
        while time.monotonic() < end and self.proc.poll() is None:
            self._wait(lambda: len(self.buf) > seen, quiet)
            if len(self.buf) == seen and seen > start:
                break
            seen = len(self.buf)
        return self._text(start).replace(command, "", 1)

    def close(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        self.out.close()
