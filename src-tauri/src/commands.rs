use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::config::{ConfigKey, ConfigValue};
use crate::error::{LauncherError, Result};
use crate::instance::Instance;
use crate::java::{JavaManager, java_version_for_minecraft_resolved};
use crate::launcher::KillResult;
use crate::minecraft::resolve as mc_resolve;
use crate::ram::{total_ram_mb, validate_ram_max, RamValidation};
use crate::state::AppState;

/// Comprobación de RAM devuelta al frontend tras escribir una configuración.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RamCheck {
    #[serde(alias = "total_mb")]
    pub total_mb: u64,
    pub ok: bool,
    pub warning: Option<String>,
}

/// Decisión pura y compartida por los tres call-sites que validan RAM:
/// `set_global_config`, `set_instance_config` y `launch_instance`.
///
/// `force` se recibe explícitamente para dejar claro —y poder probarlo— que
/// NUNCA puede confirmar el 100% o más de la RAM física. Las advertencias de
/// la franja >80% y <100% se construyen con el closure que facilita cada
/// comando, conservando el mensaje original de la UI.
enum RamConfigDecision {
    Accept,
    Warn(RamCheck),
}

fn decide_ram_config<F>(
    requested_mb: u64,
    total_mb: u64,
    force: bool,
    warning: F,
) -> Result<RamConfigDecision>
where
    F: FnOnce(u64, u64) -> String,
{
    match validate_ram_max(requested_mb, total_mb) {
        RamValidation::Exceeds {
            requested_mb,
            total_mb,
        } => Err(LauncherError::RamExceedsPhysical {
            requested: requested_mb,
            total: total_mb,
        }),
        RamValidation::Warn {
            requested_mb,
            total_mb,
        } if !force => Ok(RamConfigDecision::Warn(RamCheck {
            total_mb,
            ok: false,
            warning: Some(warning(requested_mb, total_mb)),
        })),
        _ => Ok(RamConfigDecision::Accept),
    }
}

#[tauri::command]
pub async fn get_theme(state: State<'_, AppState>) -> Result<String> {
    let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
    Ok(crate::config::resolve_theme(&inner.config)?)
}

#[tauri::command]
pub async fn set_theme(state: State<'_, AppState>, theme: String) -> Result<()> {
    let key = ConfigKey::Theme;
    let value = ConfigValue::String(theme);
    let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
    inner.config.set_global(key, value)?;
    Ok(())
}

#[tauri::command]
pub async fn list_instances(state: State<'_, AppState>) -> Result<Vec<Instance>> {
    let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
    Instance::list(&inner.instances_dir)
}

#[tauri::command]
pub async fn get_instance(
    state: State<'_, AppState>,
    id: String,
) -> Result<Instance> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| LauncherError::InvalidUuid(id.clone()))?;
    let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
    Instance::load(&inner.instances_dir, &uuid)
}

#[tauri::command]
pub async fn create_instance(
    state: State<'_, AppState>,
    name: String,
    minecraft_version: String,
    fabric_loader: Option<String>,
) -> Result<Instance> {
    let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
    let instance = Instance::create(
        &inner.instances_dir,
        name,
        minecraft_version,
        fabric_loader,
    )?;
    Ok(instance)
}

#[tauri::command]
pub async fn resolve_config(
    state: State<'_, AppState>,
    instance_id: String,
    key: String,
) -> Result<ConfigValue> {
    let uuid = Uuid::parse_str(&instance_id)
        .map_err(|_| LauncherError::InvalidUuid(instance_id.clone()))?;
    let config_key = ConfigKey::from_str(&key)
        .ok_or_else(|| LauncherError::UnknownConfigKey(key.clone()))?;
    let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
    let instance = Instance::load(&inner.instances_dir, &uuid)?;
    Ok(inner.config.resolve(&instance.config_overrides, config_key))
}

#[tauri::command]
/// `force` es obligatorio en el cable: sin él, el aviso >80% y <100% de RAM
/// era un callejón sin salida (se devolvía `ok:false` pero no había forma de
/// confirmar). El 100% o más se rechaza en duro. Tauri 2 mantiene `force`
/// en JS (es una sola palabra, no cambia).
pub async fn set_global_config(
    state: State<'_, AppState>,
    key: String,
    value: ConfigValue,
    force: bool,
) -> Result<RamCheck> {
    let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
    let config_key = ConfigKey::from_str(&key)
        .ok_or_else(|| LauncherError::UnknownConfigKey(key.clone()))?;

    // Validación de RAM contra la física antes de aceptar el valor global.
    if matches!(config_key, ConfigKey::RamMaxMb | ConfigKey::RamMinMb) {
        let requested = value.as_int().ok_or("RamMinMb/RamMaxMb deben ser enteros")?;
        if requested <= 0 {
            return Err("La RAM debe ser mayor a 0 MB".into());
        }
        let total = total_ram_mb();
        // Decisión compartida: 100% o más es error duro incluso con `force`.
        match decide_ram_config(
            requested as u64,
            total,
            force,
            |requested_mb, total_mb| {
                format!(
                    "Se solicitan {requested_mb} MB ({}% de la RAM física de {total_mb} MB). ¿Confirmar?",
                    (requested_mb as f64) * 100.0 / (total_mb as f64)
                )
            },
        )? {
            RamConfigDecision::Warn(check) => return Ok(check),
            RamConfigDecision::Accept => {}
        }
    }

    inner.config.set_global(config_key, value)?;
    Ok(RamCheck {
        total_mb: total_ram_mb(),
        ok: true,
        warning: None,
    })
}

#[tauri::command]
pub async fn set_instance_config(
    state: State<'_, AppState>,
    instance_id: String,
    key: String,
    value: ConfigValue,
    force: bool,
) -> Result<RamCheck> {
    let uuid = Uuid::parse_str(&instance_id)
        .map_err(|_| LauncherError::InvalidUuid(instance_id.clone()))?;
    let config_key = ConfigKey::from_str(&key)
        .ok_or_else(|| LauncherError::UnknownConfigKey(key.clone()))?;
    let total = total_ram_mb();

    if matches!(config_key, ConfigKey::RamMaxMb | ConfigKey::RamMinMb) {
        let requested = value.as_int().ok_or("RamMinMb/RamMaxMb deben ser enteros")?;
        if requested <= 0 {
            return Err("La RAM debe ser mayor a 0 MB".into());
        }
        // Regresión de la ruta alternativa: antes este call-site sólo miraba
        // `Warn` y un 100% exacto (`Exceeds`) caía hasta `instance.save()`.
        // Ahora delega en la misma decisión que los otros dos call-sites.
        match decide_ram_config(
            requested as u64,
            total,
            force,
            |requested_mb, total_mb| {
                format!(
                    "Se solicitan {} MB ({}% de la RAM física de {} MB). Confirma para continuar.",
                    requested_mb,
                    (requested_mb as f64) * 100.0 / (total_mb as f64),
                    total_mb
                )
            },
        )? {
            RamConfigDecision::Warn(check) => return Ok(check),
            RamConfigDecision::Accept => {}
        }
    }

        let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
        let mut instance = Instance::load(&inner.instances_dir, &uuid)?;
    instance.set_override(config_key, value.clone());
    instance.save(&inner.instances_dir)?;
    Ok(RamCheck {
        total_mb: total,
        ok: true,
        warning: None,
    })
}

#[tauri::command]
pub async fn get_system_ram(state: State<'_, AppState>) -> Result<RamCheck> {
    let total = total_ram_mb();
    Ok(RamCheck {
        total_mb: total,
        ok: true,
        warning: None,
    })
}

#[tauri::command]
pub async fn launch_instance(state: State<'_, AppState>, id: String) -> Result<LaunchResult> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| LauncherError::InvalidUuid(id.clone()))?;

    // 1. Leer datos de la instancia: bloqueo breve, no cruza ningún .await.
    let (instance, assets_dir, instances_dir) = {
        let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
        let instance = Instance::load(&inner.instances_dir, &uuid)?;
        (
            instance,
            inner.assets_dir.clone(),
            inner.instances_dir.clone(),
        )
    };

    // 2. Java: versión requerida y descarga automática si falta.
    //    (puede tardar segundos; nunca mantenemos bloqueado AppState).
    let java_dir = {
        let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
        inner.java_dir.clone()
    };
    let java_version = java_version_for_minecraft_resolved(&instance.minecraft_version).await?;
    let java_manager = JavaManager::new(java_dir);
    let java_path = java_manager.ensure(java_version).await?;

    // 3. RAM: resolución jerárquica e invalidación contra la RAM física.
    let (ram_min, ram_max) = {
        let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
        let rm = inner
            .config
            .resolve(&instance.config_overrides, ConfigKey::RamMinMb);
        let xm = inner
            .config
            .resolve(&instance.config_overrides, ConfigKey::RamMaxMb);
        (rm.as_int().unwrap_or(1024) as u64, xm.as_int().unwrap_or(2048) as u64)
    };
    let total = total_ram_mb();
    // En el lanzamiento las advertencias ya fueron confirmadas al guardar,
    // así que `force=true`; aun así, 100% o más sigue siendo error duro.
    let _ = decide_ram_config(ram_max, total, true, |_, _| String::new())?;
    if ram_min > ram_max {
        return Err("La RAM mínima no puede superar la RAM máxima".into());
    }

    // 4. Datos de lanzamiento de Minecraft (librerías, classpath, argumentos).
    let resolved = mc_resolve(
        &assets_dir,
        &instances_dir,
        &uuid,
        &instance.minecraft_version,
        instance.fabric_loader.as_deref(),
    )
    .await?;

    // 5. Spawn del proceso y actualización de marca de último lanzamiento.
    let pid = {
        let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
        let mut launcher = inner
            .launcher
            .lock()
            .map_err(|e| LauncherError::Custom(e.to_string()))?;
        if launcher.is_running(&uuid) {
            return Err(format!("la instancia {id} ya está en ejecución").into());
        }
        let mut instance = Instance::load(&inner.instances_dir, &uuid)?;
        let pid = launcher.spawn(
            &instance,
            &java_path,
            ram_min,
            ram_max,
            &resolved.working_dir,
            &resolved.classpath,
            &resolved.main_class,
            &resolved.jvm_args,
            &resolved.game_args,
            &resolved.log_file,
        )?;
        instance.mark_launched();
        instance.save(&inner.instances_dir)?;
        pid
    };

    Ok(LaunchResult {
        ok: true,
        pid: Some(pid),
        error: None,
    })
}

#[tauri::command]
pub async fn kill_instance(state: State<'_, AppState>, id: String) -> Result<KillResult> {
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| LauncherError::InvalidUuid(id.clone()))?;
    let inner = state.inner.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
    let mut launcher = inner.launcher.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
    Ok(launcher.kill(&uuid))
}

/// Resultado de lanzar una instancia.
///
/// SÓLO IPC: se devuelve al frontend y nunca se serializa a un archivo, caché
/// o log. Sus claves son palabras simples (`ok`, `pid`, `error`), por lo que
/// nunca existió una versión legacy snake_case y no necesita `alias`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub ok: bool,
    pub pid: Option<u32>,
    pub error: Option<String>,
}

#[cfg(test)]
mod wire_tests {
    use super::{LaunchResult, RamCheck};

    #[test]
    fn ram_check_wire_fields_are_camel_case_and_legacy_alias_works() {
        let check = RamCheck {
            total_mb: 16378,
            ok: false,
            warning: Some("Aviso".into()),
        };
        let obj = serde_json::to_value(&check).unwrap();
        let keys: Vec<&str> = obj.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(obj["totalMb"], 16378);
        assert_eq!(obj["ok"], false);
        assert_eq!(obj["warning"], "Aviso");
        assert!(!obj.as_object().unwrap().contains_key("total_mb"));
        assert!(keys.iter().all(|k| !k.contains('_')), "claves: {keys:?}");

        let legacy = serde_json::json!({
            "total_mb": 8192,
            "ok": true,
            "warning": null
        });
        let back: RamCheck = serde_json::from_value(legacy).unwrap();
        assert_eq!(back.total_mb, 8192);
        assert!(back.ok);
        assert_eq!(back.warning, None);
    }

    #[test]
    fn launch_result_wire_fields_are_camel_case() {
        let result = LaunchResult {
            ok: true,
            pid: Some(4242),
            error: None,
        };
        let obj = serde_json::to_value(&result).unwrap();
        let keys: Vec<&str> = obj.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(obj["ok"], true);
        assert_eq!(obj["pid"], 4242);
        assert_eq!(obj["error"], serde_json::Value::Null);
        assert!(
            keys.iter().all(|k| !k.contains('_')),
            "ningún campo snake_case en LaunchResult: {keys:?}"
        );

        let back: LaunchResult = serde_json::from_value(obj).unwrap();
        assert!(back.ok);
        assert_eq!(back.pid, Some(4242));
        assert_eq!(back.error, None);
    }
}



#[cfg(test)]
mod ram_call_site_tests {
    use super::*;

    /// Cubre la ruta alternativa que motivó el hallazgo. Si `validate_ram_max`
    /// vuelve a `>`, o si `force` vuelve a poder confirmar el 100%, este test
    /// falla. También comprueba que la franja de aviso sigue intacta.
    #[test]
    fn set_instance_config_path_rejects_100_percent_even_with_force() {
        let total = 32000u64;
        for requested in [total, total + 1] {
            for force in [false, true] {
                let decision = decide_ram_config(requested, total, force, |_, _| String::new());
                assert!(
                    matches!(decision, Err(LauncherError::RamExceedsPhysical { .. })),
                    "requested={requested} total={total} force={force} debe rechazarse en duro"
                );
            }
        }

        let warning = decide_ram_config(26000, total, false, |_, _| String::new()).unwrap();
        assert!(matches!(warning, RamConfigDecision::Warn(_)));
        let confirmed = decide_ram_config(26000, total, true, |_, _| String::new()).unwrap();
        assert!(matches!(confirmed, RamConfigDecision::Accept));
    }

    /// Fija que los TRES call-sites delegan en `decide_ram_config`. Si alguien
    /// reintroduce la comparación duplicada `> total` en `set_instance_config`
    /// o en `launch_instance`, este test falla aunque el validador puro siga
    /// bien. Es una guardia estática deliberada sobre este mismo fichero.
    #[test]
    fn every_ram_call_site_delegates_to_shared_decision() {
        let source = include_str!("commands.rs");
        // Sólo inspeccionamos la parte de producción: los propios literales de
        // este test no deben hacer falsos positivos en la guardia estática.
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        let sections = [
            (
                "set_global_config",
                "pub async fn set_global_config(",
                "pub async fn set_instance_config(",
            ),
            (
                "set_instance_config",
                "pub async fn set_instance_config(",
                "pub async fn get_system_ram(",
            ),
            (
                "launch_instance",
                "pub async fn launch_instance(",
                "pub async fn kill_instance(",
            ),
        ];

        for (name, start_marker, end_marker) in sections {
            let start = production
                .find(start_marker)
                .unwrap_or_else(|| panic!("no se encontró {name}"));
            let tail = &production[start..];
            let end = tail.find(end_marker).unwrap_or(tail.len());
            let section = &tail[..end];
            assert!(
                section.contains("decide_ram_config"),
                "{name} debe delegar en la decisión compartida de RAM"
            );
        }

        assert!(
            !production.contains("requested as u64 > total"),
            "comparación duplicada reintroducida en un comando de configuración"
        );
        assert!(
            !production.contains("ram_max > total"),
            "comparación duplicada reintroducida en launch_instance"
        );
    }
}
