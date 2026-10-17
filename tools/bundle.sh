#!/bin/bash
# Builds Ragnarok.app, the release client as a macOS app. Like the official client it sits beside data.grf (and the
# data, System and BGM folders), and works in its own folder when started from Finder.
# Usage: tools/bundle.sh [folder to put it in, the repository by default]
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release
app="${1:-.}/Ragnarok.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp assets/icon.icns "$app/Contents/Resources/icon.icns"
cp target/release/rustnarok-client "$app/Contents/MacOS/rustnarok-client"
version=$(awk -F'"' '/^version/ { print $2; exit }' Cargo.toml)
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Ragnarok</string>
  <key>CFBundleDisplayName</key><string>Ragnarok</string>
  <key>CFBundleIdentifier</key><string>local.rustnarok-client</string>
  <key>CFBundleExecutable</key><string>rustnarok-client</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
echo "$app"
