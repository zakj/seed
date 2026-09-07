#!/usr/bin/env bash
# Builds Sources/ and the sd binary into a self-contained Seed.app.
# Pass --release to optimize the Swift side too.
#
# sd is always built in release: the app runs it on every reload and every edit,
# where the debug build costs ~85ms against ~10ms.
set -euo pipefail
cd "$(dirname "$0")"

config=debug
[[ "${1:-}" == "--release" ]] && config=release

# --target-dir pins where the binary lands: it beats a CARGO_TARGET_DIR in the
# environment, which would otherwise build one sd and bundle a stale other.
cargo build --release --manifest-path ../Cargo.toml --target-dir ../target
swift build -c "$config"

app="Seed.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$(swift build -c "$config" --show-bin-path)/Seed" "$app/Contents/MacOS/Seed"
cp "../target/release/sd" "$app/Contents/MacOS/sd"
cp Info.plist "$app/Contents/Info.plist"
cp Seed.icns "$app/Contents/Resources/Seed.icns"
codesign --force --sign - "$app" >/dev/null
echo "built $PWD/$app"
