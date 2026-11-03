#!/bin/sh
# Runs a node image once and types console commands into it; the node keeps
# its configuration in the image's flash.
#
#   ./configure-node.sh button.bin "device-type 4" "hwrev 1" "device-id 77"
#   ./configure-node.sh relay.bin "device-type 5" "hwrev 2" "device-id 128" \
#     "relais-mode relais" "extension-mode sensors"
#
# BOOT_WAIT (s, default 10) is how long the boot takes before the console
# listens.
SIM=$(cd "$(dirname "$0")" && pwd)
QEMU=${QEMU:-qemu-system-xtensa}
IMAGE=${1:?usage: $0 <image> <command>...}
shift
WAIT=${BOOT_WAIT:-10}
[ -f "$SIM/efuse.bin" ] || python3 "$SIM/make-efuse.py" "$SIM/efuse.bin" || exit 1
{
  sleep "$WAIT"
  printf '\r'
  for c in "$@"; do
    sleep 2
    printf '%s\r' "$c"
  done
  sleep 2
  printf 'show\r'
  sleep 3
} | timeout $((WAIT + 2 * $# + 8)) "$QEMU" -display none -machine esp32 -m 4M \
  -drive file="$IMAGE",if=mtd,format=raw \
  -drive file="$SIM/efuse.bin",if=none,format=raw,id=efuse \
  -global driver=nvram.esp32.efuse,property=drive,value=efuse \
  -serial stdio -monitor none
[ $? -eq 124 ] || exit 1
