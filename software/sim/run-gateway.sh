#!/bin/sh
# The gateway firmware, unchanged, in the simulation: CAN on the host's CAN
# interface, Ethernet over QEMU's user network (DHCP gives 10.0.2.15; the
# host is 10.0.2.2, e.g. for an MQTT broker).
#
#   ./run-gateway.sh [image]        # default gateway.bin
#   https://127.0.0.1:8443          # the web UI (HTTP on 8080)
#
#   CAN_IF      host CAN interface (default vcan0).
#   HTTPS_PORT  host port for the web UI (default 8443), HTTP_PORT (8080).
#   QEMU        qemu-system-xtensa with the esp32-sim models (default: PATH).
#
# Ctrl-A X quits, Ctrl-A C switches between console and QEMU monitor.
SIM=$(cd "$(dirname "$0")" && pwd)
QEMU=${QEMU:-qemu-system-xtensa}
IMAGE=${1:-$SIM/gateway.bin}
[ -f "$SIM/efuse.bin" ] || python3 "$SIM/make-efuse.py" "$SIM/efuse.bin" || exit 1
exec "$QEMU" -display none -machine esp32 -m 4M \
  -drive file="$IMAGE",if=mtd,format=raw \
  -drive file="$SIM/efuse.bin",if=none,format=raw,id=efuse \
  -global driver=nvram.esp32.efuse,property=drive,value=efuse \
  -object can-bus,id=canbus0 \
  -object can-host-socketcan,id=canhost0,if="${CAN_IF:-vcan0}",canbus=canbus0 \
  -global driver=esp32.twai,property=canbus,value=canbus0 \
  -nic user,model=esp32.emac,hostfwd=tcp:127.0.0.1:"${HTTPS_PORT:-8443}"-:443,hostfwd=tcp:127.0.0.1:"${HTTP_PORT:-8080}"-:80 \
  -serial mon:stdio
