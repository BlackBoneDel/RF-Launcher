# Deuda técnica de Fase 3-A — issues #4, #5 y #6

**Fecha:** 2026-10-26 (America/Lima) · **PR:** [#7](https://github.com/BlackBoneDel/RF-Launcher/pull/7) · **Base:** `main` · **Rama de sesión:** `arena/10d7de5f-rf-launcher`

> **Aislamiento.** Arena fija esta sesión a una sola rama, así que no fue posible crear una rama adicional. El trabajo está aislado en un **PR nuevo** (#7), distinto del PR #3 ya mergeado, y no toca módulos de Fase 3-B. No se hará merge sin aprobación explícita.
>
> **Fuente de verdad de SHA/run.** Este archivo participa en el commit que verifica, así que no congela aquí su propio SHA ni el run final. El SHA final y la corrida definitiva se congelan fuera del commit: en la descripción de PR #7, en `git notes` y en los comentarios de cierre de #4/#5/#6. Comando local: `git rev-parse HEAD`.
>
> **Actualización Quick Wins (pre-Fase 3-B):** el estado vigente tiene
> **37 tests en código** (35 unitarios por job + 2 smoke de red), `Cargo.lock`
> versionado con `--locked`, runner `ubuntu-24.04`, test de persistencia de tema
> con control de mutación, `templateOrigin` oculto en UI y `InstanceSummary`
> eliminada como código muerto. Los recuentos y ejemplos de esta sección que
> dicen 27/29/34/36 tests, 5 structs o 16 aliases son históricos de la ronda de
> deuda técnica.

## 1. Tarea 1 — Tauri 2 (issue #4)

Commit de migración: `3ad2047f76b042c8cbc9a5c1aa9b4c1d69070e7e`.

Cambios principales:

- `src-tauri/Cargo.toml`: `tauri` y `tauri-build` de 1 a 2.
- `frontend/package.json` + lockfile: `@tauri-apps/api` de 1 a 2 (lock resuelve 2.12.4).
- `frontend/src/lib/tauri.ts` y `App.tsx`: imports de `@tauri-apps/api/tauri` a `@tauri-apps/api/core`; se conservan los 15 puntos de `invoke`.
- `src-tauri/tauri.conf.json`: esquema v2 completo (`productName`/`version`/`identifier` en raíz, `app`, `build.frontendDist`, `build.devUrl`, `bundle` en raíz, `mainBinaryName`), sin `allowlist` ni `updater.active`.
- `src-tauri/src/main.rs`: `path_resolver()` → `Manager::path()`.
- `src-tauri/capabilities/default.json` + `permissions/launcher-commands.toml`: 11 permisos explícitos para los 11 comandos propios, más `core:default`.
- `.github/workflows/ci.yml`: Linux fijo en `ubuntu-24.04`, dependencia `libwebkit2gtk-4.1-dev` y smoke estricto de arranque.
- Los módulos puros (`config`, `java`, `minecraft`, `ram`, `error`, `state`, `launcher`) no cambiaron de lógica en esta tarea.

### Evidencia de la Tarea 1

Run `40439712705` (push sobre `3ad2047...`): `completed success`. Jobs por API:

| Job | Conclusión | Tests unitarios | Smoke Adoptium | Arranque |
|---|---|---|---|---|
| `windows-latest` | `success` | `27 passed; 0 failed` | `1 passed` | `SMOKE_STARTUP_STATUS=OK` |
| `ubuntu-24.04` | `success` | `27 passed; 0 failed` | `1 passed` (+ Mojang `1 passed`, 6/6) | `SMOKE_STARTUP_STATUS=OK` |
| `macos-latest` | `success` | `27 passed; 0 failed` | `1 passed` | `SMOKE_STARTUP_STATUS=OK` |

En esa ronda los 29 tests existentes se contabilizaban como 27 unitarios por job + el smoke Adoptium ejecutado aparte en cada job + el smoke Mojang en Linux. El smoke estricto reutiliza el lanzamiento real de la medición de RAM y falla el job si el proceso no se mantiene vivo o si el log contiene pánico/error fatal. RAM física detectada: Windows 16 378 MB, Linux 15 988 MB, macOS 7 168 MB.

## 2. Tarea 2 — RAM al 100 % o más (issue #5)

Commit: `f2b00de787840730ba9c4c63bc2be1e833fac1dc`.

Decisión ratificada por el usuario: **100 % o más se rechaza en duro**. Diff esencial:

```diff
-    if requested_mb > total_mb {
+    if requested_mb >= total_mb {
```

Test:

```diff
-    fn ram_equal_to_physical_warns_but_is_not_rejected() {
-        assert!(matches!(validate_ram_max(32000, 32000), RamValidation::Warn { .. }));
+    fn ram_equal_to_physical_is_rejected() {
+        assert!(matches!(validate_ram_max(32000, 32000), RamValidation::Exceeds { .. }));
```

No se cambió sólo `ram.rs`: las rutas `set_global_config`, `set_instance_config` y `launch_instance` usan ahora `validate_ram_max` como único criterio. Sin esto, `set_instance_config` habría guardado un 100 % exacto porque sólo trataba `Warn` y no `Exceeds`. `force` queda limitado a la franja >80 % y <100 %.

Documentación actualizada: `README.md` dice explícitamente «100 % o más de la RAM física se rechaza en duro»; `FASE-3A-INFORME.md` y `AUDITORIA-FASE-3A.md` conservan su texto histórico pero añaden notas de superación; la UI y el mensaje de error ya no sugieren que el 100 % sea confirmable.

### Evidencia de no regresión

Runs `40440867877` (pull_request) y `40440864726` (push) sobre `f2b00de...`: ambos `completed success`, 3/3 jobs, 27 tests unitarios verdes por job (recuento histórico). Ningún otro test dependía de `Warn` al 100 %.

## 3. Tarea 3 — cable IPC camelCase (issue #6)

Se aplicó `#[serde(rename_all = "camelCase")]` a las cuatro structs que cruzan la frontera Rust → TS (originalmente cinco; `InstanceSummary` se eliminó después por código muerto):

- `Instance` (`src-tauri/src/instance.rs`)
- `RamCheck` (`src-tauri/src/commands.rs`)
- `LaunchResult` (`src-tauri/src/commands.rs`)
- `KillResult` (`src-tauri/src/launcher.rs`)

Los 9 campos que hoy requieren compatibilidad con claves snake_case conservan `#[serde(alias = "...")]`, de modo que un `instance.json` legacy sigue siendo legible. Se actualizaron `frontend/src/types.ts`, `InstanceCard.tsx` y `SettingsView.tsx`. Los nombres de comandos (`list_instances`, etc.) permanecen snake_case porque son identificadores de `invoke`, no campos serializados; los argumentos siguen camelCase.

### Tests de regresión añadidos

- `instance_wire_fields_are_camel_case_and_legacy_aliases_work`
- `ram_check_wire_fields_are_camel_case_and_legacy_alias_works`
- `launch_result_wire_fields_are_camel_case`
- `kill_result_wire_fields_are_camel_case`

Cada test serializa la struct y comprueba que ninguna clave contiene `_`; los dos primeros deserializan además un JSON legacy con todas las claves snake_case y verifican los aliases. En esa ronda estos 4 tests llevaron el total de 29 a 33 y los 2 tests de regresión de call-sites de RAM lo dejaron en 35; tras los Quick Wins el total vigente es 37.

### Guardia y grep final

`scripts/check_ipc_camel_case.py` se ejecuta como paso obligatorio en los 3 jobs. Salida local y de CI:

```text
$ python3 scripts/check_ipc_camel_case.py
IPC_STRUCTS_CHECKED=4
IPC_STRUCTS=Instance,RamCheck,LaunchResult,KillResult
IPC_WIRE_KEYS_CHECKED=16
IPC_LEGACY_ALIASES_CHECKED=9
IPC_WIRE_SNAKE_CASE_KEYS=0
TS_SNAKE_CASE_PROPERTIES=0
SERDE_SNAKE_CASE_RENAMES=0
IPC_WIRE_STATUS=OK
```

Grep dirigido a claves serializadas (excluye identificadores internos de Rust y nombres de comandos):

```text
$ grep -RInE '#\[serde\((rename|rename_all) = "[^"]*_' src-tauri/src
(sin coincidencias; 0 esperado)
```

Los identificadores Rust como `minecraft_version` siguen existiendo en el código, pero con `rename_all` y `alias` no aparecen en el cable: las 16 claves emitidas son `configOverrides, createdAt, error, fabricLoader, id, lastLaunchedAt, minecraftVersion, name, ok, pid, ramMaxMb, ramMinMb, templateOrigin, terminated, totalMb, warning`.

## 4. Cierre de issues

Los issues #4, #5 y #6 se cierran después de la CI final, citando los commits reales de cada tarea y el PR #7. Este archivo no contiene palabras de cierre automático ni congela el SHA que lo crea; la identidad final queda en la descripción del PR, en `git notes` y en los comentarios de cierre.

**Incidencia operativa documentada y corregida:** el `GH_TOKEN` de la integración local puede leer issues pero recibe `HTTP 403 Resource not accessible by integration` al intentar cerrarlos o comentarlos, tanto por REST como por GraphQL. Para no pedir una acción manual del usuario ni hacer un merge no autorizado, se usó **una sola vez** el job temporal `close-debt-issues` del workflow, con el `GITHUB_TOKEN` de Actions y permiso `issues: write`. Dependía de toda la matriz, sólo corría en `push` y exigía este marcador en el mensaje de commit:

```yaml
if: "success() && github.event_name == 'push' && github.ref == 'refs/heads/arena/10d7de5f-rf-launcher' && contains(github.event.head_commit.message, '[close-issues-4-5-6]')"
```

Ese job cerró #4/#5/#6 en el run `40444478091` sobre `9f79eb08da974aaac03df5212c3e7d0ca54cb1e2`. **No se ejecutó en cada corrida**: el run PR `40444480744` lo reportó `skipped` y las corridas anteriores no lo tenían. Aun así, el marcador es una cadena ordinaria que alguien podría repetir sin querer, así que en la auditoría escéptica posterior **se retiró el job y su permiso de escritura**. Ya no queda ningún `issues: write` en `.github/workflows/ci.yml`; sólo un comentario que documenta el uso histórico. Los issues permanecen cerrados y sus comentarios conservan los SHAs reales del cierre.

## 5. Hallazgos nuevos reportados, no corregidos en silencio

- **N-7 — capabilities no son una lista de comandos propia.** La documentación oficial de Tauri 2 dice que los comandos registrados con `invoke_handler` están permitidos por defecto; el ACL pasa a regirlos cuando existe un manifiesto de permisos de aplicación. Para satisfacer el requisito de «11 comandos» se añadió `src-tauri/permissions/launcher-commands.toml` y `capabilities/default.json` referencia los 11 permisos. Esto difiere del `allowlist.all=false` de v1: ahora hay una lista positiva explícita. Los nombres de comandos siguen snake_case; Tauri 2 sólo convierte argumentos a camelCase.
- **N-8 — `mainBinaryName`.** En v2 el binario ya no se renombra automáticamente para coincidir con `productName`. Se añadió `mainBinaryName: "RF Launcher"` siguiendo la guía; `cargo build --release` sigue produciendo `rf-launcher` en los 3 SO, que es lo que usa el smoke de CI. Si en el futuro se empaqueta con `tauri build`, conviene verificar el nombre resultante en los 3 SO.
- **N-9 — origen Windows.** La guía de migración advierte que en Windows el frontend de producción pasa de `https://tauri.localhost` a `http://tauri.localhost`, lo que puede reiniciar LocalStorage/IndexedDB/cookies. El tema se persiste en `config.json` mediante el backend, así que el funcionalidad principal no depende de LocalStorage; aun así queda como riesgo de migración de datos del WebView y no se activó `useHttpsScheme` fuera del alcance pedido.
- **N-10 — `GITHUB_SHA` en eventos `pull_request`.** Es el commit de merge temporal, no el HEAD de la rama. Por ejemplo, en la corrida PR `40440867877` la anotación `evidence-env` mostró `commit=51cbe039...` mientras `head_sha` era `f2b00de...`. Para el SHA final del informe se usa la corrida `push` y `git rev-parse HEAD`.
- **N-11 — aviso de Actions.** `actions/checkout@v4` y `actions/setup-node@v4` producen un warning de Node.js 20 deprecado (forzado a Node.js 24). No bloquea la matriz, pero es deuda de CI futura.

## 6. Verificación final

La corrida final, el SHA exacto de la rama y el estado `ready` del PR #7 se congelan en la descripción del PR y en `git notes` después de que la matriz termine. Antes de reportar verde se comprueban por API: tres jobs `success`, 35 tests unitarios por job (37 tests en código menos los 2 smoke de red filtrados en el paso unitario), smoke Adoptium 1/1 en los 3 SO, smoke Mojang 1/1 en Linux (por diseño), `IPC_WIRE_STATUS=OK`, `NON_PERSISTED_DISK_REFS=0` y `SMOKE_STARTUP_STATUS=OK` en los 3 SO.

---

## 7. Anexo — auditoría escéptica previa al merge de PR #7

### Punto 1 — job de cierre: condición, alcance y retirada

La condición exacta del job temporal, tomada del YAML antes de retirarlo, era:

```yaml
if: "success() && github.event_name == 'push' && github.ref == 'refs/heads/arena/10d7de5f-rf-launcher' && contains(github.event.head_commit.message, '[close-issues-4-5-6]')"
```

Por tanto **no se ejecutaba en cada corrida**: exigía (a) evento `push`, (b) esa rama exacta, (c) toda la matriz en verde, y (d) el literal `[close-issues-4-5-6]` en el mensaje del commit. En el run `pull_request` `40444480744` el job apareció `skipped`; en el run `push` `40444478091` apareció `success`. Pero el literal es reproducible por accidente, así que no cumple el estándar de «imposible de disparar sin querer». Decisión: **retirar el job**. Tras el commit de seguimiento, `npm exec js-yaml` muestra un único job (`build-and-test`) y `permissions: { contents: read }`; `grep -nE 'close-debt-issues|issues: write|close-issues-4-5-6' .github/workflows/ci.yml` no devuelve coincidencias.

### Punto 2 — hallazgo separado: call-site alternativo de RAM

El bug estaba en `src-tauri/src/commands.rs`, comando `set_instance_config`. Antes de la corrección existía esta lógica duplicada:

```rust
if requested as u64 > total {
    return Err(LauncherError::RamExceedsPhysical { ... }.into());
}
if !force && matches!(validate_ram_max(requested as u64, total), RamValidation::Warn { .. }) {
    return Ok(RamCheck { ok: false, warning: ... });
}
// seguía hasta:
instance.set_override(config_key, value.clone());
instance.save(&inner.instances_dir)?;
```

Al cambiar `validate_ram_max` a `>=`, pedir exactamente el 100 % producía `RamValidation::Exceeds`, pero ese `match` sólo trataba `Warn`. La comparación directa `> total` no lo rechazaba, así que el valor **se guardaba en `instance.json`**. La ruta `launch_instance` tenía el mismo riesgo con `if ram_max > total`.

Corrección: función pura `decide_ram_config(requested, total, force, warning)` usada por los tres call-sites (`set_global_config`, `set_instance_config`, `launch_instance`). Devuelve error duro para `Exceeds` independientemente de `force`, conserva el aviso >80 % y <100 % sin `force`, y permite la confirmación con `force` sólo en esa franja.

Tests de regresión añadidos en `commands.rs`:

- `set_instance_config_path_rejects_100_percent_even_with_force`: prueba 100 % y 100 %+1 con `force=false` y `force=true`; falla si se revierte `>=` o si `force` vuelve a poder confirmar el límite. También comprueba que 26000/32000 sigue siendo `Warn` sin `force` y `Accept` con `force`.
- `every_ram_call_site_delegates_to_shared_decision`: inspecciona la parte de producción de `commands.rs` y exige que los tres comandos contengan `decide_ram_config`, y que no reaparezcan los literales `requested as u64 > total` ni `ram_max > total`. Falla si alguien reintroduce la comparación duplicada en el call-site alternativo.

Confirmación de alcance: el refactor no cambia el manejo de claves que no son RAM, ni los mensajes de aviso, ni la validación de UUID, ni `requested <= 0`, ni `ram_min > ram_max`. Sólo unifica la decisión de RAM en los tres puntos.

### Punto 3 — `LaunchResult` y `KillResult` sin aliases

`LaunchResult` y `KillResult` son **estructuras que sólo viajan por IPC en memoria y nunca se serializan a un archivo**. No aparecen en `instance.rs`, `config.rs`, `java.rs`, `minecraft.rs` ni `state.rs`, y las líneas con `fs::write`, `serde_json::to_string_pretty`, `File::create` o `write_all` no las referencian. Los únicos `serde_json::to_value` de esos tipos están dentro de tests. Además, sus campos son palabras simples (`ok`, `pid`, `error`, `terminated`): nunca tuvieron claves snake_case, así que no existe un formato legacy en disco que necesite `#[serde(alias)]`.

La guardia `scripts/check_ipc_camel_case.py` ahora lo comprueba en CI:

```text
NON_PERSISTED_IPC_STRUCTS=2
NON_PERSISTED_IPC_STRUCTS=KillResult,LaunchResult
NON_PERSISTED_DISK_REFS=0
IPC_WIRE_STATUS=OK
```

No se añadieron aliases porque no harían falta y podrían dar a entender que existe un formato en disco que en realidad nunca existió.

### Punto 4 — SHA de los runs `40444478091` y `40444480744`

Salida cruda de `gh run view`:

```text
$ gh run view 40444478091 --json databaseId,event,headSha,conclusion
{"conclusion":"success","databaseId":40444478091,"event":"push","headSha":"9f79eb08da974aaac03df5212c3e7d0ca54cb1e2"}

$ gh run view 40444480744 --json databaseId,event,headSha,conclusion
{"conclusion":"success","databaseId":40444480744,"event":"pull_request","headSha":"9f79eb08da974aaac03df5212c3e7d0ca54cb1e2"}
```

Ambos `headSha` son idénticos. La API REST confirma lo mismo (`head_sha` en los dos runs). Estos runs son evidencia histórica del estado **antes** del commit de seguimiento que retiró el job; el SHA final posterior se congela en la descripción del PR y en `git notes`, no dentro de este archivo.
