#!/bin/sh
# Build-time tools only; the bundle runs its native GUI/server and static web assets.
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
# Preserve dev/debug defaults; custom Cargo target directory/profile stay consistent at assembly.
profile=${RENTEMESTER_DESKTOP_PROFILE:-dev}
target_dir=${CARGO_TARGET_DIR:-"$root/target"}
target=${CARGO_BUILD_TARGET:-}
# Build separately: server/storage JSON features must not force another GPUI build.
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo build --locked --profile "$profile" -p rentemester-server -j 4
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo +stable build --locked --profile "$profile" -p rentemester-desktop -j 4
revision=$(git rev-parse HEAD)
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p' Cargo.toml)
server_toolchain=$(rustc --version)
desktop_toolchain=$(rustc +stable --version)
architecture=$(uname -m)
if [ -n "$target" ]; then architecture=${target%%-*}; fi
set -- --root "$root" --target-dir "$target_dir" --profile "$profile" --target "$target" \
  --web "$web_source" --icon "$root/assets/brand/rentemester-app-icon.icns" \
  --output "$root/target/native/Rentemester Desktop.app" --revision "$revision" --version "$version" \
  --architecture "$architecture" --server-toolchain "$server_toolchain" --desktop-toolchain "$desktop_toolchain"
if [ -n "$(git status --porcelain -- . ':!graphify-out')" ]; then set -- "$@" --source-dirty; fi
python3 "$root/scripts/release/assemble-native-macos.py" "$@"
