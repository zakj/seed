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
bin=$(swift build -c "$config" --show-bin-path)
cp "$bin/Seed" "$app/Contents/MacOS/Seed"
cp "../target/release/sd" "$app/Contents/MacOS/sd"
cp Info.plist "$app/Contents/Info.plist"
# The two version keys live here rather than in Info.plist: a number checked in
# beside a version cargo bumps is a number that goes stale, and it goes stale
# silently — the About panel reads these, and so would any updater. Read back
# off the binary just bundled, so the bundle cannot claim a version other than
# the sd inside it. Before codesign: editing the plist after signing breaks the
# signature.
version=$("$app/Contents/MacOS/sd" --version | awk '{print $2}')
plutil -replace CFBundleShortVersionString -string "$version" "$app/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$version" "$app/Contents/Info.plist"
cp Seed.icns "$app/Contents/Resources/Seed.icns"
# The standard About panel renders a Resources/Credits file under the version,
# links live. Cheaper than replacing the panel to hold one URL.
cp Credits.html "$app/Contents/Resources/Credits.html"
# A dependency's resources are emitted as a bundle beside the binary and looked
# up relative to the main bundle, so leaving them behind is silent: Textual's
# highlighter just stops highlighting inside the app while it still works from
# the build directory. Globbed rather than named, so a dependency that gains or
# renames one does not go missing the same way — then the one bundle that is 7MB
# of dead weight is dropped again. SwiftUIMath resolves its bundle at the app
# root, beside Contents rather than inside it, so the copy is never read; it
# falls back to a path compiled in from this checkout, which is why a math fence
# renders here and would trap anywhere else. Removing by name is the safe
# direction: a rename ships the weight instead of breaking the build.
cp -R "$bin"/*.bundle "$app/Contents/Resources/"
rm -rf "$app/Contents/Resources/swiftui-math_SwiftUIMath.bundle"
codesign --force --sign - "$app" >/dev/null
echo "built $PWD/$app"
