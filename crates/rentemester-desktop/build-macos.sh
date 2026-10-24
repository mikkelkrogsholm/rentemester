#!/bin/sh
# Build-time tools only; the bundle runs its native GUI/server and static web assets.
set -eu
if [ "$(uname -s)" != Darwin ]; then
  echo "The current bundle helper requires macOS." >&2
  exit 2
fi
# App-launched shells may omit rustup from PATH although guarded Cargo resolves it.
if ! command -v rustc >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/rustc" ]; then
  PATH="$HOME/.cargo/bin:$PATH"
  export PATH
fi
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"
web_source=${RENTEMESTER_DESKTOP_WEB_DIR:-"$root/app/dist"}
if [ ! -f "$web_source/index.html" ]; then
  echo "Build the cockpit first (cd app && bun run build), or set RENTEMESTER_DESKTOP_WEB_DIR." >&2
  exit 2
fi
# Preserve dev/debug defaults; the guard enforces the shared cache used at assembly.
profile=${RENTEMESTER_DESKTOP_PROFILE:-dev}
# Resolve the exact desktop pin; a moving +stable can silently change the app.
desktop_channel=$(sed -n 's/^channel = "\([^"]*\)"/\1/p' crates/rentemester-desktop/rust-toolchain.toml)
if [ -z "$desktop_channel" ]; then
  echo "Missing pinned desktop Rust toolchain." >&2
  exit 2
fi
target_dir=${CARGO_TARGET_DIR:-"$root/target"}
target=${CARGO_BUILD_TARGET:-}
# Build the server and shared-web desktop host serially in the guarded cache.
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 python3 scripts/rust/cargo-guard.py cargo build --locked --profile "$profile" -p rentemester-server -j 1
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 python3 scripts/rust/cargo-guard.py cargo +"$desktop_channel" build --locked --profile "$profile" -p rentemester-desktop --bin rentemester-desktop -j 1
revision=$(git rev-parse HEAD)
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\([^"]*\)"/\1/p' Cargo.toml)
server_toolchain=$(rustc --version)
desktop_toolchain=$(rustc +"$desktop_channel" --version)
architecture=$(uname -m)
if [ -n "$target" ]; then architecture=${target%%-*}; fi
set -- --root "$root" --target-dir "$target_dir" --profile "$profile" --target "$target" \
  --web "$web_source" \
  --output "$root/target/native/Rentemester Desktop.app" --revision "$revision" --version "$version" \
  --architecture "$architecture" --server-toolchain "$server_toolchain" --desktop-toolchain "$desktop_toolchain"
if [ -n "$(git status --porcelain -- . ':!graphify-out')" ]; then set -- "$@" --source-dirty; fi
python3 "$root/scripts/release/assemble-native-macos.py" "$@"
