#!/bin/sh
# Normal hosts use the Python guard; pinned minimal build containers have no Python.
set -eu
umask 077
guard_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if command -v python3 >/dev/null 2>&1; then
    exec python3 "$guard_dir/cargo-guard.py" "$@"
fi
fail() { echo "Rentemester build guard: $*" >&2; exit 2; }
source_root=$(CDPATH= cd -- "$guard_dir/../.." && pwd)
selected=${CARGO_TARGET_DIR:-${RENTEMESTER_CARGO_CACHE:-}}
if [ -z "$selected" ]; then
    common=$(git -C "$source_root" rev-parse --path-format=absolute --git-common-dir)
    selected=$(dirname -- "$common")/target
fi
case "$selected" in /*) ;; *) fail "CARGO_TARGET_DIR / RENTEMESTER_CARGO_CACHE must be absolute" ;; esac
cache=$(realpath -m -- "$selected")
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
    if [ "$previous" = --target-dir ]; then
        [ "$(realpath -m -- "$argument")" = "$cache" ] || fail "--target-dir must use the selected target"
    fi
    case "$argument" in
        --out-dir=*) output "${argument#--out-dir=}" ;;
        --target-dir=*) [ "$(realpath -m -- "${argument#--target-dir=}")" = "$cache" ] || fail "--target-dir must use the selected target" ;;
    esac
    previous=$argument
done
# Resolve an existing ancestor without creating output before admission.
existing=$cache
while [ ! -e "$existing" ]; do existing=$(dirname -- "$existing"); done
available=$(df -Pk "$existing" "$source_root" | awk 'NR>1 {if(min=="" || $4<min)min=$4} END {print min}')
case "$available" in ''|*[!0-9]*) fail "Available disk space could not be determined" ;; esac
[ "$#" -gt 0 ] || fail "Compiler invocation required"
if [ "$1" = status ]; then
    allowed=false
    [ "$available" -lt 31457280 ] || allowed=true
    printf '{"cache":"%s","freeGiB":%s,"minimumFreeGiB":30,"criticalFreeGiB":15,"buildAllowed":%s}\n' "$cache" "$(awk "BEGIN {print $available / 1048576}")" "$allowed"
    [ "$allowed" = true ]
    exit
fi
clean=false
if [ "$1" = cargo ]; then
    for argument do [ "$argument" != clean ] || clean=true; done
fi
if [ "$clean" = false ]; then
    if [ "$available" -lt 15728640 ]; then fail "CRITICAL: less than 15 GiB free; compiler refused"; fi
    if [ "$available" -lt 31457280 ]; then fail "STOP: at least 30 GiB free required; compiler refused"; fi
fi
mkdir -p -- "$cache"
if [ -z "${CARGO_TARGET_DIR:-}${CARGO_BUILD_TARGET_DIR:-}" ] && [ "$(realpath -m -- "$source_root/target")" != "$cache" ]; then
    [ ! -e "$source_root/target" ] && [ ! -L "$source_root/target" ] || fail "Separate worktree cache exists; preserve work and clean it first"
    ln -s -- "$cache" "$source_root/target" || [ "$(realpath -m -- "$source_root/target")" = "$cache" ] || fail "Concurrent cache setup changed target"
fi
if [ "$1" = cargo ]; then
    shift
    export CARGO_TARGET_DIR="$cache" CARGO_INCREMENTAL=0
    exec cargo "$@"
fi
exec "$@"
