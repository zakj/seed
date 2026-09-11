#!/usr/bin/env bash
# Builds Sources/ and the sd binary into a self-contained Seed.app. --release
# optimizes the Swift side too; sd is always release, since the app runs it on
# every reload (~10ms against ~85ms for debug).
set -euo pipefail
cd "$(dirname "$0")"

config=debug
[[ "${1:-}" == "--release" ]] && config=release

# --target-dir beats a CARGO_TARGET_DIR in the environment, which would bundle a stale sd.
cargo build --release --manifest-path ../Cargo.toml --target-dir ../target
swift build -c "$config"

app="Seed.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
bin=$(swift build -c "$config" --show-bin-path)
cp "$bin/Seed" "$app/Contents/MacOS/Seed"
cp "../target/release/sd" "$app/Contents/MacOS/sd"
cp Info.plist "$app/Contents/Info.plist"
# Both version keys are stamped from the bundled sd, so the plist cannot
# disagree with the binary. Before codesign: editing the plist afterwards breaks
# the signature.
version=$("$app/Contents/MacOS/sd" --version | awk '{print $2}')
plutil -replace CFBundleShortVersionString -string "$version" "$app/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$version" "$app/Contents/Info.plist"
cp Seed.icns "$app/Contents/Resources/Seed.icns"
# The standard About panel renders Resources/Credits under the version.
cp Credits.html "$app/Contents/Resources/Credits.html"
# Dependency resources are emitted as bundles beside the binary and looked up
# relative to the main bundle; without them Textual's highlighter silently
# stops. Globbed so a renamed bundle is not lost. SwiftUIMath's is 7MB of dead
# weight, dropped by name so a rename ships weight rather than breaking the
# build. Goes away with https://github.com/gonzalezreal/textual/pull/82.
cp -R "$bin"/*.bundle "$app/Contents/Resources/"
rm -rf "$app/Contents/Resources/swiftui-math_SwiftUIMath.bundle"
codesign --force --sign - "$app" >/dev/null
echo "built $PWD/$app"
