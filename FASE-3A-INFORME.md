# RF Launcher — Fase 3-A: Núcleo de Gestión de Instancias

**Fecha:** 2026-10-25 · **Repositorio:** `BlackBoneDel/RF-Launcher` ·
**Branch:** `arena/2412f038-rf-launcher` · **Commit final:** `30c3d015fb837378ab016b719d829e71ffa9fdff`

**Estado:** Fase 3-A implementada y verificada en una matriz real de tres sistemas
operativos (Windows / macOS / Linux). Corrida final verde:
[run 40238629977](https://github.com/BlackBoneDel/RF-Launcher/actions/runs/40238629977).

---

## 1. Entorno (auditoría con evidencia cruda)

Ejecutado en el sandbox local, sin filtrar ni interpretar:

```
$ cargo --version   -> /bin/bash: cargo: command not found
$ rustc --version   -> /bin/bash: rustc: command not found
$ rustup --version  -> /bin/bash: rustup: command not found
$ node --version    -> v22.22.3
$ npm --version     -> 10.9.8

$ curl -sS -o /dev/null https://crates.io
curl: (35) OpenSSL SSL_connect: SSL_ERROR_SYSCALL in connection to crates.io:443
HTTP 000

$ curl -sS -o /dev/null https://api.adoptium.net/v3/assets/version/21/latest
curl: (35) OpenSSL SSL_connect: SSL_ERROR_SYSCALL in connection to api.adoptium.net:443
HTTP 000

$ curl -sS -o /dev/null https://registry.npmjs.org     -> HTTP 200
$ npm install left-pad@1.3.0                          -> added 1 package in 312ms

$ uname -a -> Linux e2b.local 6.1.158+ #1 SMP PREEMPT_DYNAMIC ... x86_64 GNU/Linux
$ free -h  -> Mem: 3.8Gi total, 3.3Gi free, 2 CPU

$ gh auth status -> Logged in to github.com as BlackBoneDel
```

**Conclusión del entorno:** el sandbox permite npm y GitHub, pero **no** crates.io,
**no** Adoptium, **no** `raw.githubusercontent.com`, **no** `sh.rustup.rs`, y no
tiene toolchain Rust. Por tanto **no existe verificación local del backend**:
`cargo build` / `cargo test` son imposibles aquí. Siguiendo el protocolo
"detente y reporta", no se improvisó un sustituto: se usó la estrategia
permitida de desarrollo local funcional + verificación final obligatoria en
GitHub Actions con matriz real.

Verificación que SÍ se pudo hacer localmente:

```
$ cd frontend && npm ci && npx tsc --noEmit && npm run build
TSC_OK
✓ 1940 modules transformed
dist/index.html                   0.45 kB │ gzip:  0.29 kB
dist/assets/index-BlandS4J.css   18.31 kB │ gzip:  4.36 kB
dist/assets/index-CRf2h6et.js   243.45 kB │ gzip: 75.24 kB
✓ built in 461ms
```

---

## 2. Tareas con evidencia

| # | Tarea | Dónde | Evidencia |
|---|---|---|---|
| 1 | Launcher standalone Tauri/Rust + React/TS/Vite, sin framework de launcher previo | `src-tauri/`, `frontend/` | `cargo build --release` y `npm run build` verdes en 3 SO |
| 2 | Modelo de Instancia con UUID, nombre, versión MC, Fabric Loader opcional, `template_origin`, carpeta aislada, overrides, timestamps | `src-tauri/src/instance.rs` | tests `instance_serializes_template_origin_as_null`, `list_returns_only_valid_instance_dirs` |
| 3 | Motor de configuración jerárquica instancia > global > default + tests de los 3 casos | `src-tauri/src/config.rs` | `resolve_instance_override_wins_over_global`, `resolve_falls_back_to_global_when_instance_does_not_override`, `resolve_falls_back_to_system_default_when_neither_defines_key` |
| 4 | Gestión automática de Java vía Adoptium con checksum, reintentos y estado consistente | `src-tauri/src/java.rs` | smoke test real en CI: `ensure_java_21_downloads_from_adoptium` (15.60 s, descarga + SHA-256 + `java -version`) |
| 5 | Validación de RAM contra RAM física real, error duro y warning >80 % | `src-tauri/src/ram.rs`, `frontend/src/components/InstanceCard.tsx` | `ram_detection_reads_real_physical_memory` imprime `RAM_FISICA_MB` en los 3 SO; UI con aviso y confirmación `force` |
| 6 | Lanzar y matar procesos reales con terminación verificada | `src-tauri/src/launcher.rs` | `kill_really_terminates_process_and_pid_disappears ... ok` en Windows, macOS y Linux |
| 7 | Línea base de RAM del launcher en reposo | `.github/workflows/ci.yml` | `VmRSS: 91500 kB` (ver §7) |
| 8 | Temas claro/oscuro persistentes | `src-tauri/src/commands.rs` (`get_theme`/`set_theme`), `frontend/src/components/ThemeToggle.tsx` | tema guardado como clave `Theme` del mismo motor jerárquico |
| 9 | CI matrix con evidencia separada por SO | `.github/workflows/ci.yml` | anotaciones `evidence-*` por check-run (ver §5) |

**Tests:** 27 en total. En la corrida final, Windows y macOS ejecutaron **25 passed / 0 failed**;
Ubuntu ejecutó **25 passed / 0 failed / 2 filtered** (los 2 filtrados son los smoke
tests de red, que se ejecutan aparte: **1 passed** cada uno). Los 2 tests de red
están bajo `#[cfg(target_os = "linux")]`, por eso no existen en Windows/macOS.

Límites respetados: sin mods/modpacks/loaders extra (solo Fabric básico), sin
cuentas/OAuth, sin RF Client ni repositorio hermano, sin plantilla hardcodeada,
sin lógica de negocio en React.

---

## 3. Bloqueos (reales, documentados)

1. **Sin Rust/Cargo y sin crates.io en el sandbox.** Imposible compilar o testear
   el backend localmente. Evidencia: `command not found` y
   `SSL_ERROR_SYSCALL` / HTTP 000 arriba. `rustup` tampoco se pudo instalar
   (`sh.rustup.rs` bloqueado).
2. **Sin Adoptium/Mojang en el sandbox.** La descarga real de un JDK y la
   consulta al manifiesto de Mojang no se pudieron probar localmente.
3. **Los logs y artefactos de GitHub Actions son inalcanzables desde el
   sandbox.** `gh api .../jobs/<id>/logs` y la descarga de artefactos redirigen a
   `productionresultssa*.blob.core.windows.net`, que devuelve
   `SSL_ERROR_SYSCALL`. Esto bloqueó la lectura directa de los logs de fallos
   durante varias iteraciones y obligó a construir un canal alternativo.
4. **`ubuntu-latest` (2026 = Ubuntu 24.04) es incompatible con Tauri 1.x.**
   Tauri 1 enlaza `webkit2gtk-4.0`, sustituido por 4.1 en 24.04.
5. **Sin pantalla en los runners.** Medir RAM del launcher en reposo requiere
   un servidor X; sólo se pudo hacer en Linux con `xvfb-run`.
6. **Sin verificación end-to-end de Minecraft real.** El sandbox no puede
   ejecutar la app de escritorio ni descargar el cliente de Mojang. El código de
   `launch_instance` compila en 3 SO y el mecanismo de kill se prueba con un
   proceso hijo real, pero no se ha lanzado una partida de Minecraft completa.
7. **Humo de red sólo en Linux.** Los smoke tests de Adoptium/Mojang son
   `#[cfg(target_os = "linux")]`; en Windows y macOS se verificó compilación,
   tests y mapeo estático, no una descarga real de JDK.

---

## 4. Soluciones realistas aplicadas

- **Verificación multiplataforma delegada a GitHub Actions.** Matriz
  `windows-latest` / `macos-latest` / `ubuntu-22.04` con `fail-fast: false`,
  `cargo build --release`, `cargo test`, build de frontend, smoke tests y
  medición de RAM. No se aceptó "compiló en mi máquina": el backend nunca
  compiló en este sandbox y así se declara.
- **Excepción razonada de `ubuntu-latest` → `ubuntu-22.04`.** Documentada en
  el propio YAML y en el README. Es la única forma de verificar Tauri 1 de
  verdad en Linux sin migrar a Tauri 2 (fuera del alcance de la fase).
- **Canal de diagnóstico alternativo a los logs.** El workflow publica
  anotaciones `::error title=cargo-log-N::` en fallos y `::notice
  title=evidence-*::` siempre. Las anotaciones SÍ se leen por API:
  `gh api repos/BlackBoneDel/RF-Launcher/check-runs/<CRID>/annotations`.
  Sin esto, los 11 fallos intermedios habrían sido indiagnosticables desde
  aquí. **Restricción honesta:** las anotaciones son un extracto filtrado,
  no el log completo.
- **Validación de APIs externas contra fuentes reales** antes de escribir el
  código (ver §6), porque no se podía probar la red localmente.
- **Frontend verificado localmente** con `tsc --noEmit` + `npm run build`, y
  además reconstruido dentro de cada job de la matriz.

---

## 5. Tres links / logs separados por plataforma

Corrida final verde, commit `30c3d015`:

| SO | Job (log) | Anotaciones (API, legibles) | Resultado |
|---|---|---|---|
| Windows Server 2025 | [job 121305448146](https://github.com/BlackBoneDel/RF-Launcher/actions/runs/40238629977/job/121305448146) | [annotations](https://api.github.com/repos/BlackBoneDel/RF-Launcher/check-runs/121305448146/annotations) | `25 passed; 0 failed` · `RAM_FISICA_MB=16378` |
| macOS 26.6.2 arm64 | [job 121305448290](https://github.com/BlackBoneDel/RF-Launcher/actions/runs/40238629977/job/121305448290) | [annotations](https://api.github.com/repos/BlackBoneDel/RF-Launcher/check-runs/121305448290/annotations) | `25 passed; 0 failed` · `RAM_FISICA_MB=7168` |
| Ubuntu 22.04 (Linux 6.8.0-azure) | [job 121305448272](https://github.com/BlackBoneDel/RF-Launcher/actions/runs/40238629977/job/121305448272) | [annotations](https://api.github.com/repos/BlackBoneDel/RF-Launcher/check-runs/121305448272/annotations) | `25 passed; 0 failed` · 2 smoke `1 passed` · `RAM_FISICA_MB=15988` · VmRSS 91500 kB |

Run completo: [40238629977](https://github.com/BlackBoneDel/RF-Launcher/actions/runs/40238629977).
Ejemplo de evidencia fallida (para contraste): [40236919866](https://github.com/BlackBoneDel/RF-Launcher/actions/runs/40236919866).
Corridas verdes previas: `40237313439`, `40237684461`, `40238204012`.

> **Falso verde detectado y excluido:** el run
> [40216745092](https://github.com/BlackBoneDel/RF-Launcher/actions/runs/40216745092)
> figura como `success` en la lista de la branch, pero es un workflow de
> diagnóstico llamado `Minimal test` con **1 solo job (`hello`)** y ninguna
> matriz. Se usó al principio para comprobar permisos de Actions. No cuenta
> como verificación del launcher y se excluye explícitamente del recuento.

---

## 6. Decisiones de diseño (y defectos propios encontrados)

1. **JSON por instancia en vez de TOML.** El IPC de Tauri ya es JSON; el mismo
   esquema sirve en disco y en la frontera. Los `ConfigValue` etiquetados se
   serializan de forma natural. Mojang y Adoptium también hablan JSON.
2. **UUID inmutable como identificador.** El nombre visible cambia sin romper
   referencias. `template_origin` se persiste como `null` en esta fase.
3. **Motor jerárquico extensible.** `ConfigKey` enum + `default_value()`;
   añadir claves para mods (3-B) no toca la lógica de resolución. El tema
   usa este mismo mecanismo, no una persistencia paralela.
4. **Java: `majorVersion`, no `component`.** Error propio detectado y
   corregido: `javaVersion.component` es un string (`jre-legacy`,
   `java-runtime-gamma`), no un número. La fuente autoritativa es
   `javaVersion.majorVersion` del JSON de versión de Mojang; la tabla estática
   es sólo fallback offline. También se corrigió el mapeo 1.18–1.20.4, que
   inicialmente decía Java 16 cuando es Java 17.
5. **Adoptium verificado contra documentación real.**
   `GET /v3/assets/feature_releases/{v}/ga` con `os`, `architecture`,
   `image_type=jdk`, `jvm_impl=hotspot`, `vendor=eclipse`; se lee
   `binaries[].package.{link,checksum}` y se valida SHA-256. Fallback a
   `/v3/binary/latest/{v}/ga/{os}/{arch}/jdk/hotspot/normal/eclipse`.
   Fuentes: [Adoptium cookbook](https://github.com/adoptium/api.adoptium.net/blob/main/docs/cookbook.adoc),
   [Adoptium CI scripts](https://adoptium.net/installation/ci-scripts),
   [Minecraft Wiki: Update Java](https://minecraft.wiki/w/Tutorial:Update_Java),
   [Client.json](https://minecraft.wiki/w/Client.json),
   [MCReference Java matrix](https://mcreference.com/guides/minecraft-java-version-matrix).
6. **Instalación atómica con marcador.** Defecto propio encontrado tarde:
   `ensure()` aceptaba cualquier `jdk-N/bin/java` existente, así que una
   descarga cortada podía dejar un JDK "válido" a medias. Ahora se escribe
   `.rf-launcher-ok` sólo tras el rename final y `is_installed()` exige el
   marcador; cualquier fallo borra staging, temporal y directorio de destino.
7. **Kill verificado, sin zombis.** Defecto propio corregido: un
   `std::mem::forget(child)` inicial dejaba zombis y hacía imposible verificar
   la muerte. Ahora se conserva el `Child`, se usa `try_wait()` (que **reaparece**
   al hijo), se escala a `kill()`/`taskkill` y se comprueba que el PID
   desapareció de la tabla de procesos.
8. **Cerrar el launcher NO cierra el juego.** Comportamiento esperado por el
   usuario de Minecraft. Contrapartida documentada: al reiniciar el launcher
   se pierde el registro de partidas en curso; "detener" sólo funciona sobre
   lo lanzado en esa sesión. Persistir PIDs entre sesiones queda para después.
9. **Tauri 1, no Tauri 2.** Error propio de configuración: se escribió primero
   un `tauri.conf.json` con claves de v2 (`frontendDist`, `capabilities`). Se
   reescribió al esquema v1 (`build.devPath`, `build.distDir`,
   `tauri.allowlist`, `tauri.security`, `tauri.windows`) y se eliminó
   `capabilities/default.json`. Los comandos devuelven errores que implementan
   `serde::Serialize`, requisito de Tauri 1.
10. **Iconos reales.** `generate_context!()` paniquea si faltan; se generaron
    PNG/ICO/ICNS válidos en `src-tauri/icons/`.
11. **RAM: error duro >100 %, warning >80 %.** Error propio de test: se había
    escrito un test que exigía rechazar exactamente el 100 %, contradiciendo la
    propia especificación ("no permitir **más** RAM de la disponible"). Se
    corrigió el test a `Warn` y se añadió el caso 32001/32000 como `Exceeds`.

---

## 7. Línea base de recursos

**Launcher en reposo, sin instancias corriendo** (Linux, `xvfb-run`, binario
release, medido a los 12 s; evidencia en anotación `evidence-ram` de Ubuntu):

```
Name:   rf-launcher
VmPeak:  538208 kB   (≈ 525.6 MiB de memoria virtual máxima)
VmSize:  538208 kB
VmHWM:    91500 kB   (≈ 89.4 MiB pico de RSS)
VmRSS:    91500 kB   (≈ 89.4 MiB residente)
ps:      4452 91500 538208 ./target/release/rf-launcher
```

**RAM física detectada por `sysinfo::System::new_all()` en cada SO**
(anotación `evidence-ram-physical`, imprimida por el test
`ram_detection_reads_real_physical_memory`):

| SO | `RAM_FISICA_MB` | Equivalente |
|---|---|---|
| Windows Server 2025 (10.0.26100) | 16378 | ≈ 16.0 GiB |
| macOS 26.6.2 arm64 | 7168 | 7.0 GiB |
| Ubuntu 22.04 (Linux 6.8.0-1064-azure) | 15988 | ≈ 15.6 GiB |

Sandbox local de desarrollo (referencia, **no** es measurement del launcher):
3.8 GiB de RAM, 2 CPU, Debian/Linux 6.1.158.

**Advertencia interpretativa:** 89 MiB de RSS incluye el runtime WebKit/GTK y
React, no es el coste de la lógica Rust aislada. Es una línea base de proceso
completo, medida con un único muestreo; no es un benchmark estadístico.

---

## 8. Checklist de escepticismo (6 puntos, respondido con evidencia)

**1. ¿Los 3 casos del motor de configuración están realmente probados?**
Sí. En `src-tauri/src/config.rs`: `resolve_instance_override_wins_over_global`,
`resolve_falls_back_to_global_when_instance_does_not_override` y
`resolve_falls_back_to_system_default_when_neither_defines_key`, más
`default_values_are_never_missing`. Los 4 pasaron en los 3 jobs (25/25).
No son tests de la UI: llaman directamente a `ConfigEngine::resolve`.
Limitación: prueban la resolución con `HashMap` en memoria, no la lectura del
`config.json` persistido desde el `AppState` (esa ruta sólo se ejerce en runtime).

**2. ¿"Matar instancia" verifica de verdad que el PID desapareció?**
Sí, con proceso real. `kill_really_terminates_process_and_pid_disappears` hace
spawn de un hijo (`sleep 60` en Unix, `ping` en Windows), comprueba
`process_exists(pid)` con `sysinfo`, envía señal, reapa con `try_wait()`,
escala a `kill()` y **falla el test si el PID sigue en la tabla de procesos**.
Evidencia literal en los 3 jobs:
`test launcher::tests::kill_really_terminates_process_and_pid_disappears ... ok`.
Matiz: se prueba sobre un proceso sustituto, no sobre un `java` de Minecraft
real (véase bloqueo 6).

**3. ¿El mapeo MC → Java tiene fuente real y se comprobó contra Mojang?**
Sí. Fuentes: Minecraft Wiki (`Tutorial:Update_Java`, `Client.json`) y
MCReference Java version matrix, citadas en `java.rs`. Se usa
`javaVersion.majorVersion` del JSON de Mojang como fuente autoritativa; la
tabla es fallback para offline. El smoke test
`mojang_manifest_major_version_matches_static_table` consultó el manifiesto de
Mojang para 1.16.5, 1.17, 1.20.1, 1.20.4, 1.20.5 y 1.21 y comparó con la
tabla: **1 passed en 0.84 s**. Matiz escéptico: el test usa
`if let Ok(major) = fetch_java_major_version(...)` — si la red falla, ese caso
se omite silenciosamente en lugar de fallar. La tabla estática sí se verifica
siempre. Es una debilidad conocida del smoke test, no una prueba de que la red
funcione siempre.

**4. ¿Qué pasa si la descarga de Java se corta a la mitad?**
Hay dos capas. (a) `verify_sha256` rechaza el artefacto truncado:
`truncated_download_fails_checksum` escribe 12 bytes de un payload de 48 y
comprueba que el SHA-256 del archivo completo NO valida. (b) `ensure()`
ejecuta todo en `.staging-{version}` y, ante cualquier error (red, checksum,
extracción), borra staging, el `.download-*` temporal y el `jdk-{version}`
de destino; el marcador `.rf-launcher-ok` sólo se escribe tras el rename
final. `interrupted_install_leaves_no_reusable_half_jdk` simula un binario a
medias sin marcador y comprueba que `is_installed()` es falso y que se limpian
temporal y staging. Ambos tests pasaron en los 3 SO. **Matiz honesto:** la
limpieza se prueba de forma determinista simulando el estado, no cortando una
conexión real a mitad de un JDK de 200 MB.

**5. ¿La RAM es la real del SO en los tres sistemas?**
Sí, y hay números distintos por SO, lo que descarta un valor hardcodeado:
Windows `RAM_FISICA_MB=16378`, macOS `7168`, Ubuntu `15988`, impresos por el
mismo `total_ram_mb()` (`sysinfo 0.32.1`, `System::new_all().total_memory()`)
en cada job. Las reglas derivadas se prueban con
`ram_exceeds_physical_is_rejected`, `ram_over_80_percent_warns`,
`ram_under_80_percent_is_ok` y
`ram_equal_to_physical_warns_but_is_not_rejected`. La UI muestra el aviso y
exige `force=true` para confirmar.

**6. ¿Es una "matrix falsa" verde a la primera?**
No. La branch acumula **27 runs: 22 fallidos, 4 verdes de la matriz real y 1
falso verde de diagnóstico**. Antes de la primera verde hubo 22 fallos con
causas reales: YAML inválido (`if:` sin comillas, `:` en nombres de paso),
`npm ci` ejecutándose en la raíz sin `package-lock.json`, errores de
compilación Rust (`uuid` feature `fmt` inexistente, `tar::TarError` privado,
`Serialize` faltante en el error de comando, `Child::id()` = `u32` no `Option`,
`setup` de Tauri 1, alias `Result` en `Serialize`, préstamos, iconos), y un
test de RAM mal planteado. Después vinieron 4 corridas verdes consecutivas de
la matriz completa: `40237313439`, `40237684461`, `40238204012` y
`40238629977`.

Además, al revisar el histórico apareció un `success` temprano
([40216745092](https://github.com/BlackBoneDel/RF-Launcher/actions/runs/40216745092))
que **no** es evidencia: es un workflow `Minimal test` con un único job
`hello`, creado para diagnosticar permisos de Actions. Se excluye del
recuento. La última corrida es rápida (~2 min) por caché de
`Swatinem/rust-cache@v2`, pero se verificó que tiene **3 jobs reales**, cada
uno con los pasos 7–10 (npm ci, build frontend, `cargo build --release`,
`cargo test`) en `success`, no `skipped`.

**Sospechas que quedan abiertas (no barridas bajo la alfombra):**

- La matriz usa `ubuntu-22.04`, no `ubuntu-latest`; es una excepción razonada,
  pero significa que no se ha verificado Ubuntu 24.04.
- El smoke test de red (descarga real de JDK 21 desde Adoptium + `java
  -version`) se ejecutó **sólo en Linux** (15.60 s, 1 passed). Windows y macOS
  verificaron compilación y tests unitarios, no descarga real.
- No hay prueba end-to-end de lanzar Minecraft con el cliente de Mojang.
- La línea base de RAM se midió en un único muestreo y sólo en Linux/Xvfb.
- El frontend se verificó con `tsc`/`vite build`, pero nunca se ejecutó contra
  un backend Tauri vivo dentro de este entorno (sin GUI).

---

## 9. Cómo reproducir

```bash
# Frontend
cd frontend && npm ci && npx tsc --noEmit && npm run build

# Backend (requiere Rust + dependencias de sistema de Tauri 1)
cd src-tauri && cargo build --release && cargo test

# Evidencia de la corrida final (anotaciones legibles por API)
gh api repos/BlackBoneDel/RF-Launcher/actions/runs/40238629977/jobs
gh api repos/BlackBoneDel/RF-Launcher/check-runs/121305448272/annotations
```

Documentación de arquitectura: [`README.md`](README.md) ·
Workflow: [`.github/workflows/ci.yml`](.github/workflows/ci.yml).
