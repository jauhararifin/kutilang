#!/bin/sh
# Wraps the macOS binary in Kutilang.app and zips it.
#
#   scripts/package-macos.sh <binary> <output.zip>
#
# The app is signed ad hoc, not notarized: macOS asks for confirmation the
# first time it is opened (right-click > Open).
set -eu

binary=$1
output=$2
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
app="$work/Kutilang.app"
mkdir -p "$app/Contents/MacOS"
cp "$binary" "$app/Contents/MacOS/kutilang"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>Kutilang</string>
  <key>CFBundleDisplayName</key>
  <string>Kutilang</string>
  <key>CFBundleIdentifier</key>
  <string>com.github.jauhararifin.kutilang</string>
  <key>CFBundleExecutable</key>
  <string>kutilang</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$version</string>
  <key>CFBundleVersion</key>
  <string>$version</string>
  <key>LSMinimumSystemVersion</key>
  <string>10.15.7</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.medical</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

codesign --force --sign - "$app"
rm -f "$output"
ditto -c -k --keepParent "$app" "$output"
echo "wrote $output"
