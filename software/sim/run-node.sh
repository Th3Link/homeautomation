#!/bin/sh
# One CAN node (cancomponents firmware, unchanged) in the simulation: its
# TWAI on the host's CAN interface, its GPIOs on a TCP socket and, if asked,
# its I2C buses on i2c_models.py.
#
#   ./run-node.sh <image>
#   GPIO_PORT=5556 I2C_PORTS="5600 5601" ./run-node.sh relay.bin
#
#   GPIO_PORT   TCP port for the pins (default 5555): "pin <n> <0|1>" drives
#               an input, "out <n> <0|1>" reports an output.
#   I2C_PORTS   i2c_models.py ports for I2C0 and I2C1 (the second optional);
#               without them no I2C device answers.
#   CAN_IF      host CAN interface (default vcan0).
#   QEMU        qemu-system-xtensa with the esp32-sim models (default: PATH).
#
# Ctrl-A X quits, Ctrl-A C switches between console and QEMU monitor.
SIM=$(cd "$(dirname "$0")" && pwd)
QEMU=${QEMU:-qemu-system-xtensa}
IMAGE=${1:?usage: $0 <image>}
[ -f "$SIM/efuse.bin" ] || python3 "$SIM/make-efuse.py" "$SIM/efuse.bin" || exit 1
set -- -display none -machine esp32 -m 4M \
  -drive file="$IMAGE",if=mtd,format=raw \
  -drive file="$SIM/efuse.bin",if=none,format=raw,id=efuse \
  -global driver=nvram.esp32.efuse,property=drive,value=efuse \
  -object can-bus,id=canbus0 \
  -object can-host-socketcan,id=canhost0,if="${CAN_IF:-vcan0}",canbus=canbus0 \
  -global driver=esp32.twai,property=canbus,value=canbus0 \
  -chardev socket,id=gpio0,host=127.0.0.1,port="${GPIO_PORT:-5555}",server=on,wait=off \
  -global driver=esp32.gpio,property=chardev,value=gpio0
if [ -n "$I2C_PORTS" ]; then
  n=0
  for port in $I2C_PORTS; do
    set -- "$@" -chardev socket,id=i2c$n,host=127.0.0.1,port="$port",reconnect-ms=500
    n=$((n + 1))
  done
  set -- "$@" -global driver=esp32.i2c,property=remote,value=i2c
fi
exec "$QEMU" "$@" -serial mon:stdio
