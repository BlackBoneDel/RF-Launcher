use serde::Serialize;
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
#[derive(Debug, Clone, Serialize)]
pub struct RamCheck {
    pub total_mb: u64,
    pub ok: bool,
    pub warning: Option<String>,
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
/// `force` es obligatorio en el cable: sin él, el aviso >80% de RAM era un
/// callejón sin salida (se devolvía `ok:false` pero no había forma de
/// confirmar). Tauri 2 mantiene `force` en JS (es una sola palabra, no cambia).
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
        match validate_ram_max(requested as u64, total) {
            RamValidation::Exceeds { requested_mb, total_mb } => {
                return Err(LauncherError::RamExceedsPhysical {
                    requested: requested_mb,
                    total: total_mb,
                }
                .into())
            }
            RamValidation::Warn { requested_mb, total_mb } => {
                // Sin confirmación explícita se devuelve el aviso y NO se
                // guarda. Con `force=true` se guarda igual que en la ruta por
                // instancia (`set_instance_config`), para que el aviso sea
                // realmente confirmable y no un mensaje muerto.
                if !force {
                    return Ok(RamCheck {
                        total_mb,
                        ok: false,
                        warning: Some(format!(
                            "Se solicitan {requested_mb} MB ({}% de la RAM física de {total_mb} MB). ¿Confirmar?",
                            (requested_mb as f64) * 100.0 / (total_mb as f64)
                        )),
                    });
                }
            }
            RamValidation::Ok => {}
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
        if requested as u64 > total {
            return Err(LauncherError::RamExceedsPhysical {
                requested: requested as u64,
                total,
            }
            .into());
        }
        if !force && matches!(validate_ram_max(requested as u64, total), RamValidation::Warn { .. }) {
            return Ok(RamCheck {
                total_mb: total,
                ok: false,
                warning: Some(format!(
                    "Se solicitan {} MB ({}% de la RAM física de {} MB). Confirma para continuar.",
                    requested,
                    (requested as f64) * 100.0 / (total as f64),
                    total
                )),
            });
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
    if ram_max > total {
        return Err(LauncherError::RamExceedsPhysical {
            requested: ram_max,
            total,
        }
        .into());
    }
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
#[derive(Debug, Clone, Serialize)]
pub struct LaunchResult {
    pub ok: bool,
    pub pid: Option<u32>,
    pub error: Option<String>,
}


