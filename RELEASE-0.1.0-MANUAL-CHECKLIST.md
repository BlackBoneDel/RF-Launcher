# Checklist de prueba manual — RF Launcher v0.1.0

> Esta checklist existe porque **un CI verde no demuestra que un instalador sea usable**.
> Ejecútala en cada sistema operativo con el artefacto descargado del Release v0.1.0.
> Repo público: no hace falta iniciar sesión para descargar, sí para comentar incidencias.

## 0. Antes de empezar

- [ ] Descargar el instalador desde el Release `v0.1.0`.
- [ ] Verificar SHA-256 local contra `SHA256SUMS.txt` del propio Release.
- [ ] Anotar SO, versión, arquitectura y si es una máquina limpia o ya tenía una versión previa.
- [ ] Tener conexión a internet: la descarga automática de Java usa Adoptium.

## 1. Instalación limpia

### Windows

- [ ] Ejecutar el `.msi` o el `-setup.exe`.
- [ ] Si SmartScreen muestra “Windows protegió su PC”, usar **Más información → Ejecutar de todos modos**. Es esperado: el instalador **no está firmado**.
- [ ] Instalar y abrir RF Launcher desde el menú de inicio.
- [ ] Confirmar que el icono y el nombre “RF Launcher” aparecen correctamente.

### macOS

- [ ] Abrir el `.dmg` y arrastrar `RF Launcher.app` a Aplicaciones.
- [ ] Si Gatekeeper bloquea la app sin firmar, abrir con **clic derecho → Abrir**, o quitar la cuarentena:
      `xattr -dr com.apple.quarantine "/Applications/RF Launcher.app"`.
- [ ] Abrir la app y confirmar que el icono del Dock es legible.

### Linux

- [ ] Instalar el `.deb`: `sudo apt install ./linux-x86_64-RF-Launcher_0.1.0_amd64.deb`.
- [ ] Abrir RF Launcher desde el menú de aplicaciones o ejecutarlo desde terminal.
- [ ] Confirmar que el icono del escritorio y el nombre aparecen correctamente.

## 2. Arranque y tema

- [ ] La app arranca sin ventana en blanco y sin error de permisos/capabilities.
- [ ] Cambiar tema a oscuro en Configuración.
- [ ] Cerrar y reabrir la app: el tema debe persistir.
- [ ] Volver a tema claro y repetir la persistencia.

## 3. Instancias

- [ ] Crear una instancia vanilla, por ejemplo `Manual Vanilla 1.20.1`.
- [ ] Crear una segunda instancia con Fabric Loader, por ejemplo `Manual Fabric 1.20.1`.
- [ ] Verificar que ambas aparecen en la lista tras refrescar.
- [ ] Verificar que cada instancia tiene su carpeta aislada bajo el directorio de datos de la app.
- [ ] Cerrar y reabrir la app: las instancias deben persistir.

## 4. Configuración jerárquica y RAM

- [ ] En Configuración global, fijar RAM máxima en un valor seguro (por ejemplo 2048 MB).
- [ ] Crear/abrir una instancia y fijar un override de RAM distinto.
- [ ] Verificar que el override de la instancia gana sobre el global.
- [ ] Probar un valor >80 % y <100 % de la RAM física: debe aparecer aviso y el botón **Confirmar de todos modos (force)**.
- [ ] Probar exactamente el 100 % de la RAM física: debe devolver **error duro**, no un aviso confirmable.
- [ ] Probar 100 % + 1 MB: debe devolver **error duro**.
- [ ] Confirmar que un valor rechazado no se guarda en `instance.json` ni reaplica tras reiniciar.

## 5. Java automático vía Adoptium

- [ ] Con una instancia vanilla 1.20.1, pulsar **Iniciar**.
- [ ] Observar que, si no existe JDKcompatible, se descarga desde Adoptium y se valida SHA-256.
- [ ] Verificar que el proceso Java se lanza con PID real.
- [ ] Revisar el log de lanzamiento de la instancia y comprobar que no hay error de “Java no encontrado”.
- [ ] Repetir con una versión antigua (por ejemplo 1.16.5) y otra moderna (1.21) si es posible.

## 6. Terminación de procesos

- [ ] Con una instancia en ejecución, pulsar **Detener**.
- [ ] Verificar que la UI reporta éxito y que el PID ya no aparece en el administrador de tareas / `ps`.
- [ ] Confirmar que no queda un proceso Java huérfano.
- [ ] Cerrar el launcher con una instancia en ejecución y comprobar que el juego sigue vivo (comportamiento esperado documentado).

## 7. Límites que esta prueba manual debe confirmar

- [ ] No hay funcionalidad de mods, modpacks ni loaders adicionales más allá de Fabric básico.
- [ ] No hay sistema de cuentas.
- [ ] La app no intenta descargar contenido de RF Client ni de un repositorio hermano.
- [ ] Los instaladores sin firmar sólo generan advertencias del SO; la app funciona tras la confirmación manual.

## 8. Reporte de resultados

Para cada ítem, anotar:

- [ ] ✅ funciona como se describe.
- [ ] ⚠️ funciona con salvedades (detallar).
- [ ] ❌ falla (adjuntar SO, versión, log y captura).

**Criterio de aceptación:** todas las secciones 1–6 en ✅ o ⚠️ documentadas; cualquier ❌ en arranque, RAM 100 %, Java automático o kill debe considerarse bloqueante para Fase 3-B.
