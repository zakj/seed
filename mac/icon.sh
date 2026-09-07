#!/usr/bin/env bash
# Renders icon.svg into Seed.icns. Needs rsvg-convert (brew install librsvg).
# The result is checked in, so build.sh never needs either.
set -euo pipefail
cd "$(dirname "$0")"

set="$(mktemp -d)/Seed.iconset"
mkdir -p "$set"

for size in 16 32 128 256 512; do
    rsvg-convert -w "$size" -h "$size" icon.svg -o "$set/icon_${size}x${size}.png"
    rsvg-convert -w $((size * 2)) -h $((size * 2)) icon.svg -o "$set/icon_${size}x${size}@2x.png"
done

iconutil -c icns "$set" -o Seed.icns
echo "built $PWD/Seed.icns"
