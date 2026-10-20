#!/usr/bin/env python3
"""Cargo compiler wrapper and preflight: one cache per repository, >=30 GiB free."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

GIB = 1024 ** 3
MIN_FREE = 30 * GIB
CRITICAL_FREE = 15 * GIB
ROOT = Path(__file__).resolve().parents[2]


def shared_cache(root=ROOT):
    # Container runners explicitly select their shared mounted build volume.
    configured = os.environ.get("RENTEMESTER_CARGO_CACHE")
    if configured:
        cache = Path(configured)
        if not cache.is_absolute():
            raise ValueError("RENTEMESTER_CARGO_CACHE must be absolute")
        return cache.resolve()
    common = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "--path-format=absolute", "--git-common-dir"],
        text=True,
    ).strip()
    return (Path(common).parent / "target").resolve()


def free_bytes(cache):
    existing = cache
    while not existing.exists():
        existing = existing.parent
    return shutil.disk_usage(existing).free


def check_space(cache):
    available = free_bytes(cache)
    if available < MIN_FREE:
        severity = "CRITICAL" if available < CRITICAL_FREE else "STOP"
        raise ValueError(
            f"{severity}: {available / GIB:.1f} GiB free; at least 30 GiB required. "
            "No compilation started. Remove rebuildable caches first; "
            "scripts/rust/cargo-guard.py cargo clean cleans only the selected shared cache."
        )
    return available


def same_path(value, expected):
    return Path(value).resolve() == expected


def prepare_cache(root, cache, compiler_args=()):
    for key in ("CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR", "CARGO_BUILD_BUILD_DIR"):
        if os.environ.get(key) and not same_path(os.environ[key], cache):
            raise ValueError(f"{key} must use the shared cache {cache}")
    for index, arg in enumerate(compiler_args):
        output = None
        if arg == "--out-dir" and index + 1 < len(compiler_args):
            output = compiler_args[index + 1]
        elif arg.startswith("--out-dir="):
            output = arg.split("=", 1)[1]
        if output and not Path(output).resolve().is_relative_to(cache):
            raise ValueError(f"Refusing separate compiler output {output}; shared cache is {cache}")
    cache.mkdir(parents=True, exist_ok=True)
    local = root / "target"
    if os.environ.get("CARGO_TARGET_DIR") or os.environ.get("CARGO_BUILD_TARGET_DIR"):
        return
    if local.resolve() == cache:
        return
    if local.exists() or local.is_symlink():
        raise ValueError(f"Separate worktree cache exists at {local}; preserve work and clean it first")
    try:
        local.symlink_to(cache, target_is_directory=True)
    except FileExistsError:
        if local.resolve() != cache:
            raise ValueError(f"Concurrent cache setup changed {local}")


def main(args):
    cache = shared_cache()
    if args == ["status"]:
        available = free_bytes(cache)
        print(json.dumps({"cache": str(cache), "freeGiB": round(available / GIB, 2),
                          "minimumFreeGiB": 30, "criticalFreeGiB": 15,
                          "buildAllowed": available >= MIN_FREE}))
        return 0 if available >= MIN_FREE else 1
    if args and args[0] == "cargo":
        cargo_args = args[1:]
        for i, arg in enumerate(cargo_args):
            value = cargo_args[i + 1] if arg == "--target-dir" and i + 1 < len(cargo_args) else None
            if arg.startswith("--target-dir="):
                value = arg.split("=", 1)[1]
            if value and not same_path(value, cache):
                raise ValueError("--target-dir must use the shared cache")
        if "clean" not in cargo_args:
            check_space(cache)
        prepare_cache(ROOT, cache)
        env = {**os.environ, "CARGO_TARGET_DIR": str(cache), "CARGO_INCREMENTAL": "0"}
        cargo = Path.home() / ".cargo/bin/cargo"
        executable = str(cargo) if cargo.is_file() else shutil.which("cargo")
        if not executable:
            raise ValueError("cargo executable unavailable")
        return subprocess.call([executable, *cargo_args], env=env)
    if not args:
        raise ValueError("Use status, cargo <arguments>, or invoke through Cargo's rustc-wrapper")
    # Cargo invokes this even for its rustc capability probe, before compilation.
    check_space(cache)
    prepare_cache(ROOT, cache, args[1:])
    os.execvp(args[0], args)


if __name__ == "__main__":
    os.umask(0o077)
    try:
        sys.exit(main(sys.argv[1:]))
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        print(f"Rentemester build guard: {error}", file=sys.stderr)
        sys.exit(2)
