#!/usr/bin/env bash
set -euo pipefail

# Run from the repository root after assembling any rustdoc and metrics artifacts.
output=${1:?Usage: bash docs/site/build.sh OUTPUT_DIR}
mkdir -p "$output/assets"
cp -R docs/site/assets/. "$output/assets/"

options=()
if [[ -f "$output/firmware-memory/index.html" ]]; then
    options+=(--metadata=firmware-memory:true)
fi

pandoc --from=gfm --to=html5 --standalone --toc --toc-depth=2 \
    --template=docs/site/handbook.html --lua-filter=docs/site/handbook.lua \
    "${options[@]}" docs/handbook.md --output="$output/index.html"
