#!/bin/sh
# Package the approved raster master at standard macOS icon resolutions; no redesign.
set -eu
icon_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
command -v sips >/dev/null || { echo "macOS sips is required." >&2; exit 2; }
command -v iconutil >/dev/null || { echo "macOS iconutil is required." >&2; exit 2; }
icon_master="$icon_root/assets/branding/rentemester-app-icon-v1.png"
icon_output="$icon_root/assets/branding/rentemester-app-icon-v1.icns"
if [ ! -f "$icon_master" ] || [ -L "$icon_master" ] || [ -L "$icon_output" ]; then
  echo "Expected the regular approved PNG master and a non-symlink output." >&2
  exit 2
fi
icon_work=$(mktemp -d "${TMPDIR:-/tmp}/rentemester-icon.XXXXXX")
trap 'rm -rf "$icon_work"' EXIT HUP INT TERM
icon_set="$icon_work/Rentemester.iconset"
mkdir "$icon_set"
for icon_size in 16 32 128 256 512; do
  sips --resampleHeightWidth "$icon_size" "$icon_size" "$icon_master" --out "$icon_set/icon_${icon_size}x${icon_size}.png" >/dev/null
  icon_retina=$((icon_size * 2))
  sips --resampleHeightWidth "$icon_retina" "$icon_retina" "$icon_master" --out "$icon_set/icon_${icon_size}x${icon_size}@2x.png" >/dev/null
done
iconutil -c icns "$icon_set" -o "$icon_work/Rentemester.icns"
mv "$icon_work/Rentemester.icns" "$icon_output"
