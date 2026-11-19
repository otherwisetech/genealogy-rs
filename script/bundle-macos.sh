#!/usr/bin/env bash
# Wraps the `genealogy-rs` binary in a macOS .app so Finder and the Dock show the app icon.
# Usage: script/bundle-macos.sh [debug|release]   (default: release; builds first)
#        UNIVERSAL=1 script/bundle-macos.sh       (release for Apple silicon + Intel, joined with lipo)
set -euo pipefail
profile="${1:-release}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
if [ "${UNIVERSAL:-0}" = 1 ]; then
  profile=release
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target add "$t" >/dev/null
    cargo build -p gen-app --release --target "$t"
  done
  mkdir -p target/release
  lipo -create -output target/release/genealogy-rs target/aarch64-apple-darwin/release/genealogy-rs target/x86_64-apple-darwin/release/genealogy-rs
elif [ "$profile" = release ]; then cargo build -p gen-app --release; else cargo build -p gen-app; fi
version="$(cargo pkgid -p gen-app | sed 's/.*[#@]//')"
app="target/$profile/genealogy-rs.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$profile/genealogy-rs" "$app/Contents/MacOS/genealogy-rs"
cp crates/gen-app/assets/logo/AppIcon.icns "$app/Contents/Resources/AppIcon.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>genealogy-rs</string>
  <key>CFBundleDisplayName</key><string>genealogy-rs</string>
  <key>CFBundleIdentifier</key><string>io.github.otherwisetech.genealogy-rs</string>
  <key>CFBundleExecutable</key><string>genealogy-rs</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
plutil -lint -s "$app/Contents/Info.plist"
# Make LaunchServices pick up the (possibly changed) icon.
touch "$app"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$app" || true
echo "$app"
