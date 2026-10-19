#!/bin/sh
# Export the vector source directly at each macOS icon resolution.
set -eu
icon_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
command -v rsvg-convert >/dev/null || { echo "Install librsvg to regenerate the app icon." >&2; exit 2; }
command -v iconutil >/dev/null || { echo "macOS iconutil is required." >&2; exit 2; }
icon_work=$(mktemp -d "${TMPDIR:-/tmp}/rentemester-icon.XXXXXX")
trap 'rm -rf "$icon_work"' EXIT HUP INT TERM
icon_set="$icon_work/Rentemester.iconset"
mkdir "$icon_set"
for icon_size in 16 32 128 256 512; do
  rsvg-convert -w "$icon_size" -h "$icon_size" "$icon_root/assets/brand/rentemester-app-icon.svg" -o "$icon_set/icon_${icon_size}x${icon_size}.png"
  icon_retina=$((icon_size * 2))
  rsvg-convert -w "$icon_retina" -h "$icon_retina" "$icon_root/assets/brand/rentemester-app-icon.svg" -o "$icon_set/icon_${icon_size}x${icon_size}@2x.png"
done
cp "$icon_set/icon_512x512@2x.png" "$icon_root/assets/brand/rentemester-app-icon.png"
iconutil -c icns "$icon_set" -o "$icon_root/assets/brand/rentemester-app-icon.icns"
