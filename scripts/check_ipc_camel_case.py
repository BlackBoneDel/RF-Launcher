#!/usr/bin/env python3
"""Guardia estática del cable IPC Rust -> TypeScript (issue #6).

No intenta compilar Rust: comprueba que las structs que cruzan la frontera
tienen `rename_all = "camelCase"`, que todo campo snake_case conserva su
`alias` legacy, que no queda ningún `rename` snake_case y que los tipos TS
declaran las claves camelCase resultantes. Los nombres de comandos
(`get_theme`, `list_instances`, ...) NO se revisan aquí: Tauri sigue
usándolos en snake_case dentro de `invoke()`.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STRUCTS = {
    "Instance": "src-tauri/src/instance.rs",
    "InstanceSummary": "src-tauri/src/instance.rs",
    "RamCheck": "src-tauri/src/commands.rs",
    "LaunchResult": "src-tauri/src/commands.rs",
    "KillResult": "src-tauri/src/launcher.rs",
}
TS_TYPES = ROOT / "frontend" / "src" / "types.ts"


def camel_case(name: str) -> str:
    first, *rest = name.split("_")
    return first + "".join(part[:1].upper() + part[1:] for part in rest)


def struct_block(source: str, name: str) -> str:
    pattern = (
        r"^#\[derive\([^\]]*\)\]\s*"
        r"(?:^#\[serde\([^\]]*\)\]\s*)*"
        r"^pub struct " + re.escape(name) + r"\s*\{"
    )
    match = re.search(pattern, source, re.MULTILINE)
    if not match:
        raise AssertionError(f"No se encontró la definición de {name}")
    start = source.index("{", match.start())
    depth = 0
    for i in range(start, len(source)):
        if source[i] == "{":
            depth += 1
        elif source[i] == "}":
            depth -= 1
            if depth == 0:
                return source[match.start() : i + 1]
    raise AssertionError(f"Bloque sin cerrar para {name}")


def main() -> int:
    ts = TS_TYPES.read_text(encoding="utf-8")
    errors: list[str] = []
    wire_keys: set[str] = set()
    aliases = 0

    for name, rel_path in STRUCTS.items():
        source = (ROOT / rel_path).read_text(encoding="utf-8")
        block = struct_block(source, name)
        if '#[serde(rename_all = "camelCase")]' not in block:
            errors.append(f"{name}: falta #[serde(rename_all = \"camelCase\")]")

        bad_renames = re.findall(
            r'#\[serde\(rename\s*=\s*"([^"]*_[^"]*)"\)\]', block
        )
        if bad_renames:
            errors.append(f"{name}: rename snake_case {bad_renames}")

        fields = re.findall(r"^\s*pub\s+([a-z][a-z0-9_]*)\s*:", block, re.MULTILINE)
        if not fields:
            errors.append(f"{name}: no se detectaron campos")
        for field in fields:
            key = camel_case(field)
            wire_keys.add(key)
            if "_" in key:
                errors.append(f"{name}.{field}: clave camelCase inválida {key}")
            if "_" in field:
                aliases += 1
                if f'#[serde(alias = "{field}")]' not in block:
                    errors.append(f"{name}.{field}: falta alias legacy")
            if not re.search(r"^\s*" + re.escape(key) + r"\??\s*:", ts, re.MULTILINE):
                errors.append(f"types.ts: falta la propiedad {key} ({name})")

    ts_snake = re.findall(
        r"^\s*([a-z][A-Za-z0-9]*_[A-Za-z0-9_]+)\??\s*:", ts, re.MULTILINE
    )
    if ts_snake:
        errors.append(f"types.ts: propiedades snake_case {ts_snake}")

    rust_sources = "\n".join(
        p.read_text(encoding="utf-8")
        for p in (ROOT / "src-tauri" / "src").glob("*.rs")
    )
    serde_snake = re.findall(
        r'#\[serde\((?:rename|rename_all)\s*=\s*"([^"]*_[^"]*)"\)\]',
        rust_sources,
    )
    if serde_snake:
        errors.append(f"serde: atributos con nombre snake_case {serde_snake}")

    snake_wire_keys = sorted(k for k in wire_keys if "_" in k)
    print(f"IPC_STRUCTS_CHECKED={len(STRUCTS)}")
    print("IPC_STRUCTS=" + ",".join(STRUCTS))
    print(f"IPC_WIRE_KEYS_CHECKED={len(wire_keys)}")
    print("IPC_WIRE_KEYS=" + ",".join(sorted(wire_keys)))
    print(f"IPC_LEGACY_ALIASES_CHECKED={aliases}")
    print(f"IPC_WIRE_SNAKE_CASE_KEYS={len(snake_wire_keys)}")
    print(f"TS_SNAKE_CASE_PROPERTIES={len(ts_snake)}")
    print(f"SERDE_SNAKE_CASE_RENAMES={len(serde_snake)}")
    if errors:
        print("IPC_WIRE_STATUS=FAILED")
        for error in errors:
            print("ERROR:", error)
        return 1
    print("IPC_WIRE_STATUS=OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
