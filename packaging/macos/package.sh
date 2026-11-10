#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
    echo "usage: $0 <version> <videleum-binary> [output-dir]" >&2
    exit 2
fi

version="$1"
binary="$2"
output_dir="${3:-dist}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if [[ ! -f "$binary" ]]; then
    echo "missing binary: $binary" >&2
    exit 1
fi

mkdir -p "$output_dir"
app="$output_dir/Videleum.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp "$binary" "$app/Contents/MacOS/videleum"
chmod +x "$app/Contents/MacOS/videleum"

sed "s/__VERSION__/$version/g" \
    "$repo_root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"

iconset="$output_dir/Videleum.iconset"
rm -rf "$iconset"
mkdir -p "$iconset"
icon_source="$repo_root/assets/branding/videleum_logo.png"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$icon_source" \
        --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$((size * 2))" "$((size * 2))" "$icon_source" \
        --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/Videleum.icns"
rm -rf "$iconset"

xattr -cr "$app"
codesign --force --deep --sign - "$app"

dmg="$output_dir/Videleum-${version}-macOS-universal.dmg"
rm -f "$dmg"
hdiutil create \
    -volname "Videleum" \
    -srcfolder "$app" \
    -ov \
    -format UDZO \
    "$dmg"

echo "Created $dmg"
