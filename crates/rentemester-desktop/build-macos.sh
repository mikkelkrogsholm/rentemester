#!/bin/sh
# Bundle the native GUI, same-version Mac server and browser setup assets.
set -eu
if [ "$(uname -s)" != Darwin ]; then
  echo "The current bundle helper requires macOS." >&2
  exit 2
fi
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"
web_source=${RENTEMESTER_DESKTOP_WEB_DIR:-"$root/app/dist"}
if [ ! -f "$web_source/index.html" ]; then
  echo "Build the cockpit first (cd app && bun run build), or set RENTEMESTER_DESKTOP_WEB_DIR." >&2
  exit 2
fi
# Build separately: server/storage JSON features must not force another GPUI build.
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo build --locked -p rentemester-server -j 4
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo +stable build --locked -p rentemester-desktop -j 4
bundle="$root/target/native/Rentemester Desktop.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources/web"
cp "$root/target/debug/rentemester-desktop" "$bundle/Contents/MacOS/rentemester-desktop"
cp "$root/target/debug/rentemester-server" "$bundle/Contents/MacOS/rentemester-server"
cp -R "$web_source/." "$bundle/Contents/Resources/web/"
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
