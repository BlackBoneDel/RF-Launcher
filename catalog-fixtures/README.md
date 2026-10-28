# Catalog fixtures (Fase 3-A.2, sub-ronda 1)

Estos ficheros son **fallbacks offline deliberadamente recortados**, no el catálogo completo ni una lista hardcodeada que se haga pasar por datos reales.

- `mojang-version-manifest-v2.json`: copia mínima del manifiesto v2 de Mojang con `latest` y dos entradas reales. El runtime usará el endpoint completo; este fichero sólo evita que el selector falle sin red.
- `fabric-loader-1.21.1.json`: copia mínima de `meta.fabricmc.net/v2/versions/loader/1.21.1` con dos loaders reales (`0.20.1 stable`, `0.20.0 inestable`) y `launcherMeta.mainClass`.

Cada fixture es un sobre con:

- `sourceUrl`: endpoint público de origen.
- `fetchedAt`: fecha UTC de captura.
- `sha256`: SHA-256 del canonical JSON de `payload` (`sort_keys=True`, sin espacios).
- `payload`: datos parseables.

`scripts/verify_catalog_fixtures.py` valida el sobre y, con `--live`, sondea los endpoints reales. Si no hay red, imprime `LIVE_PROBE=SKIPPED` y **no** reporta un OK falso.
