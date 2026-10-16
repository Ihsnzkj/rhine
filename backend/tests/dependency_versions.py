#!/usr/bin/env python3
"""Check the advertised MSRV against every package in the locked graph."""
import json
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[2]
metadata = json.loads(subprocess.check_output([
    'cargo', 'metadata', '--manifest-path', str(root/'backend/Cargo.toml'),
    '--locked', '--all-features', '--format-version', '1',
], text=True))
project = next(p for p in metadata['packages'] if p['name'] == 'rhine-lock')


def version(text):
    return tuple(map(int, text.split('.')[:2]))


minimum = version(project['rust_version'])
too_new = [f"{p['name']} {p['version']}: {p['rust_version']}" for p in metadata['packages']
           if p.get('rust_version') and version(p['rust_version']) > minimum]
assert not too_new, '\n'.join(too_new)
print(f"PASS dependency MSRV: {len(metadata['packages'])} packages fit Rust {project['rust_version']}")
