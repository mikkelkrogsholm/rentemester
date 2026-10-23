#!/usr/bin/env python3
"""Verify locked all-platform Rust licenses; metadata resolution never compiles code."""
import argparse
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source', type=Path, default=Path(__file__).resolve().parents[2])
    p.add_argument('--metadata', type=Path, help='Reuse complete cargo metadata --locked --format-version 1')
    p.add_argument('--output', type=Path)
    a = p.parse_args()
    source = a.source.resolve()
    with tempfile.TemporaryDirectory(prefix='rentemester-license-metadata-') as temp:
        metadata = a.metadata
        if metadata is None:
            cargo = shutil.which('cargo') or str(Path.home() / '.cargo/bin/cargo')
            metadata = Path(temp) / 'cargo-metadata.json'
            with metadata.open('wb') as stream:
                subprocess.run([cargo, 'metadata', '--locked', '--all-features', '--format-version', '1'],
                               cwd=source, stdout=stream, check=True, timeout=600)
        command = [sys.executable, str(source / 'scripts/release/check-rust-licenses.py'),
                   '--source', str(source), '--metadata', str(metadata.resolve()),
                   '--policy', str(source / 'scripts/release/rust-license-policy.json')]
        if a.output:
            command += ['--output', str(a.output)]
        subprocess.run(command, cwd=source, check=True, timeout=120)


if __name__ == '__main__':
    main()
