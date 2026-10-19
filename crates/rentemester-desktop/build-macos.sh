#!/bin/sh
# Create a local native app bundle; no installer, signing or server provisioning.
set -eu
if [ "$(uname -s)" != Darwin ]; then
  echo "The current bundle helper requires macOS." >&2
  exit 2
fi
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo +stable build --locked -p rentemester-desktop -j 4
bundle="$root/target/native/Rentemester Desktop.app"
mkdir -p "$bundle/Contents/MacOS"
cp "$root/target/debug/rentemester-desktop" "$bundle/Contents/MacOS/rentemester-desktop"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dk.rentemester.desktop</string>
<key>CFBundleName</key><string>Rentemester Desktop</string>
<key>CFBundleExecutable</key><string>rentemester-desktop</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
printf '%s\n' "$bundle"
