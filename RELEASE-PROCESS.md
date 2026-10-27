# Proceso reutilizable de Release — RF Launcher

Este documento describe cómo publicar instaladores reales con el workflow único
`.github/workflows/ci.yml`. No crea un segundo workflow: la guardia anti-falsos
verdes sigue viendo un solo fichero YAML en `.github/workflows/`.

## 1. Actualizar metadatos de versión

Antes de un Release, deben coincidir:

- `src-tauri/Cargo.toml` → `version = "X.Y.Z"`.
- `src-tauri/tauri.conf.json` → `"version": "X.Y.Z"`.
- `frontend/package.json` y `frontend/package-lock.json` → `"version": "X.Y.Z"`.
- `.github/release-manifest.json`:
  - `version`: `X.Y.Z`.
  - `release_tag`: `vX.Y.Z`.
  - `build_trigger_tag`: `build-vX.Y.Z`.
  - `target_sha`: SHA exacto de `main` al que debe apuntar el Release.
  - `product_name`: nombre visible de la app.

El `target_sha` se verifica dentro del workflow contra `origin/main`. Si `main`
avanzó, el Release falla en vez de apuntar silenciosamente a otro commit.

## 2. Verificación normal

Todo push/PR sigue ejecutando la matriz `build-and-test` en Windows, macOS y
Linux. El job de Release **no** se ejecuta en cada push.

```bash
git push origin <rama>
gh pr create ...   # PR separado, sin merge automático
```

## 3. Disparo controlado del build de instaladores

El workflow tiene dos disparadores controlados para Release:

1. **Tag de build temporal** `build-vX.Y.Z` (usado en v0.1.0).
2. `workflow_dispatch` desde la pestaña Actions de GitHub (útil cuando el
   workflow ya está mergeado en `main` y se dispara con permisos de usuario).

Para el tag de build:

```bash
git tag build-v0.1.0
git push origin build-v0.1.0
```

Eso ejecuta únicamente los jobs `build-installers` y `publish-release`.
El job `build-installers` usa `npx --yes @tauri-apps/cli@2.12.4 build` en cada
runner con bundles explícitos:

- Windows: `nsis` obligatorio → `.exe`; `msi` se intenta como segundo bundle opcional.
- macOS: `dmg` obligatorio → `.dmg`; si el `.app` intermedio existe, también se publica como `.zip`.
- Linux: `deb` obligatorio → `.deb`; `appimage` se intenta como bundle opcional si las dependencias del runner lo permiten.

> Nota: `bundle.targets` en `tauri.conf.json` sigue siendo `"all"`, pero el CLI
> recibe `--bundles` por SO para controlar el resultado y evitar generar rpm
> innecesarios. La documentación oficial de Tauri 2 confirma que `targets: "all"`
> y las listas explícitas son válidas.

## 4. Publicación y verificación end-to-end

`publish-release`:

1. Descarga los artefactos de los 3 runners con `actions/download-artifact@v4`.
2. Renombra cada instalador con prefijo de SO y arquitectura.
3. Genera `SHA256SUMS.txt`.
4. Crea el Release `vX.Y.Z` con `gh release create --target <target_sha>`.
5. Sube todos los instaladores + `SHA256SUMS.txt`.
6. Vuelve a descargar **los assets publicados** con `gh release download`.
7. Ejecuta `sha256sum -c SHA256SUMS.txt` sobre lo descargado.
8. Compara además el `digest` SHA-256 de la API de GitHub cuando está disponible.
9. Publica anotaciones `evidence-release-*` con el resultado.

Si el hash descargado no coincide, el workflow falla: no se considera Release
verificado.

## 5. Limitaciones conocidas

- Los instaladores se publican **sin firma de código**. Windows SmartScreen y
  macOS Gatekeeper mostrarán advertencias. Para distribución pública real habrá
  que configurar certificados y notarización.
- El workflow no puede probar una instalación interactiva en un sistema limpio.
  Eso se cubre con `RELEASE-0.1.0-MANUAL-CHECKLIST.md`.
- El icono fuente se guarda en `src-tauri/icons/app-icon-source.png`; los iconos
  nativos se regeneran con:
  `cd src-tauri && npx --yes @tauri-apps/cli@2.12.4 icon icons/app-icon-source.png`.
- El tag `build-vX.Y.Z` es un disparador de build, no el tag del Release; puede
  borrarse después con `git push --delete origin build-vX.Y.Z`.

## 6. Futuras versiones

Para `v0.2.0` tras Fase 3-B:

1. Repetir §1 con la nueva versión y nuevo `target_sha`.
2. Actualizar/reemplazar `RELEASE-0.1.0-MANUAL-CHECKLIST.md` por la checklist de la nueva fase.
3. Actualizar las notas generadas en el workflow para describir las funciones nuevas.
4. Repetir §3 y §4.
