#!/usr/bin/env python3
"""Controles negativos: mutar código productivo, exigir fallo y restaurar siempre."""
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1] / 'src-tauri'
cases = [
    ('JAVA_SHA256', root / 'src/java.rs', 'if actual.eq_ignore_ascii_case(expected) {',
     'if true { // MUTATION: aceptar checksum incorrecto',
     'truncated_download_fails_checksum', 'una descarga truncada NUNCA debe pasar'),
    ('LAUNCH_ASSET_SHA1', root / 'src/minecraft.rs', 'if actual != expected { return Err(format!("SHA-1 incorrecto para asset {expected}: {actual}").into()); }',
     'if false { return Err(format!("SHA-1 incorrecto para asset {expected}: {actual}").into()); } // MUTATION',
     'assets_validate_cache_and_reject_corruption', 'respuesta con SHA-1 incorrecto debe fallar'),
]
for label, file, original, mutant, test, expected in cases:
    saved = file.read_bytes()
    try:
        text = saved.decode('utf-8')
        if text.count(original) != 1:
            raise RuntimeError(f'{label}: punto de mutación ambiguo')
        baseline = subprocess.run(['cargo', 'test', '--locked', test, '--', '--nocapture'], cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        print(f'MUTATION_BASELINE={label} EXIT={baseline.returncode}', flush=True)
        if baseline.returncode or '1 passed' not in baseline.stdout:
            print(baseline.stdout, flush=True)
            raise RuntimeError(f'{label}: baseline no pasó exactamente un test')
        file.write_text(text.replace(original, mutant, 1), encoding='utf-8')
        run = subprocess.run(['cargo', 'test', '--locked', test, '--', '--nocapture'], cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        print(run.stdout, flush=True)
        if run.returncode == 0 or expected not in run.stdout or 'test result: FAILED' not in run.stdout:
            raise RuntimeError(f'{label}: no se observó el fallo de aserción esperado; exit={run.returncode}')
        print(f'MUTATION_DETECTED={label} EXPECTED_ASSERTION={expected}', flush=True)
    finally:
        file.write_bytes(saved)
        print(f'MUTATION_RESTORED={label}', flush=True)
print('MUTATION_CONTROLS=PASS (los tests mutados fallaron como se esperaba)')
