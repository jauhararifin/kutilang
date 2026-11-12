#!/bin/sh
# Renders the application icons from assets/app-icon/*.svg (on macOS):
#
#   assets/app-icon/kutilang.ico    the Windows executable's icon
#   assets/app-icon/Kutilang.icns   the macOS app's icon
#
# Run it after changing the SVGs; the results are committed.
set -eu
cd "$(dirname "$0")/.."
icons=assets/app-icon
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# SVG -> 1024 px PNG, drawn by AppKit.
render() {
    swift - "$1" "$2" <<'SWIFT'
import AppKit
let args = CommandLine.arguments
let image = NSImage(contentsOfFile: args[1])!
let size = 1024
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
image.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: args[2]))
SWIFT
}
render "$icons/icon.svg" "$work/windows.png"
render "$icons/icon-macos.svg" "$work/macos.png"

# macOS: an iconset of every size, packed by iconutil.
iconset="$work/Kutilang.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
    sips -z $size $size "$work/macos.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double "$work/macos.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$icons/Kutilang.icns"

# Windows: PNG images of each size in one .ico file.
for size in 16 24 32 48 64 128 256; do
    sips -z $size $size "$work/windows.png" --out "$work/ico-$size.png" >/dev/null
done
python3 - "$work" "$icons/kutilang.ico" <<'PY'
import struct, sys
work, out = sys.argv[1], sys.argv[2]
sizes = [16, 24, 32, 48, 64, 128, 256]
images = [open(f"{work}/ico-{s}.png", "rb").read() for s in sizes]
header = struct.pack("<HHH", 0, 1, len(images))
offset = 6 + 16 * len(images)
entries = b""
for size, data in zip(sizes, images):
    side = 0 if size == 256 else size  # 0 means 256
    entries += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(data), offset)
    offset += len(data)
open(out, "wb").write(header + entries + b"".join(images))
PY
echo "wrote $icons/Kutilang.icns and $icons/kutilang.ico"
