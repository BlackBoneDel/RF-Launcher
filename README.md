# RF Launcher

Launcher de Minecraft independiente: **Tauri (Rust nativo) + React/TypeScript/Vite**.

Gestiona **instancias estándar** (Minecraft vanilla + Fabric Loader opcional), siguiendo el
modelo de Prism Launcher / MultiMC. No distribuye versiones parcheadas de Minecraft.

> **Fase 3-A — Núcleo de gestión de instancias.** Sin instalación de mods (3-B),
> sin sistema de cuentas (3-C), sin plantillas predefinidas. `template_origin`
> existe en el modelo de datos y se persiste, pero no se usa todavía.

## Arquitectura

```
frontend/            React + TS + Vite: capa de presentación DELGADA.
                     Toda llamada al núcleo es `invoke()` a un comando Tauri.
src-tauri/
  src/
    main.rs          Arranque de Tauri, estado compartido, registro de comandos.
    commands.rs      Frontera Tauri (11 comandos). SIN lógica de negocio.
    config.rs        Motor de configuración jerárquica (instancia > global > defecto).
    instance.rs      Modelo de Instancia + persistencia JSON por instancia.
    java.rs          Gestión automática de Java (Adoptium) + mapeo MC -> Java.
    minecraft.rs     Resolución del lanzamiento (Mojang manifest, librerías, Fabric).
    ram.rs           RAM física del SO (sysinfo) + validación de límites.
    launcher.rs      Spawn, log por lanzamiento, terminación verificada.
    state.rs         Estado compartido (AppState).
    error.rs         Errores tipados + Serialize para la frontera Tauri.
  tauri.conf.json    Configuración en formato **Tauri 2.x**.
  capabilities/      Permisos IPC de la ventana principal.
  permissions/       Definición de los 11 comandos propios permitidos.
.github/workflows/   CI: matriz Windows / macOS / Linux.
```

**Regla de oro:** ninguna decisión de negocio vive en el frontend. El frontend
solo muestra datos y llama comandos.

## Persistencia en disco

Formato elegido: **JSON** (y no TOML) porque:

1. Los datos de Instancia se intercambian directamente con el frontend vía IPC
   de Tauri, que ya es JSON: mismo esquema en disco y en la frontera, sin
   conversión intermedia.
2. Los valores de configuración son un enum etiquetado (`ConfigValue`); serde
   los serializa de forma natural en JSON, mientras que en TOML los enums
   etiquetados anidados son más frágiles de leer/editar.
3. El manifiesto de versiones de Mojang y la API de Adoptium también son JSON,
   así que se reutiliza `serde_json` en todo el núcleo.

Estructura (bajo el `app_data_dir` de Tauri):

```
data/
  config.json                       Configuración global del launcher.
  instances/<uuid>/
    instance.json                   Datos de la instancia.
    minecraft/                      .minecraft AISLADO por instancia.
    logs/launch-<timestamp>.log     stdout+stderr de cada lanzamiento.
  java/jdk-<version>/               JDKs gestionados por el launcher (aislados del sistema).
  assets/versions/…, libraries/…    Archivos de Minecraft compartidos con checksum.
```

El identificador es un **UUID inmutable**: el nombre visible puede cambiar sin
romper referencias. `template_origin` se serializa como `null` en esta fase.

## Motor de configuración jerárquica

`ConfigEngine::resolve(instance_overrides, key)`:

```
override de la instancia  >  override global  >  defecto del sistema
```

Claves tipadas (`ConfigKey`): `RamMinMb`, `RamMaxMb`, `JavaAutoDownload`, `Theme`.
Añadir una clave nueva (por ejemplo, opciones de mods en 3-B) sólo requiere
extender el enum y su `default_value()`; **la lógica de resolución no se toca**.
Defectos del sistema: Xms 1024 MB, Xmx 2048 MB, descarga automática de Java
activada, tema claro. El tema visual se guarda en esta misma configuración
global (no en un mecanismo de persistencia paralelo).

## Java automático (Adoptium)

- La versión de Java se resuelve **primero** desde `javaVersion.majorVersion`
  del JSON de versión de Mojang (fuente autoritativa). `component` es un string
  (`jre-legacy`, `java-runtime-gamma`, …), no un número.
- Si no hay red, se usa una tabla estática documentada con fuentes:

  | Minecraft | Java |
  |---|---|
  | 1.0 – 1.16.5 | 8 |
  | 1.17 – 1.17.1 | 16 |
  | 1.18 – 1.20.4 | 17 |
  | 1.20.5 – 1.21.11 | 21 |
  | 26.1+ | 25 |

  Fuentes: [Minecraft Wiki: Update Java](https://minecraft.wiki/w/Tutorial:Update_Java),
  [Client.json](https://minecraft.wiki/w/Client.json),
  [MCReference Java version matrix](https://mcreference.com/guides/minecraft-java-version-matrix).

- Se consulta `GET /v3/assets/feature_releases/{version}/ga` de Adoptium con
  `os`, `architecture`, `image_type=jdk`, `jvm_impl=hotspot`, `vendor=eclipse`.
  Se verifica el **SHA-256** de `binaries[].package.checksum`. Si la API falla,
  se usa la URL directa `/v3/binary/latest/{version}/ga/{os}/{arch}/jdk/hotspot/normal/eclipse`.
  Fuentes: [Adoptium cookbook](https://github.com/adoptium/api.adoptium.net/blob/main/docs/cookbook.adoc),
  [Adoptium CI scripts](https://adoptium.net/installation/ci-scripts).
- Todo el trabajo se hace en `.staging-{version}`. Si la descarga se corta, el
  checksum falla o la extracción es inválida, se borran staging, el temporal y
  el directorio de destino: nunca queda un `jdk-{version}` a medio instalar.
- Los JDK gestionados viven en `data/java/`, nunca se toca el Java del sistema.

## RAM

`sysinfo::System::new_all()` da la RAM física real del SO (mismo crate en los
tres sistemas, verificado en la matriz de CI). Reglas:

- **Error duro:** **100 % o más de la RAM física se rechaza en duro**
  (`Xmx >= RAM física`); no se puede confirmar con `force`.
- **Advertencia blanda:** Xmx > 80 % y < 100 % de la RAM física → se avisa y
  se exige confirmación explícita (`force=true` en los comandos de
  configuración).
- Xms no puede superar Xmx.

## Lanzamiento y terminación

El comando `launch_instance` resuelve Java, RAM, librerías, classpath y
argumentos de Mojang y hace `spawn` real de `java`. `stdout`/`stderr` se
redirigen a `logs/launch-<timestamp>.log`.

`kill_instance` (criterio de éxito crítico): señal cortés (SIGTERM en Unix,
`taskkill` en Windows) → espera con `Child::try_wait()` (que **reaparece** el
hijo; sin esto quedaría un zombi) → escalada a `Child::kill()` → verificación
final de que el **PID desapareció de la tabla de procesos** del SO. Sólo entonces
se reporta éxito. Hay un test real (`kill_really_terminates_process_and_pid_disappears`)
que se ejecuta en los tres SO mediante la matriz de CI.

**Cierre del launcher con una instancia corriendo:** el juego **sigue vivo**.
Se conserva el `Child` en memoria, pero `drop(Child)` no mata el proceso: sólo
cierra el handle. Es el comportamiento esperado por cualquier usuario de
Minecraft (cerrar el launcher no debe cerrar la partida). Contrapartida
documentada: al reiniciar el launcher se pierde el registro de las partidas en
curso, por lo que "detener instancia" sólo funciona sobre lo lanzado en esa
sesión. Persistir PIDs entre sesiones queda para una fase posterior.

## Verificación

```bash
# Frontend (verificable localmente)
cd frontend && npm ci && npm run build

# Backend (requiere toolchain Rust + librerías de sistema de Tauri 2)
cd src-tauri && cargo build --release && cargo test
```

El workflow `.github/workflows/ci.yml` ejecuta la matriz
`windows-latest` / `macos-latest` / `ubuntu-24.04` con build del frontend,
`cargo build --release`, `cargo test`, un smoke test de descarga real de Java 21
desde Adoptium en los 3 SO, smoke estricto de arranque de la app en los 3 SO y
la medición de la línea base de RAM en reposo.

> **Runner Linux fijo:** Tauri 2 requiere `libwebkit2gtk-4.1-dev`, disponible en
> Ubuntu 24.04. Se usa la etiqueta inmóvil `ubuntu-24.04`, no `ubuntu-latest`,
> porque esta última puede pasar a Ubuntu 26.04 sin previo aviso en el workflow.
