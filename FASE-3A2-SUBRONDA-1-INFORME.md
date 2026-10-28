# Fase 3-A.2 — Sub-ronda 1: informe de trabajo en curso

**Estado: detenida, sin certificación E2E de los seis bloqueantes.** No cierra la Fase 3-A.2. Los módulos H, I/J, K, L, M, N y O–S siguen pendientes. No se recomienda merge ni retest manual todavía.

## Evidencia previa

El run [40690208391](https://github.com/BlackBoneDel/RF-Launcher/actions/runs/40690208391) sobre `f32a4638291415c0b723517caf6ecee3c7d9e7d0` (véase el SHA exacto en GitHub) tuvo matriz Linux/Windows/macOS verde y `e2e-linux` rojo: la sesión WebDriver se abrió, pero el WebView mostró `Could not connect to localhost: Connection refused` antes de ejecutar los casos. Ningún caso de bloqueante quedó certificado. El job corrió `npm run build` antes de `cargo build --release --features e2e`.

El modo Tauri depende de la feature `tauri/custom-protocol`, no de `--release`: en `tauri` 2.12.5, build.rs define `dev = !custom_protocol`; `tauri-build` 2.7.4 utiliza `DEP_TAURI_DEV`. La compilación anterior no activaba esa feature y la configuración incluye `devUrl=http://localhost:1420`. La corrección y la presencia del contenido en el WebView deben validarse mediante un nuevo run. No se ha demostrado todavía el contenido de `dist` en el momento del build anterior; el nuevo job registra sus archivos y hash.

| Bloqueante | Estado | Evidencia y límite |
|---|---|---|
| Editar | PARCIAL | Comando y UI implementados; sin E2E ejecutado con éxito. |
| Eliminar | PARCIAL | Implementación de dos fases y pruebas de fallo agregadas; pendiente de ejecución y validación E2E. |
| Override | PARCIAL | Sincronización y UI agregadas; DOM sin verificar en E2E. |
| Java y progreso | PARCIAL | Eventos y UI agregados; progreso no observado aún en WebView; mutación no ejecutada. |
| Lanzar | NO RESUELTO | El fixture es un script, no una JVM ni Minecraft; no prueba classpath/main class Fabric ni logs reales. Mutación no ejecutada. |
| Matar | NO RESUELTO | No se ha lanzado ni matado Minecraft real mediante UI. |

La suite propuesta no mockea `invoke`, pero no ha completado su flujo. No es evidencia de que las pruebas pasen. No hay prueba de mutación ejecutada. Cuando exista evidencia de una JVM real con classpath y main class de Fabric y log de arranque, debe distinguirse `E2E-PROCESS-VERIFIED / PLAYABILITY-PENDING-MANUAL`; hoy no se alcanzó.

## Catálogos y borrado

Las fixtures incluyen metadatos y SHA-256, y son recortadas. El verificador offline informó `LIVE_PROBE=SKIPPED` y `FABRIC_PROBE=SKIPPED`; un run anterior registró HTTP 200 para Mojang y Fabric. La implementación nueva intenta recuperar caché stale antes de usar fixture; las pruebas añadidas para ambos escenarios aún requieren ejecución. La caché es una capa interna, no los selectores de la sub-ronda 2.

La eliminación cambia el nombre de la carpeta a `.trash-<uuid>-<uuid>` antes de borrar. Si falla el renombrado, no se llama a eliminar; si falla el borrado posterior, se informa la ruta de basura, que puede contener datos parcialmente eliminados. Se añadieron pruebas de fallos simulados, aún pendientes de ejecución. No se garantiza rollback después de iniciada la eliminación recursiva.

## Otras limitaciones

- El feature flag `e2e` habilita hooks de test; no hay evidencia por inspección binaria de exclusión de código de test en Release.
- El tiempo incremental de CI aún no está medido.
- El comentario de placeholders `auth_*` está en `minecraft.rs`; no se implementó sistema de cuentas.
- La checklist manual de Windows/macOS queda pendiente hasta que el flujo sea seguro y verificable; no solicitar prueba sobre datos reales mientras existan fallos sin resolver.
- La implementación de `Launcher::kill` elimina el proceso del registro antes de verificar la terminación; un fallo de terminación puede perder el seguimiento. Esto requiere revisión antes de confiar en borrado o kill.
