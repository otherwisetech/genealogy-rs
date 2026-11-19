#!/usr/bin/env bash
# Wraps the `genealogy-rs` binary in a macOS .app so Finder and the Dock show the app icon.
# Usage: script/bundle-macos.sh [debug|release]   (default: release; builds first)
#        UNIVERSAL=1 script/bundle-macos.sh       (release for Apple silicon + Intel, joined with lipo)
# Honours CARGO_TARGET_DIR (binary read from, and the .app written to, that directory).
set -euo pipefail
profile="${1:-release}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
target="${CARGO_TARGET_DIR:-target}"
if [ "${UNIVERSAL:-0}" = 1 ]; then
  profile=release
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target add "$t" >/dev/null
    cargo build -p gen-app --release --target "$t"
  done
  mkdir -p "$target/release"
  lipo -create -output "$target/release/genealogy-rs" "$target/aarch64-apple-darwin/release/genealogy-rs" "$target/x86_64-apple-darwin/release/genealogy-rs"
elif [ "$profile" = release ]; then cargo build -p gen-app --release; else cargo build -p gen-app; fi
version="$(cargo pkgid -p gen-app | sed 's/.*[#@]//')"
app="$target/$profile/genealogy-rs.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$target/$profile/genealogy-rs" "$app/Contents/MacOS/genealogy-rs"
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
  <key>UTImportedTypeDeclarations</key>
  <array>
    <dict>
      <key>UTTypeIdentifier</key><string>org.gedcom.ged</string>
      <key>UTTypeDescription</key><string>GEDCOM family tree</string>
      <key>UTTypeIconFile</key><string>AppIcon</string>
      <key>UTTypeConformsTo</key>
      <array><string>public.data</string><string>public.text</string><string>public.plain-text</string></array>
      <key>UTTypeTagSpecification</key>
      <dict>
        <key>public.filename-extension</key><array><string>ged</string><string>GED</string></array>
        <key>public.mime-type</key><array><string>application/x-gedcom</string><string>text/x-gedcom</string></array>
      </dict>
    </dict>
  </array>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>GEDCOM family tree</string>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>CFBundleTypeIconFile</key><string>AppIcon</string>
      <key>LSHandlerRank</key><string>Owner</string>
      <key>LSItemContentTypes</key><array><string>org.gedcom.ged</string></array>
    </dict>
  </array>
</dict>
</plist>
PLIST
plutil -lint -s "$app/Contents/Info.plist"
# Make LaunchServices pick up the (possibly changed) icon.
touch "$app"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$app" || true
echo "$app"
