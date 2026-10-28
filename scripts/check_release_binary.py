#!/usr/bin/env python3
"""Busca marcadores concretos de hooks test; control positivo en binario E2E."""
import hashlib
import sys
from pathlib import Path

binary = Path(sys.argv[1]).read_bytes()
mode = sys.argv[2]
markers = [b'RF_LAUNCHER_E2E_MINECRAFT_FIXTURE', b'RF_LAUNCHER_E2E_ADOPTIUM_BASE', b'rf.launcher.e2e.FakeMinecraft']
print(f'BINARY_MODE={mode} SIZE={len(binary)} SHA256={hashlib.sha256(binary).hexdigest()}')
for marker in markers:
    present = marker in binary
    print(f'BINARY_MARKER={marker.decode()} PRESENT={present}')
    if present != (mode == 'e2e'):
        raise SystemExit(f'Unexpected marker status: {marker.decode()}')
