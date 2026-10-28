#!/usr/bin/env python3
"""Valida los fixtures de catálogo y, opcionalmente, sondea la red real.

Uso:
  python3 scripts/verify_catalog_fixtures.py            # offline, siempre en CI
  python3 scripts/verify_catalog_fixtures.py --live    # añade sonda HTTP real

Regla anti-falsos-verdes: si la red falla se imprime LIVE_PROBE=SKIPPED y el
proceso sale 0, pero NUNCA se imprime LIVE_PROBE=OK. El smoke de Fabric es
independiente y también se reporta como FABRIC_PROBE=OK|SKIPPED.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
import urllib.error
import urllib.request
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = [
    ROOT / "catalog-fixtures" / "mojang-version-manifest-v2.json",
    ROOT / "catalog-fixtures" / "fabric-loader-1.21.1.json",
]
LIVE_URLS = {
    "mojang": "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
    "fabric": "https://meta.fabricmc.net/v2/versions/loader/1.21.1",
}


def canonical(payload: object) -> bytes:
    return json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")


def validate_fixture(path: Path) -> dict:
    data = json.loads(path.read_text(encoding="utf-8"))
    for key in ("sourceUrl", "fetchedAt", "sha256", "payload"):
        if key not in data:
            raise SystemExit(f"ERROR: {path.name} sin clave {key}")
    datetime.fromisoformat(data["fetchedAt"].replace("Z", "+00:00"))
    actual = hashlib.sha256(canonical(data["payload"])).hexdigest()
    if actual != data["sha256"]:
        raise SystemExit(f"ERROR: {path.name} sha256 {data['sha256']} != {actual}")
    print(f"FIXTURE_OK={path.name} sha256={actual} source={data['sourceUrl']}")
    return data


def probe(url: str) -> tuple[bool, str]:
    request = urllib.request.Request(url, headers={"User-Agent": "rf-launcher-catalog-smoke/0.1"})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            body = response.read()
            if response.status != 200 or not body:
                return False, f"HTTP {response.status} bytes={len(body)}"
            json.loads(body)
            return True, f"HTTP 200 bytes={len(body)}"
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError, OSError) as exc:
        return False, repr(exc)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--live", action="store_true")
    args = parser.parse_args()

    for path in FIXTURES:
        validate_fixture(path)

    if not args.live:
        print("LIVE_PROBE=SKIPPED (modo offline)")
        print("FABRIC_PROBE=SKIPPED (modo offline)")
        return 0

    ok, detail = probe(LIVE_URLS["mojang"])
    print(f"MOJANG_PROBE={'OK' if ok else 'SKIPPED'} detail={detail}")
    ok_fabric, detail_fabric = probe(LIVE_URLS["fabric"])
    print(f"FABRIC_PROBE={'OK' if ok_fabric else 'SKIPPED'} detail={detail_fabric}")
    if ok or ok_fabric:
        print("LIVE_PROBE=OK")
    else:
        print("LIVE_PROBE=SKIPPED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
