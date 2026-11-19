#!/usr/bin/env bash
# Wraps the `gen` binary in a macOS .app so Finder and the Dock show the app icon.
# Usage: script/bundle-macos.sh [debug|release]   (default: release; builds first)
set -euo pipefail
profile="${1:-release}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
if [ "$profile" = release ]; then cargo build -p gen-app --release; else cargo build -p gen-app; fi
version="$(cargo pkgid -p gen-app | sed 's/.*[#@]//')"
app="target/$profile/gen.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$profile/gen" "$app/Contents/MacOS/gen"
cp crates/gen-app/assets/logo/AppIcon.icns "$app/Contents/Resources/AppIcon.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>gen</string>
  <key>CFBundleDisplayName</key><string>gen</string>
  <key>CFBundleIdentifier</key><string>io.github.otherwisetech.gen</string>
  <key>CFBundleExecutable</key><string>gen</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  </dict></array>
</dict>
</plist>
PLIST
echo "$app"
