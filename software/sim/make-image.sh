#!/bin/sh
# Merges a release build into a flash image (bootloader, partition table,
# app) the way espflash would write it to a real chip.
#
#   ./make-image.sh node    [elf] [image]   # 4 MB,  default node.bin
#   ./make-image.sh gateway [elf] [image]   # 16 MB, default gateway.bin
#
# Build first: `cargo build --release` in cancomponents-hardware or
# gateway-hardware. The image is a fresh chip: a node then needs its
# device type and id (configure-node.sh), a gateway starts with defaults.
set -e
SIM=$(cd "$(dirname "$0")" && pwd)
TARGET=${CARGO_TARGET_DIR:-$SIM/../../target}/xtensa-esp32-none-elf/release
case "$1" in
  node) CRATE=cancomponents-hardware ELF=cancomponents SIZE=4mb ;;
  gateway) CRATE=gateway-hardware ELF=gateway-hardware SIZE=16mb ;;
  *) echo "usage: $0 node|gateway [elf] [image]" >&2; exit 2 ;;
esac
ELF=${2:-$TARGET/$ELF}
IMAGE=${3:-$SIM/$1.bin}
espflash save-image --chip esp32 --merge --flash-size "$SIZE" \
  --partition-table "$SIM/../$CRATE/partitions.csv" "$ELF" "$IMAGE"
