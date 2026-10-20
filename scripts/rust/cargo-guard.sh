#!/bin/sh
# Normal hosts use the Python guard; pinned minimal build containers have no Python.
set -eu
umask 077
guard_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if command -v python3 >/dev/null 2>&1; then
    exec python3 "$guard_dir/cargo-guard.py" "$@"
fi
fail() { echo "Rentemester build guard: $*" >&2; exit 2; }
[ -n "${RENTEMESTER_CARGO_CACHE:-}" ] || fail "Python 3 required on local hosts; container cache must be explicit"
case "$RENTEMESTER_CARGO_CACHE" in /*) ;; *) fail "Container cache must be absolute" ;; esac
cache=$(realpath -m -- "$RENTEMESTER_CARGO_CACHE")
[ "$(realpath -m -- "${CARGO_TARGET_DIR:-/missing-target}")" = "$cache" ] || fail "CARGO_TARGET_DIR must use the shared mounted cache"
for configured in "${CARGO_BUILD_TARGET_DIR:-}" "${CARGO_BUILD_BUILD_DIR:-}"; do
    [ -z "$configured" ] || [ "$(realpath -m -- "$configured")" = "$cache" ] || fail "Separate build cache refused"
done
output() {
    actual=$(realpath -m -- "$1")
    case "$actual" in "$cache"|"$cache"/*) ;; *) fail "Separate compiler output refused: $actual" ;; esac
}
previous=
for argument do
    if [ "$previous" = --out-dir ]; then output "$argument"; fi
    case "$argument" in --out-dir=*) output "${argument#--out-dir=}" ;; esac
    previous=$argument
done
mkdir -p -- "$cache"
source_root=$(CDPATH= cd -- "$guard_dir/../.." && pwd)
# Check the host-shared source mount too: the build volume may be thin-provisioned.
available=$(df -Pk "$cache" "$source_root" | awk 'NR>1 {if(min=="" || $4<min)min=$4} END {print min}')
case "$available" in ''|*[!0-9]*) fail "Available disk space could not be determined" ;; esac
if [ "$available" -lt 15728640 ]; then fail "CRITICAL: less than 15 GiB free; compiler refused"; fi
if [ "$available" -lt 31457280 ]; then fail "STOP: at least 30 GiB free required; compiler refused"; fi
[ "$#" -gt 0 ] || fail "Compiler invocation required"
exec "$@"
