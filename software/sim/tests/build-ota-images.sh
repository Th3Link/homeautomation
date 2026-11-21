#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 1 ]]; then
    printf 'Usage: %s OUTPUT_DIRECTORY\n' "$0" >&2
    exit 2
fi

mkdir -p "$1"
output=$(cd "$1" && pwd)
repo=$(cd "$(dirname "$0")/../../.." && pwd)
CARGO_TARGET_DIR=$(realpath -m "${CARGO_TARGET_DIR:-$repo/target/qemu-ota}")
export CARGO_TARGET_DIR
release="$CARGO_TARGET_DIR/xtensa-esp32-none-elf/release"

for crate in gateway-hardware cancomponents-hardware; do
    binary=$crate
    if [[ $crate == cancomponents-hardware ]]; then
        binary=cancomponents
    fi
    # Each label fits in the CAN version reply (eight bytes).
    for version in qemu-v1 qemu-v2; do
        mkdir -p "$output/$version"
        (cd "$repo/software/$crate" && VERGEN_GIT_DESCRIBE="$version" cargo build --release --locked)
        cp "$release/$binary" "$output/$version/"
        # Convert the ELF just built, since gateway build.rs runs before linking.
        espflash save-image --chip esp32 "$output/$version/$binary" "$output/$version/$binary.bin"
    done
done
