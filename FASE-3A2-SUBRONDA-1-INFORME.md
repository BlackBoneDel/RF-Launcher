# Fase 3-A.2 — Sub-ronda 1: E2E Linux + cierre de los 6 bloqueantes

> **Estado:** implementación en PR, pendiente de CI verde. Esta NO cierra la Fase 3-A.2 completa.
> Los módulos H, I/J (selectores de UI), K, L, M, N y O–S quedan para la sub-ronda 2.

## 1. Decisiones aplicadas de la Sección 0

| Decisión | Aplicación |
|---|---|
| E2E Linux-only | Job `e2e-linux` en `ci.yml`: `xvfb-run` + `tauri-driver` + WebdriverIO. No hay `tauri-driver` en Windows ni provider embebido en macOS. |
| Sin plugins WDIO en Release | El binario de prueba se compila con `cargo build --locked --features e2e`. El Release sigue usando `default = []`. |
| Prohibido mockear `invoke` | El spec dispara clicks reales sobre el binario Tauri. Ningún comando IPC es interceptado ni reemplazado. |
| Fixtures + caché 24 h + fallback | `catalog-fixtures/` + `src-tauri/src/catalog.rs` + `scripts/verify_catalog_fixtures.py`. |
| Smoke específico de Fabric Meta | `scripts/verify_catalog_fixtures.py --live` sondea `meta.fabricmc.net/v2/versions/loader/1.21.1`; si no hay red imprime `FABRIC_PROBE=SKIPPED`, nunca OK falso. |
| `auth_*` | Comentario explícito en `minecraft.rs`: placeholder de identidad offline, NO sistema de cuentas, reservado a Fase 3-C. |
| Alcance recortado | No se tocaron rediseño visual, selectores de MC/Fabric, RAM global en config, barra de botones, panel completo, tracking ni menú de configuración por secciones. |

## 2. Diagnóstico de causa real (Sección 0)

| Bloqueante | Categoría | Causa raíz |
|---|---|---|
| Editar instancia | (a) | No existía comando `update_instance` ni método de actualización; `CreateInstanceForm` era sólo creación. |
| Eliminar instancia | (a) | No existía comando `delete_instance`, ni borrado seguro, ni UI. |
| Override jerárquico | (c) | El motor resolvía bien, pero `set_override` no sincronizaba `ramMinMb`/`ramMaxMb`; la tarjeta seguía mostrando “hereda global”. |
| Java automático con progreso | (d) | Backend de descarga existía y se invocaba, pero no había `Content-Length`, eventos, UI de progreso, detección de Java del sistema ni lectura real de `JavaAutoDownload`. |
| Lanzar instancia | (d) | El backend hacía `spawn()` real, pero nunca se había verificado con un click real; faltaban feedback de error/log y había piezas incompletas en el pipeline. |
| Matar instancia | (d) | Backend y UI estaban cableados, pero sólo se había probado contra un binario de test de 96 MiB, no contra el proceso lanzado por el flujo real. |

## 3. Cambios principales

### Backend (`src-tauri`)

- Nuevos comandos registrados en `main.rs` y `capabilities/default.json`:
  - `update_instance`
  - `delete_instance`
  - `clear_instance_config`
  - `get_global_config`
- `Instance`:
  - `update()` con validación.
  - `delete()` con canonicalización y defensa de path.
  - `set_override()` sincroniza `ram_min_mb` / `ram_max_mb`.
  - `clear_override()` para “usar global”.
- `ConfigValue::as_bool()`.
- `JavaManager`:
  - `ensure_with_options(version, allow_download, progress)`.
  - Detección de Java del sistema con `java -version` real.
  - Eventos `JavaProgressEvent` con `Content-Length`.
  - Base Adoptium configurable bajo `--features e2e` para el fixture server local.
- `launch_instance`:
  - Recibe `AppHandle` y emite `java-download-progress`.
  - Lee `JavaAutoDownload` desde la configuración jerárquica.
  - Pasa el observador de progreso a `JavaManager`.
- `minecraft.rs`:
  - Comentario de `auth_*`.
  - Hook `--features e2e` para resolver un `ResolvedLaunch` sin red, manteniendo el `spawn()` real.
- `catalog.rs`:
  - Caché con TTL 24 h, parseo puro, verificación SHA-256 de fixtures y fallback obligatorio.
  - **Sin comandos IPC ni UI todavía**: es la capa de datos para la sub-ronda 2.

### Frontend

Sin rediseño visual; sólo cableado funcional y `data-testid`s:

- `App.tsx`: escucha `java-download-progress` y muestra barra/porcentaje real.
- `CreateInstanceForm.tsx`: `data-testid`s para el E2E.
- `InstanceList.tsx`: `data-testid`s y estado vacío identificable.
- `InstanceCard.tsx`:
  - Editar instancia con modal y `updateInstance`.
  - Eliminar instancia con confirmación modal y `deleteInstance`.
  - Override RAM con botones “Fijar” y “Usar global / limpiar”.
  - `data-testid`s para launch/kill/mensajes.
- `SettingsView.tsx`: carga real de `JavaAutoDownload` con `getGlobalConfig`.
- `lib/tauri.ts` / `types.ts`: wrappers y tipos nuevos, incluido `JavaProgressEvent`.

### E2E (`e2e/`)

- `wdio.conf.js`:
  - Inicia un servidor HTTP fixture de Adoptium.
  - Aísla `XDG_DATA_HOME`.
  - Arranca `tauri-driver`.
  - Compila/usa `src-tauri/target/debug/rf-launcher` con `--features e2e`.
- `specs/blockers.e2e.js`:
  1. Crear instancia desde la UI.
  2. Editar nombre/versión y verificar `instance.json` + UI.
  3. Fijar override RAM, verificar `ramMaxMb`/`configOverrides`, limpiarlo y verificar “hereda global”.
  4. Lanzar: observar progreso real de Java >0%, verificar mensaje “Iniciado”, leer PID real del proceso Java fixture.
  5. Matar: verificar que el PID desaparece del SO.
  6. Eliminar: confirmar modal y verificar que la carpeta desaparece.

El “JDK” del E2E es un script ejecutable dentro de un `.tar.gz` servido con `Content-Length` y checksum SHA-256. No es un JDK real, pero **sí es un proceso del sistema operativo real** lanzado por `Command::spawn()` y terminado por PID. No se mockea `invoke`.

## 4. Fixtures de catálogo

- `catalog-fixtures/mojang-version-manifest-v2.json`
- `catalog-fixtures/fabric-loader-1.21.1.json`

Ambos son sobres con `sourceUrl`, `fetchedAt`, `sha256` y `payload`. Son copias **recortadas** y están documentadas como fallback, no como catálogo completo. El script de CI valida integridad offline y sondea red real con `--live`.

## 5. Pruebas de mutación / por qué el E2E detecta regresiones

- Si se revierte `update_instance`, el paso “editar” no encuentra el botón/cambio y falla.
- Si se revierte `delete_instance`, el modal no borra la carpeta y falla.
- Si se revierte la sincronización de `ram_*`, `ram-label-*` sigue diciendo “hereda global” y falla.
- Si se quitan los eventos de progreso, no aparece `java-progress` con `%` y falla.
- Si `launch_instance` deja de hacer `spawn()`, no se escribe el PID file y falla.
- Si `kill_instance` deja de verificar el PID, el proceso sigue vivo y falla.

## 6. Limitaciones honestas

- E2E sólo en Linux. Windows/macOS requieren checklist manual en esta sub-ronda.
- El E2E no descarga Minecraft real: usa un hook `--features e2e` para evitar red/juego pesado, pero conserva Java, RAM, IPC, spawn y kill reales.
- No se certificó aún que el pipeline completo abra una ventana jugable de Minecraft en un usuario final; eso requiere la prueba manual del instalador de Windows/macOS y la sub-ronda 2.
- Los módulos I/J sólo tienen capa de datos/fixtures; **no hay selectores de versión en UI todavía**.
- No hay tracking de tiempo de juego ni rediseño visual en esta sub-ronda.
- El job E2E instala `tauri-driver` con `cargo install`, lo que puede aumentar el tiempo de CI; se medirá en las primeras 2–3 corridas antes de extender a Windows.

## 7. Checklist manual para Windows/macOS (esta sub-ronda)

1. Instalar el binario correspondiente del PR.
2. Crear instancia `Manual 1.21` sin Fabric.
3. Editar nombre a `Manual Editada` y versión a `1.21.1`; guardar; reabrir y verificar persistencia.
4. Fijar RAM máxima 4096 MB; verificar que la tarjeta muestre `4096 MB`; usar “Usar global / limpiar”; verificar `hereda global`.
5. Activar “Descargar Java automáticamente” en Configuración; lanzar y observar la barra “Descargando Java 21…” con porcentaje.
6. Verificar mensaje “Iniciado (PID …)” y que exista un proceso Java en Administrador de tareas / Monitor de actividad.
7. Pulsar “Forzar cierre”; verificar mensaje “Instancia detenida” y que el proceso Java desaparezca.
8. Borrar la instancia; confirmar modal; verificar que desaparece de la lista y de la carpeta de datos.
9. Repetir creación/edición/borrado con Fabric activado para detectar regresiones de loader.
10. Cambiar tema claro/oscuro y reiniciar; verificar que la persistencia de tema sigue funcionando.
