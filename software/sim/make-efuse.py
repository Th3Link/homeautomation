#!/usr/bin/env python3
"""Writes the eFuse image the simulated chips boot with: blank except for
chip revision 3.0, because the bootloader refuses anything older. QEMU's
default (all zero) is revision 0.

  make-efuse.py efuse.bin
"""
import struct
import sys

words = [0] * 31  # BLK0..BLK3 as QEMU's esp32.efuse stores them
words[3] |= 1 << 15  # CHIP_VER_REV1
words[5] |= 1 << 20  # CHIP_VER_REV2 (with APB_CTRL_DATE bit 31: revision 3.0)
with open(sys.argv[1], "wb") as f:
    f.write(struct.pack("<31I", *words))
